#!/usr/bin/env bash
# Kiln layer (MIT): merge an upstream ShroudForge release branch into this fork's `kiln` branch, rebuild, test.
#   kiln/sync-upstream.sh <upstream-branch> [--scratch]     e.g. kiln/sync-upstream.sh 1.8.0
# Steps: fetch upstream (bonsaibauer/shroudforge) with tags; merge upstream/<branch> into the current branch with
# --no-commit (--scratch: into a throwaway worktree on branch scratch/merge-<branch>, removed afterwards); list
# conflicts and stop if there are any; update submodules; write kiln/UPSTREAM-VERSION; run kiln/build.sh and the host
# tests below; check that the C interface Kiln drives is unchanged. Never commits or pushes: review, then commit.
set -euo pipefail
branch=${1:?usage: kiln/sync-upstream.sh <upstream-branch> [--scratch]}
scratch=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
git remote get-url upstream >/dev/null 2>&1 || git remote add upstream https://github.com/bonsaibauer/shroudforge.git
git fetch --tags upstream "$branch"
work=$root
if [[ $scratch == --scratch ]]; then
  work=${KILN_SF_SCRATCH:-$(mktemp -d)}/merge-$branch
  git worktree add -q "$work" -b "scratch/merge-$branch" HEAD
  trap 'git -C "$root" worktree remove --force "$work"; git -C "$root" branch -D "scratch/merge-$branch" >/dev/null' EXIT
fi
cd "$work"
if ! git merge --no-commit --no-ff "upstream/$branch"; then
  echo "== conflicts:"; git diff --name-only --diff-filter=U
  echo "Resolve them (keep upstream's side unless kiln/UPSTREAM-PATCHES.md lists the file), then rerun the build." >&2
  exit 1
fi
echo "== merged upstream/$branch: $(git diff --cached --shortstat)"
echo "== upstream files the merge touches that Kiln patches (kiln/UPSTREAM-PATCHES.md):"
grep -o '^| `[^`]*`' kiln/UPSTREAM-PATCHES.md | tr -d '|` ' | while read -r f; do
  git diff --cached --quiet -- "$f" || echo "  $f"
done
# the C interface kiln-shroudforge.dll drives
for sym in shroudforge_prepare_startup shroudforge_create shroudforge_update shroudforge_destroy; do
  grep -q "fn $sym" modules/entrypoints/loader/embedded.rs || { echo "missing export $sym: kiln-shroudforge needs an update" >&2; exit 1; }
done
grep -n 'KFC_RUNTIME_ABI_VERSION' modules/game-runtime/native/abi/runtime_api.h
git submodule update --init --recursive kfc-parser modules/game-runtime/native/json modules/hotkeys/SDL
echo "$branch" | sed 's/^v//' > kiln/UPSTREAM-VERSION
SF_UI_REBUILD=1 kiln/build.sh "${KILN_SF_OUT:-$work/kiln/out}"
# tests: the loader-config tests run on the Linux host; upstream's other test crates are Windows-only (zip and Win32
# dependencies), so they are compiled for Windows (KILN_SF_WINE_TESTS=1 also runs them under Wine)
cargo test --locked -q -p shroudforge-loader-config
if [[ ${KILN_SF_WINE_TESTS:-0} == 1 ]]; then
  CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER=wine SHROUDFORGE_NO_APP_ICON=1 cargo xwin test --locked -q -p shroudforge-api --target x86_64-pc-windows-msvc
else
  SHROUDFORGE_NO_APP_ICON=1 cargo xwin test --locked -q --no-run -p shroudforge-api --target x86_64-pc-windows-msvc
fi
echo "== upstream/$branch merged and built in $work (not committed)"
