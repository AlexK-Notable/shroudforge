//! Recoverable publication for one parsed KFC file set.
//!
//! Mods only ever write into a private staging directory. `commit` publishes
//! the validated set; `recover` restores the previous complete set after an
//! interrupted publication.

use std::{
    fs, io,
    io::Read,
    path::{Path, PathBuf},
};

fn invalid() -> io::Error {
    io::Error::other("invalid ShroudForge asset transaction")
}

// Never follow operator/cache-controlled links while cleaning or restoring an interrupted transaction.
fn metadata(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(invalid());
                }
            }
            if meta.file_type().is_symlink() {
                return Err(invalid());
            }
            Ok(Some(meta))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn regular(path: &Path, required: bool) -> io::Result<()> {
    match metadata(path)? {
        Some(meta) if meta.is_file() => Ok(()),
        None if !required => Ok(()),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "missing transaction file",
        )),
        _ => Err(invalid()),
    }
}

fn stage_path(root: &Path, stem: &str) -> io::Result<PathBuf> {
    if stem.is_empty()
        || stem.len() > 128
        || !stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(invalid());
    }
    let stage = directory(root, stem);
    if metadata(&stage)?.is_some_and(|meta| !meta.is_dir()) {
        return Err(invalid());
    }
    Ok(stage)
}

fn sync(path: &Path) -> io::Result<()> {
    fs::OpenOptions::new().write(true).open(path)?.sync_all()
}

pub fn directory(root: &Path, stem: &str) -> PathBuf {
    root.join(format!(".{stem}.shroudforge-stage"))
}

fn valid_name(stem: &str, name: &str) -> bool {
    name == format!("{stem}.kfc")
        || name == format!("{stem}.kfc_resources")
        || name
            .strip_prefix(&format!("{stem}_"))
            .and_then(|value| value.strip_suffix(".dat"))
            .is_some_and(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
            })
}

fn records(stage: &Path, stem: &str) -> io::Result<Vec<(String, bool)>> {
    const MAX_JOURNAL: u64 = 8 * 1024 * 1024;
    let journal = stage.join("pending.json");
    regular(&journal, true)?;
    let mut bytes = Vec::new();
    fs::File::open(journal)?
        .take(MAX_JOURNAL + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_JOURNAL {
        return Err(invalid());
    }
    let records: Vec<(String, bool)> = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let mut names = std::collections::HashSet::new();
    if records.len() > 16384
        || records
            .iter()
            .any(|(name, _)| !valid_name(stem, name) || !names.insert(name))
    {
        return Err(invalid());
    }
    Ok(records)
}

fn clean(stage: &Path) -> io::Result<()> {
    match metadata(stage)? {
        None => return Ok(()),
        Some(meta) if meta.is_dir() => {}
        _ => return Err(invalid()),
    }
    // Validate the complete directory before deleting its first file.
    let mut files = Vec::new();
    for entry in fs::read_dir(stage)? {
        let entry = entry?;
        regular(&entry.path(), true)?;
        files.push(entry.path());
    }
    for file in files {
        fs::remove_file(file)?;
    }
    fs::remove_dir(stage)
}

fn replace(source: &Path, target: &Path) -> io::Result<()> {
    regular(source, true)?;
    regular(target, false)?;
    // Removing first leaves an open Windows file delete-pending and prevents
    // reuse of its path. A single replacement also preserves the old file when
    // publication fails, so recovery can be retried after a foreign lock closes.
    fs::rename(source, target).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "replace asset {} from {}: {error}",
                target.display(),
                source.display()
            ),
        )
    })
}

pub fn recover(root: &Path, stem: &str) -> io::Result<()> {
    let stage = stage_path(root, stem)?;
    if metadata(&stage.join("pending.json"))?.is_some() {
        let records = records(&stage, stem)?;
        regular(&stage.join("completed.json"), false)?;
        // Refuse every unsafe input before mutating any live asset.
        for (name, existed) in &records {
            regular(&root.join(name), false)?;
            if *existed {
                regular(&stage.join(format!("{name}.rollback")), true)?;
                regular(&stage.join(format!("{name}.restore")), false)?;
            }
        }
        for (name, existed) in records {
            let target = root.join(&name);
            if existed {
                let restore = stage.join(format!("{name}.restore"));
                fs::copy(stage.join(format!("{name}.rollback")), &restore)?;
                sync(&restore)?;
                replace(&restore, &target)?;
            } else if target.exists() {
                fs::remove_file(target)?;
            }
        }
        fs::rename(stage.join("pending.json"), stage.join("completed.json"))?;
    }
    clean(&stage)
}

