use std::{error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=SHROUDFORGE_SKIP_APP_ICON");
    if std::env::var_os("SHROUDFORGE_SKIP_APP_ICON").is_some() { return Ok(()); }
    if std::env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }

    let manifest_directory = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or("Cargo did not provide the package directory for the Windows app icon build")?,
    );
    let icon_path = manifest_directory
        .ancestors()
        .map(|directory| directory.join("assets/shroudforge.ico"))
        .find(|path| path.is_file())
        .ok_or("Could not find assets/shroudforge.ico above the application package")?;
    println!("cargo:rerun-if-changed={}", icon_path.display());

    let icon_path = icon_path
        .to_str()
        .ok_or("The ShroudForge icon path is not valid Unicode")?;
    let output_directory = PathBuf::from(
        std::env::var_os("OUT_DIR")
            .ok_or("Cargo did not provide the output directory for the Windows app icon build")?,
    );
    let output_directory_text = output_directory
        .to_str()
        .ok_or("The Windows app icon output path is not valid Unicode")?;
    let resource_script_path = output_directory.join("shroudforge-app-icon.rc");
    let resource_script_text = resource_script_path
        .to_str()
        .ok_or("The Windows app icon resource path is not valid Unicode")?;
    let resource_icon_path = icon_path.replace('\\', "/").replace('"', "\\\"");
    fs::write(
        &resource_script_path,
        format!("1 ICON \"{resource_icon_path}\"\n"),
    )?;
    winres::WindowsResource::new()
        .set_resource_file(resource_script_text)
        .set_output_directory(output_directory_text)
        .compile()?;

    // winres already links resource.lib on MSVC. Adding it again duplicates the icons.
    // GNU needs the object explicitly because its static archive has no referenced symbols.
    if std::env::var("CARGO_CFG_TARGET_ENV")? == "gnu" {
        let resource_file = output_directory.join("resource.o");
        println!("cargo:rustc-link-arg-bins={}", resource_file.display());
    }
    Ok(())
}
