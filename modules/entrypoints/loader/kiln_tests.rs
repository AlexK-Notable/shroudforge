// SPDX-License-Identifier: MIT
//! Kiln layer: tests of the loader paths that only a host-bundled runtime takes.
#![cfg(windows)]

use kfc::{
    container::{KFCFile, KFCWriter},
    reflection::TypeRegistry,
};
use std::{fs, path::Path, rc::Rc};

fn scratch() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "shroudforge-kiln-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    root
}

/// A small, valid archive pair of one game version.
fn write_game(folder: &Path, stem: &str, game_version: &str) {
    for extension in ["kfc", "kfc_resources"] {
        fs::write(folder.join(format!("{stem}.{extension}")), []).unwrap();
    }
    KFCWriter::<Rc<KFCFile>, _>::new(folder, stem, Rc::new(TypeRegistry::default()), game_version)
        .unwrap()
        .finalize()
        .unwrap();
}

#[test]
fn a_game_update_gets_new_working_backups_without_an_updater() {
    let stem = "enshrouded_server";
    let root = scratch();
    let previous = root.join("previous");
    fs::create_dir(&previous).unwrap();
    write_game(&previous, stem, "game 1");
    write_game(&root, stem, "game 2");
    for extension in ["kfc", "kfc_resources"] {
        fs::copy(
            previous.join(format!("{stem}.{extension}")),
            root.join(format!("{stem}.{extension}.bak")),
        )
        .unwrap();
    }
    fs::write(root.join(format!("{stem}.kfc.bak.integrity")), "seal of the old pair").unwrap();
    let old_header = fs::read(root.join(format!("{stem}.kfc.bak"))).unwrap();
    let game = root.to_str().unwrap();
    let resources = root.join(format!("{stem}.kfc_resources"));
    assert!(shroudforge_api::working_baseline_is_incompatible(game, stem));

    // The steps of asset_preparation/apply_asset_mods.rs for an outdated baseline.
    crate::recycle_incompatible_game_asset_backups(&root, stem).unwrap();
    shroudforge_api::create_working_kfc_backup(game, stem).unwrap();
    assert!(shroudforge_api::backup_companion_file(resources.to_str().unwrap()));

    assert!(!shroudforge_api::working_baseline_is_incompatible(game, stem));
    for extension in ["kfc", "kfc_resources"] {
        assert_eq!(
            fs::read(root.join(format!("{stem}.{extension}.bak"))).unwrap(),
            fs::read(root.join(format!("{stem}.{extension}"))).unwrap(),
            "the new {extension} backup is the updated game's file"
        );
    }
    assert!(!root.join(format!("{stem}.kfc.bak.integrity")).exists());
    assert!(shroudforge_api::restore(game, stem), "the new pair restores");

    let kept = root.join("shroudforge/backups/outdated-working-baseline").join(stem);
    let generations: Vec<_> = fs::read_dir(&kept).unwrap().map(|entry| entry.unwrap().path()).collect();
    assert_eq!(generations.len(), 1);
    assert_eq!(fs::read(generations[0].join(format!("{stem}.kfc.bak"))).unwrap(), old_header);
    assert_eq!(fs::read_dir(&generations[0]).unwrap().count(), 3, "header, companion and seal");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outdated_working_backups_are_replaced_without_an_updater() {
    let root = scratch();
    let backups = ["enshrouded_server.kfc.bak", "enshrouded_server.kfc_resources.bak"];
    for name in backups {
        fs::write(root.join(name), "the previous game version").unwrap();
    }

    // A host-bundled runtime: no Updater and no ShroudForge installation in the game folder.
    crate::recycle_incompatible_game_asset_backups(&root, "enshrouded_server").unwrap();
    for name in backups {
        assert!(!root.join(name).exists(), "{name} is still in the game folder");
    }
    let kept = root.join("shroudforge/backups/outdated-working-baseline/enshrouded_server");
    assert_eq!(fs::read_dir(&kept).unwrap().count(), 1, "one generation is kept");

    // An installation whose Updater is missing keeps ShroudForge's refusal.
    fs::write(root.join("shroudforge/shroudforge-runtime.dll"), "installed").unwrap();
    for name in backups {
        fs::write(root.join(name), "the previous game version").unwrap();
    }
    let error = crate::recycle_incompatible_game_asset_backups(&root, "enshrouded_server").unwrap_err();
    assert!(error.contains("Updater is unavailable"), "{error}");
    for name in backups {
        assert!(root.join(name).exists(), "{name} was moved");
    }
    fs::remove_dir_all(root).unwrap();
}
