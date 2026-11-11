// SPDX-License-Identifier: MIT
//! Kiln layer: tests of the loader paths that only a host-bundled runtime takes.
#![cfg(windows)]

use std::fs;

#[test]
fn outdated_working_backups_are_replaced_without_an_updater() {
    let root = std::env::temp_dir().join(format!(
        "shroudforge-kiln-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
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
