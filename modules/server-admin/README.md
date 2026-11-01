# Emberwatch: local ShroudForge administration

This module owns the loopback HTTP service, its web dashboard and an explicit-command engine adapter. The ShroudForge fork adds acknowledged runtime administration under `modules/mod-runtime/ingame/process_admin_requests.rs`. It retains the Wine asset-publication fix from `0f12100`.

## Run

```sh
python3 modules/server-admin/dashboard.py --root "$HOME/.local/share/Steam/steamapps/common/EnshroudedServer"
```

Open http://127.0.0.1:8787. It binds only to loopback. Browser mutations require a same-origin request and a per-process anti-CSRF token. No passwords from `enshrouded_server.json` are sent to the browser. There is no shell-command or arbitrary-Lua endpoint.

## What it does

- Live server status from Steam A2S; live player names, positions, inventories and item catalog from the engine adapter.
- Inventory additions and exact-slot removals through native game transactions. The whole displayed inventory must still match before a mutation is queued. Partial additions are reported as partial, never successful completion. A pre-operation inventory snapshot is kept locally; it is an audit record, not an automatic character-save restore.
- Native fast-travel requests to coordinates or the last recorded death site. Death positions are captured from `EntityDiedEvent`, keyed by account hash and world save slot. Graves created before the adapter was installed cannot be reconstructed. A death-site record does not prove a tombstone still exists or the destination is free of hazards.
- World settings through the engine's live settings API and its replication flag. Values are range checked and acknowledged only after readback. Session-only is the default; explicit persistence is delegated to the engine library's per-world settings store.
- World-save requests acknowledged by the engine's save-completion callback. Backups request a completed save before copying the save directory.
- ShroudForge runtime mod settings, activation, unload and Lua refresh. Asset patches and native module replacement require restart. Lua refresh currently rebuilds the Lua VM and reapplies its enabled runtime mods; it is not a native DLL reload.
- Server, ShroudForge and engine logs; correlated command results and a local action log.

The administration adapter never activates survival cheats, changes rates, edits inventories or teleports anyone automatically. Existing gameplay-mod configuration is preserved. Native modules remain process-owned across Lua refreshes because engine hooks can retain callbacks. Replacing their binaries requires a process restart.

## Native dependency

The safe inventory, fast-travel, save and world-setting operations are supplied by the **EMF 0.3.3 engine library, API 21** (MIT). This is a deliberate dependency: ShroudForge 1.7.5 does not expose those native transactions. The installer copies only the engine DLL, bootstrap DLL, license and this one administration adapter. It does **not** install EMF's bundled gameplay mods or enable its debug bridge. ShroudForge loads the bootstrap as the server-only `admin-engine` package. Embervale Teleport stays separate and unchanged.

The installed EMF SDK and library are expected at `~/Downloads/EMF-0.3.3-manual` (`EMF_SDK_ROOT` overrides the build input). `build_engine.sh` uses clang-cl and the existing cargo-xwin Windows SDK cache. The nlohmann JSON single header is vendored with its license and source checksum.

```sh
modules/server-admin/build_engine.sh
SHROUDFORGE_SKIP_APP_ICON=1 cargo xwin build --release --target x86_64-pc-windows-msvc -p shroudforge-modloader --lib
# Stop the target server cleanly before replacing a native DLL.
python3 modules/server-admin/install_engine.py /absolute/server/directory
# Copy target/x86_64-pc-windows-msvc/release/shroudforge_modloader.dll to
# <server>/shroudforge/shroudforge-runtime.dll, preserving a rollback copy first.
```

`SHROUDFORGE_SKIP_APP_ICON` omits the application icon when cross-building a runtime DLL without Microsoft's resource compiler. It does not change runtime functionality.

## Local protocol

`<server>/shroudforge/runtime/admin/{engine,shroudforge}/requests/<32-hex-id>.json` is atomically published by the dashboard. A request has `id`, `createdAt`, an allowlisted `op`, and a current process/session identity. Native player operations also require account and entity identities; inventory mutations require the exact observed inventory. Requests expire after 30 seconds. Consumers claim each request before execution and write `results/<id>.json` with `ok` and operation-specific data. A queued request is not an applied action. Requests interrupted after claim are not replayed automatically.

A missing result or HTTP timeout is an **uncertain outcome**, not proof of failure or cancellation. Inspect state and the action log before retrying. Server restarts invalidate player command sessions. HTTP success for submission is 202; action success comes only from its result.

Files under `~/.local/state/shroudforge-admin/<server-path-hash>/` hold the action log, results, pre-edit inventory snapshots and world backups. This directory is private to the local user. Backup archives contain the world save directory; player character saves live on their clients and are not included.

## Validation

```sh
python3 -m unittest discover -s modules/server-admin/tests -v
node --check modules/server-admin/web/app.js
cargo test -p shroudforge-api --lib process_admin_requests
```

The isolated integration server is a separate copy on UDP 25637 with its own Proton prefix and corrected ShroudForge storage paths. `restart_test_server.py` targets only that copy. Do not point a copied server's absolute ShroudForge paths back at the live install.