pub fn begin(root: &Path, stem: &str) -> io::Result<PathBuf> {
    recover(root, stem)?;
    let stage = stage_path(root, stem)?;
    fs::create_dir(&stage)?;
    Ok(stage)
}

pub fn abort(root: &Path, stem: &str) -> io::Result<()> {
    recover(root, stem)
}

fn prepare(root: &Path, stem: &str) -> io::Result<Vec<(String, bool)>> {
    let stage = stage_path(root, stem)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(&stage)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| invalid())?;
        regular(&entry.path(), true)?;
        if !valid_name(stem, &name) {
            return Err(invalid());
        }
        regular(&root.join(&name), false)?;
        files.push(name);
        if files.len() > 16384 {
            return Err(invalid());
        }
    }

    // Publish the index last, after every referenced data file is in place.
    files.sort_by_key(|name| (name.ends_with(".kfc"), name.clone()));
    let mut records = Vec::new();
    for name in files {
        sync(&stage.join(&name))?;
        let target = root.join(&name);
        let existed = target.is_file();
        if existed {
            let rollback = stage.join(format!("{name}.rollback"));
            fs::copy(&target, &rollback)?;
            sync(&rollback)?;
        }
        records.push((name, existed));
    }

    let journal = stage.join("journal.tmp");
    fs::write(
        &journal,
        serde_json::to_vec(&records).map_err(|_| invalid())?,
    )?;
    sync(&journal)?;
    fs::rename(journal, stage.join("pending.json"))?;
    Ok(records)
}

