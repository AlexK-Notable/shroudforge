#!/usr/bin/env bash
# Kiln layer (MIT): merge an upstream ShroudForge release branch into this fork's `kiln` branch, rebuild, test.
#   kiln/sync-upstream.sh <upstream-branch> [--scratch]     e.g. kiln/sync-upstream.sh 1.8.0
# Requires a clean kiln branch and the official upstream remote. Fetches the named branch without changing tags,
# merges FETCH_HEAD with --no-commit, validates the bootstrap contract, rebuilds and tests. --scratch uses a detached
# worktree retained for review (including conflicts). Never commits, pushes, force-removes worktrees or deletes branches.
set -euo pipefail
branch=${1:?usage: kiln/sync-upstream.sh <upstream-branch> [--scratch]}
scratch=${2:-}
[[ $# -le 2 && ( -z $scratch || $scratch == --scratch ) ]] || { echo "unknown arguments" >&2; exit 2; }
git check-ref-format "refs/heads/$branch" >/dev/null || { echo "invalid upstream branch" >&2; exit 2; }
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[[ $(git branch --show-current) == kiln ]] || { echo "run on branch kiln" >&2; exit 1; }
[[ -z $(git status --porcelain --untracked-files=all --ignore-submodules=none) ]] || { echo "working tree/submodules must be clean" >&2; exit 1; }
for state in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do
  [[ ! -f $(git rev-parse --git-path "$state") ]] || { echo "finish the existing Git operation first" >&2; exit 1; }
done
url=$(git remote get-url upstream)
[[ $url == https://github.com/bonsaibauer/shroudforge.git ]] || { echo "unexpected upstream remote: $url" >&2; exit 1; }
git fetch --no-tags upstream "refs/heads/$branch"
upstream=$(git rev-parse --verify 'FETCH_HEAD^{commit}')
work=$root
if [[ $scratch == --scratch ]]; then
  scratch_root=$(mktemp -d "${KILN_SF_SCRATCH:-${TMPDIR:-/tmp}}/kiln-sf-merge.XXXXXX")
  work=$scratch_root/work
  git worktree add -q --detach "$work" HEAD
  # Preserve conflict/build evidence; never force-remove a dirty worktree or delete a branch.
  trap 'echo "scratch worktree kept at $work (remove it after review)" >&2' EXIT
fi
cd "$work"
if ! git merge --no-commit --no-ff "$upstream"; then
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
grep -Eq '^#define KFC_RUNTIME_ABI_VERSION 12u$' modules/game-runtime/native/abi/runtime_api.h || {
  echo "native ABI changed: update kiln-shroudforge before merging" >&2; exit 1;
}
python3 kiln/check-contract.py
git submodule update --init --recursive kfc-parser modules/game-runtime/native/json modules/hotkeys/SDL
echo "$branch" | sed 's/^v//' > kiln/UPSTREAM-VERSION
KILN_SF_ALLOW_DIRTY=1 kiln/build.sh "${KILN_SF_OUT:-$work/kiln/out}"
# tests: the loader-config tests run on the Linux host; upstream's other test crates are Windows-only (zip and Win32
# dependencies), so they are compiled for Windows (KILN_SF_WINE_TESTS=1 also runs them under Wine)
cargo test --locked -q -p shroudforge-loader-config
if [[ ${KILN_SF_WINE_TESTS:-0} == 1 ]]; then
  CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER=wine SHROUDFORGE_NO_APP_ICON=1 cargo xwin test --locked -q -p shroudforge-api --target x86_64-pc-windows-msvc
else
  SHROUDFORGE_NO_APP_ICON=1 cargo xwin test --locked -q --no-run -p shroudforge-api --target x86_64-pc-windows-msvc
fi
echo "== upstream/$branch merged and built in $work (not committed)"
