//! Bounded, acknowledged local administration. Requests contain operations, never executable code.
use super::IngameRuntime;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}
fn request_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate(request: &Value, id: &str, now: f64) -> Result<(), String> {
    if !request_id(id) || request["id"].as_str() != Some(id) {
        return Err("request ID mismatch".into());
    }
    let created = request["createdAt"]
        .as_f64()
        .ok_or("missing creation time")?;
    if !created.is_finite() || now - created > 30.0 || created - now > 5.0 {
        return Err("request expired; no action performed".into());
    }
    if request["pid"].as_u64() != Some(std::process::id() as u64) {
        return Err("runtime process changed; refresh first".into());
    }
    Ok(())
}
fn write_result(path: &Path, result: &Value) -> Result<(), String> {
    shroudforge_loader_config::config::write_json(path, result)
}
impl IngameRuntime {
    pub(super) fn process_admin_requests(&mut self) {
        let dir =
            shroudforge_loader_config::paths::runtime_dir(&self.root).join("admin/shroudforge");
        let _ = fs::create_dir_all(&dir);
        let mods = self.configurations.iter().filter_map(|(id, package, _)| {
            let manifest = mod_loader::config::read_manifest_path(&self.root, package).ok()?;
            let revision = mod_loader::config_revision(&json!({"enabled":manifest.enabled,"settings":manifest.setting_values}));
            Some(json!({"id":id,"name":manifest.name,"enabled":manifest.enabled,"settings":manifest.settings,
                "values":manifest.setting_values,"capabilities":manifest.capabilities,"revision":revision,
                "active":self.active_mod_ids().contains(id),"error":self.errors.get(id)}))
        }).collect::<Vec<_>>();
        let _ = write_result(
            &dir.join("state.json"),
            &json!({"schemaVersion":1,"pid":std::process::id(),
            "updatedAt":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64(),"mods":mods}),
        );
        let Ok(entries) = fs::read_dir(dir.join("requests")) else {
            return;
        };
        if fs::create_dir_all(dir.join("results")).is_err() {
            return;
        }
        for entry in entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .take(4)
        {
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|x| x.to_str())
                .map(str::to_owned)
                .filter(|x| request_id(x))
            else {
                continue;
            };
            let output = dir.join("results").join(format!("{id}.json"));
            if output.exists() {
                let _ = fs::remove_file(&path);
                continue;
            }
            // Claim before executing: a crash leaves an uncertain request, never an automatic replay.
            let claimed = path.with_extension("processing");
            if fs::rename(&path, &claimed).is_err() {
                continue;
            }
            let result = (|| {
                if fs::metadata(&claimed).map_err(|e| e.to_string())?.len() > 65536 {
                    return Err("request too large".into());
                }
                let request: Value =
                    serde_json::from_slice(&fs::read(&claimed).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();
                validate(&request, &id, now)?;
                self.admin_operation(&request)
            })();
            let response = match result {
                Ok(data) => json!({"id":id,"ok":true,"data":data}),
                Err(error) => json!({"id":id,"ok":false,"data":{"error":error}}),
            };
            if write_result(&output, &response).is_ok() {
                let _ = fs::remove_file(&claimed);
            }
        }
    }
    fn admin_operation(&mut self, request: &Value) -> Result<Value, String> {
        let op = request["op"].as_str().ok_or("missing operation")?;
        if op == "ping" {
            return Ok(
                json!({"protocol":1,"pid":std::process::id(),"active":self.active_mod_ids()}),
            );
        }
        let id = request["modId"]
            .as_str()
            .filter(|x| identifier(x))
            .ok_or("invalid mod ID")?;
        if !self.configurations.iter().any(|(known, _, _)| known == id) {
            return Err("unknown mod ID".into());
        }
        if matches!(op, "reload" | "unload") {
            let manifest = self
                .configurations
                .iter()
                .find(|(known, _, _)| known == id)
                .unwrap();
            let caps = manifest.2["capabilities"].as_array();
            if caps.is_some_and(|c| {
                c.iter()
                    .any(|v| v == "patch" || v == "runtime-register-dll")
            }) {
                return Err("Asset patches and native DLLs require a server restart; Lua reload does not unload engine hooks".into());
            }
            if op == "reload" {
                self.reload_runtime_packages(&[id.to_owned()])?;
            } else {
                self.unload_runtime_packages(&[id.to_owned()])?;
            }
            return Ok(json!({"operation":op,"active":self.active_mod_ids()}));
        }
        let revision = request["revision"]
            .as_str()
            .ok_or("missing configuration revision")?;
        match op {
            "mod.enabled" => mod_loader::set_mod_enabled(
                &self.root,
                id,
                request["enabled"]
                    .as_bool()
                    .ok_or("enabled must be boolean")?,
                revision,
            )?,
            "mod.settings" => {
                mod_loader::save_mod_settings(&self.root, id, &request["values"], revision)?
            }
            _ => return Err("unknown administration operation".into()),
        }
        self.refresh_configuration();
        self.publish_status(true);
        Ok(json!({"saved":true,"active":self.active_mod_ids(),"errors":self.errors}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_cross_process_and_malformed_requests_are_rejected() {
        let id = "0123456789abcdef0123456789abcdef";
        let mut v = json!({"id":id,"createdAt":100.0,"pid":std::process::id()});
        assert!(validate(&v, id, 105.0).is_ok());
        assert!(validate(&v, id, 131.0).is_err());
        assert!(validate(&v, "../../escape", 105.0).is_err());
        v["pid"] = 0.into();
        assert!(validate(&v, id, 105.0).is_err());
    }
}
