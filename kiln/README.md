# ShroudForge for Kiln (the `kiln` branch of frostdev-ops/shroudforge)

This fork is ShroudForge ([bonsaibauer/shroudforge](https://github.com/bonsaibauer/shroudforge), MIT) with a thin
Kiln layer in this `kiln/` folder. Kiln ships the runtime built from it as a separate component,
`<game>/kiln/plugins/shroudforge/`. Kiln's own MIT plugin `kiln-shroudforge.dll` drives it through the runtime's exported
C functions. With a setting, it drives the player's installed ShroudForge instead. Kiln-side docs:
`docs/kiln/shroudforge-compat.md` in the Kiln repo.

## Layout

| Path | What |
|---|---|
| `kiln/build.sh [OUT]` | builds the SF web UI (vite), `shroudforge-runtime.dll` (`cargo xwin`, crate `shroudforge-modloader`), and `kfc-runtime.dll` (CMake with clang-cl); stages `OUT/shroudforge/` with the profiles, the bundled mods, `version.json`, licences, `NOTICE.txt` and `SOURCE.txt` |
| `kiln/native/CMakeLists.txt` | wraps upstream's `modules/game-runtime/native` for the cross build |
| `kiln/cmake/clang-cl-xwin.cmake` | the toolchain file (a copy of Kiln's) |
| `kiln/sync-upstream.sh` | merges an upstream release branch, rebuilds, tests |
| `kiln/UPSTREAM-VERSION` | the upstream release merged into `kiln` (written into `version.json`) |
| `kiln/UPSTREAM-PATCHES.md` | every edit to an upstream file, one line each |

## The interface Kiln drives

`shroudforge-runtime.dll`: `shroudforge_prepare_startup(const wchar_t* game_root) -> bool`,
`shroudforge_create(root) -> handle`, `shroudforge_update(handle, double dt) -> bool`, `shroudforge_destroy(handle)`.
`kfc-runtime.dll`: `KfcRuntimeAbi`, `KfcRuntimeInitialize`, `KfcRuntimeTick`, `KfcRuntimeShutdown`,
`KfcRuntimeStatus` (`modules/game-runtime/native/abi/runtime_api.h`, ABI 12). These are the functions ShroudForge's own
`winmm.dll` bootstrap calls. If upstream changes them, `kiln-shroudforge` (in Kiln) must follow, and
`sync-upstream.sh` fails loudly.

## Merging upstream

```
kiln/sync-upstream.sh 1.8.0 --scratch     # trial run in an isolated worktree (retained): conflicts, build, tests
kiln/sync-upstream.sh 1.8.0               # for real, on the kiln branch (uncommitted merge: review, then commit)
```
Resolve conflicts on upstream's side unless the file is in `UPSTREAM-PATCHES.md`. Then bump Kiln's submodule
`third_party/shroudforge` to the new commit.

Trial on 2026-11-07: merging `upstream/1.8.0` into `kiln` (1.7.9) went through with no conflicts, 205 files changed.
The four runtime exports, `KFC_RUNTIME_ABI_VERSION` 12, the kfc-parser submodule and `build_windows_app_icon.rs` are
unchanged in 1.8.0. `kiln` stays on 1.7.9 until the user asks for 1.8.

## Licences

ShroudForge is MIT. `shroudforge-runtime.dll` statically links kfc-parser (Brabb3l, GPL-3.0, a submodule), so that DLL
is distributed under the GPL-3.0. Kiln ships it as a separate component with `LICENSES/`, `NOTICE.txt`, `SOURCE.txt`
and, in each Kiln release, `shroudforge-kiln-<commit>-src.tar.gz` (this tree with its submodules). Files in `kiln/` are
MIT.

Builds require a clean tree and validate initialized submodules against their gitlinks without updating any checkout.
`KILN_SF_ALLOW_DIRTY=1` permits an explicitly marked development build; sync uses it for its uncommitted merge. Release exports carry
`KILN-SOURCE-COMMIT`, `KILN-SOURCE-EPOCH` and `KILN-SOURCE-SUBMODULES.json` from Kiln's recursive source exporter;
no Git repository is needed to build them. UI assets are rebuilt, paths remapped, and staged mtimes normalized.
`sync-upstream.sh` requires a clean `kiln` branch and the official upstream remote, merges the fetched commit directly,
and checks the exact ABI/signatures. Scratch worktrees are retained for review, including conflicts; no force removal,
branch deletion, commit or push is performed by the script.
