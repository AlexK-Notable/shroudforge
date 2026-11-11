# Edits to upstream files (keep this list short)

Every change to a file outside `kiln/` is listed here with one line, so a merge from upstream stays cheap. Everything
else Kiln needs lives in `kiln/`.

| File | Change | Why |
|---|---|---|
| `modules/entrypoints/build_windows_app_icon.rs` | tracks `SHROUDFORGE_NO_APP_ICON` and skips the icon resource when it is set | winres needs `rc.exe`, which a Linux cross build (cargo xwin) lacks; the runtime DLL needs no icon. Upstream behaviour unchanged without the variable. |
| `modules/game-runtime/native/engine-hooks/install_engine_hooks.cpp` (+ `.h`) | optional host path: with a table from `KfcRuntimeSetHost` the six engine hooks are found and placed by the host (`find_signature`, `install_hook`, `remove_hook`) instead of by SF's own scan and trampolines; `Uninstall` removes host hooks through the host. Without a host nothing changes. | Kiln hosts SF's hooks in `kiln.hooks`, so a DLL that hooked a place first does not switch SF's native runtime off. |
| `modules/game-runtime/native/abi/runtime_host.h` (new) | the host table and `KfcRuntimeSetHost` | the interface above; `KFC_RUNTIME_ABI_VERSION` stays 12 (an optional export) |
| `modules/hotkeys/observe_keyboard.rs` | host feed inside `mod windows` (`host_begin`, `host_key`, `host_mouse_button`, `host_reset`, `host_end`, one block before the tests); while a host feeds, `initialize` starts neither SDL input nor the helper process, and a helper reader that ends or comes up late leaves the host's state alone. Without `host_begin` nothing changes. | Kiln's bundled component has no `shroudforge.exe`, so the `--keyboard-input` helper could not start and every mod keybind answered "not held". Kiln passes the game window's own key records instead. |
| `modules/hotkeys/host_input.rs`, `modules/hotkeys/host_keyboard.c` (new) | the seven exported `shroudforge_input_host_*` functions (`kiln/abi/input_host.h`) and their key translation with SDL's Windows scan-code table | the interface above; optional exports of `shroudforge-runtime.dll` |
| `modules/hotkeys/lib.rs`, `modules/hotkeys/build.rs` | `mod host_input;` and `host_keyboard.c` added to the adapter's sources | wiring for the two new files |

Not patched, handled in `kiln/` instead:
- `modules/game-runtime/native/CMakeLists.txt`: its generated license resource script quotes the resource names, which
  llvm-rc refuses. `kiln/native/CMakeLists.txt` rewrites the generated file after configure.
- The test of the host path: `kiln/native/tests/engine_hooks_host_test.cpp`, target `kfc-runtime-test-engine-hooks` in
  `kiln/native/CMakeLists.txt` (Wine, no game).
- The tests of the host key feed are in `modules/hotkeys/host_input.rs` itself (they need the crate's private state):
  `cargo xwin test -p shroudforge-hotkeys --target x86_64-pc-windows-msvc` under Wine.
