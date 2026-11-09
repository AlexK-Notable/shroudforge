# Edits to upstream files (keep this list short)

Every change to a file outside `kiln/` is listed here with one line, so a merge from upstream stays cheap. Everything
else Kiln needs lives in `kiln/`.

| File | Change | Why |
|---|---|---|
| `modules/entrypoints/build_windows_app_icon.rs` | tracks `SHROUDFORGE_NO_APP_ICON` and skips the icon resource when it is set | winres needs `rc.exe`, which a Linux cross build (cargo xwin) lacks; the runtime DLL needs no icon. Upstream behaviour unchanged without the variable. |

| `modules/game-runtime/native/engine-hooks/install_engine_hooks.cpp` (+ `.h`) | optional host path: with a table from `KfcRuntimeSetHost` the six engine hooks are found and placed by the host (`find_signature`, `install_hook`, `remove_hook`) instead of by SF's own scan and trampolines; `Uninstall` removes host hooks through the host. Without a host nothing changes. | Kiln hosts SF's hooks in `kiln.hooks`, so a DLL that hooked a place first does not switch SF's native runtime off. |
| `modules/game-runtime/native/abi/runtime_host.h` (new) | the host table and `KfcRuntimeSetHost` | the interface above; `KFC_RUNTIME_ABI_VERSION` stays 12 (an optional export) |

Not patched, handled in `kiln/` instead:
- `modules/game-runtime/native/CMakeLists.txt`: its generated license resource script quotes the resource names, which
  llvm-rc refuses. `kiln/native/CMakeLists.txt` rewrites the generated file after configure.
- The test of the host path: `kiln/native/tests/engine_hooks_host_test.cpp`, target `kfc-runtime-test-engine-hooks` in
  `kiln/native/CMakeLists.txt` (Wine, no game).