pub fn commit(root: &Path, stem: &str) -> io::Result<()> {
    let stage = stage_path(root, stem)?;
    let records = prepare(root, stem)?;
    let publish = (|| {
        for (name, _) in records {
            replace(&stage.join(&name), &root.join(&name))?;
        }
        fs::rename(stage.join("pending.json"), stage.join("completed.json"))?;
        Ok::<_, io::Error>(())
    })();

    if let Err(error) = publish {
        if let Err(recovery_error) = recover(root, stem) {
            return Err(io::Error::new(
                error.kind(),
                format!("{error}; asset recovery also failed: {recovery_error}"),
            ));
        }
        return Err(error);
    }
    clean(&stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("shroudforge-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("game.kfc"), b"old-index").unwrap();
        fs::write(root.join("game.kfc_resources"), b"old-data").unwrap();
        root
    }

    fn remove_fixture(root: &Path) {
        for name in ["game.kfc", "game.kfc_resources", "game_123.dat"] {
            let path = root.join(name);
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn interrupted_publish_restores_previous_set() {
        let root = fixture();
        let stage = begin(&root, "game").unwrap();
        fs::write(stage.join("game.kfc"), b"new-index").unwrap();
        fs::write(stage.join("game.kfc_resources"), b"new-data").unwrap();
        fs::write(stage.join("game_123.dat"), b"new-content").unwrap();
        prepare(&root, "game").unwrap();
        replace(
            &stage.join("game.kfc_resources"),
            &root.join("game.kfc_resources"),
        )
        .unwrap();
        replace(&stage.join("game_123.dat"), &root.join("game_123.dat")).unwrap();
        recover(&root, "game").unwrap();
        assert_eq!(fs::read(root.join("game.kfc")).unwrap(), b"old-index");
        assert_eq!(
            fs::read(root.join("game.kfc_resources")).unwrap(),
            b"old-data"
        );
        assert!(!root.join("game_123.dat").exists());
        remove_fixture(&root);
    }

    #[test]
    fn commit_publishes_complete_set() {
        let root = fixture();
        let stage = begin(&root, "game").unwrap();
        fs::write(stage.join("game.kfc"), b"new-index").unwrap();
        fs::write(stage.join("game.kfc_resources"), b"new-data").unwrap();
        commit(&root, "game").unwrap();
        assert_eq!(fs::read(root.join("game.kfc")).unwrap(), b"new-index");
        assert_eq!(
            fs::read(root.join("game.kfc_resources")).unwrap(),
            b"new-data"
        );
        assert!(!stage.exists());
        remove_fixture(&root);
    }

    #[cfg(windows)]
    #[test]
    fn blocked_publication_preserves_live_file_and_can_recover_after_handle_closes() {
        let root = fixture();
        let stage = begin(&root, "game").unwrap();
        fs::write(stage.join("game.kfc"), b"new-index").unwrap();
        fs::write(stage.join("game.kfc_resources"), b"new-data").unwrap();
        // This is the same ordinary file handle retained by KFCCursor.
        let retained = fs::File::open(root.join("game.kfc_resources")).unwrap();
        let error = commit(&root, "game").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("game.kfc_resources"), "{error}");
        assert_eq!(
            fs::read(root.join("game.kfc_resources")).unwrap(),
            b"old-data"
        );
        assert_eq!(fs::read(root.join("game.kfc")).unwrap(), b"old-index");
        assert!(stage.join("pending.json").is_file());
        drop(retained);
        recover(&root, "game").unwrap();
        recover(&root, "game").unwrap();
        assert_eq!(
            fs::read(root.join("game.kfc_resources")).unwrap(),
            b"old-data"
        );
        assert!(!stage.exists());
        remove_fixture(&root);
    }

    #[test]
    fn failed_replacement_preserves_existing_target() {
        let root = fixture();
        let error = replace(&root.join("missing-staged-file"), &root.join("game.kfc")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read(root.join("game.kfc")).unwrap(), b"old-index");
        remove_fixture(&root);
    }

    #[cfg(unix)]
    #[test]
    fn linked_stage_cannot_delete_unrelated_files() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), b"unrelated").unwrap();
        symlink(&outside, directory(&root, "game")).unwrap();
        assert!(recover(&root, "game").is_err());
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"unrelated");
        assert!(begin(&root, "game").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn linked_recovery_inputs_preserve_live_assets_and_external_files() {
        use std::os::unix::fs::symlink;
        for name in ["pending.json", "game.kfc.rollback", "game.kfc.restore"] {
            let root = fixture();
            let stage = begin(&root, "game").unwrap();
            let external = root.join("external");
            fs::write(&external, b"unrelated").unwrap();
            fs::write(stage.join("pending.json"), br#"[["game.kfc",true]]"#).unwrap();
            fs::write(stage.join("game.kfc.rollback"), b"replacement").unwrap();
            let link = stage.join(name);
            if link.exists() {
                fs::remove_file(&link).unwrap();
            }
            symlink(&external, &link).unwrap();
            assert!(recover(&root, "game").is_err(), "{name}");
            assert_eq!(fs::read(&external).unwrap(), b"unrelated", "{name}");
            assert_eq!(
                fs::read(root.join("game.kfc")).unwrap(),
                b"old-index",
                "{name}"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn recovery_validates_the_complete_journal_before_replacing_assets() {
        let root = fixture();
        let stage = begin(&root, "game").unwrap();
        fs::write(stage.join("game.kfc.rollback"), b"replacement").unwrap();
        fs::write(
            stage.join("pending.json"),
            br#"[["game.kfc",true],["../external",false]]"#,
        )
        .unwrap();
        assert!(recover(&root, "game").is_err());
        assert_eq!(fs::read(root.join("game.kfc")).unwrap(), b"old-index");
        fs::write(
            stage.join("pending.json"),
            br#"[["game.kfc",true],["game.kfc",false]]"#,
        )
        .unwrap();
        assert!(recover(&root, "game").is_err());
        assert!(begin(&root, "../game").is_err());
        assert_eq!(fs::read(root.join("game.kfc")).unwrap(), b"old-index");
        fs::remove_dir_all(root).unwrap();
    }
}
