# Edits to upstream files (keep this list short)

Every change to a file outside `kiln/` is listed here with one line, so a merge from upstream stays cheap. Everything
else Kiln needs lives in `kiln/`.

| File | Change | Why |
|---|---|---|
| `modules/entrypoints/build_windows_app_icon.rs` | tracks `SHROUDFORGE_NO_APP_ICON` and skips the icon resource when it is set | winres needs `rc.exe`, which a Linux cross build (cargo xwin) lacks; the runtime DLL needs no icon. Upstream behaviour unchanged without the variable. |

Not patched, handled in `kiln/` instead:
- `modules/game-runtime/native/CMakeLists.txt`: its generated license resource script quotes the resource names, which
  llvm-rc refuses. `kiln/native/CMakeLists.txt` rewrites the generated file after configure.
