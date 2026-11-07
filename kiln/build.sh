#!/usr/bin/env bash
# Kiln layer (MIT): builds ShroudForge's runtime from this fork for Kiln's bundle, cross from Linux.
#   kiln/build.sh [OUT]        OUT default kiln/out (gitignored)
# Produces OUT/shroudforge/ (installed by Kiln as <game>/kiln/plugins/shroudforge/):
#   shroudforge-runtime.dll   SF's runtime (crate shroudforge-modloader, cdylib; links kfc-parser: GPL-3)
#   kfc-runtime.dll           SF's native runtime (C++, MIT)
#   profiles/enshrouded/{client,server}/*.json   game-build profiles (data)
#   mods/<id>/                SF's bundled mods (as in SF's release ZIP)
#   version.json              {"version": <kiln/UPSTREAM-VERSION>, "build": "kiln.<commit>"}
#   LICENSES/, NOTICE.txt, SOURCE.txt   GPL-3 + MIT notices, the exact source commit
# Needs: cargo-xwin, clang-cl/lld-link/llvm-rc, cmake, ninja, node/npm (SF's in-game Modloader page is built with vite
# and embedded into the runtime). Environment: CARGO_TARGET_DIR (default OUT/cargo), NICE (default 10).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
out=$(mkdir -p "${1:-$here/out}" && cd "${1:-$here/out}" && pwd)
nice=${NICE:-10}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$out/cargo}
cd "$root"
git submodule update --init --recursive kfc-parser modules/game-runtime/native/json modules/hotkeys/SDL >/dev/null

# 1. SF's web UI (embedded with include_str! into the runtime)
if [[ ! -f build/ui/modloader/index.html || ${SF_UI_REBUILD:-0} == 1 ]]; then
  (cd modules/modloader/ui && nice -n "$nice" npm ci --no-audit --no-fund >/dev/null && nice -n "$nice" npm run build >/dev/null)
fi

# 2. shroudforge-runtime.dll (SHROUDFORGE_NO_APP_ICON: winres cannot run rc.exe on Linux; the DLL needs no icon)
SHROUDFORGE_NO_APP_ICON=1 nice -n "$nice" cargo xwin build --release --locked -p shroudforge-modloader --lib \
  --target x86_64-pc-windows-msvc
dll=$(find "$CARGO_TARGET_DIR/x86_64-pc-windows-msvc/release" -maxdepth 1 -name shroudforge_modloader.dll)
[[ -f $dll ]] || { echo "shroudforge_modloader.dll not built" >&2; exit 1; }

# 3. kfc-runtime.dll
nb=$out/native
nice -n "$nice" cmake -S "$here/native" -B "$nb" -G Ninja -DCMAKE_TOOLCHAIN_FILE="$here/cmake/clang-cl-xwin.cmake" \
  -DCMAKE_BUILD_TYPE=Release -Wno-dev >/dev/null
nice -n "$nice" cmake --build "$nb" --target kfc-runtime >/dev/null

# 4. stage
stage=$out/shroudforge
rm -rf "$stage"
mkdir -p "$stage/LICENSES"
cp "$dll" "$stage/shroudforge-runtime.dll"
cp "$nb/bin/kfc-runtime.dll" "$stage/kfc-runtime.dll"
cp -r modules/game-runtime/profiles "$stage/profiles"
rm -f "$stage/profiles/README.md"
if [[ -d mods ]]; then
  mkdir -p "$stage/mods"
  for m in mods/*/; do [[ -f $m/mod.json ]] && cp -r "$m" "$stage/mods/"; done
fi
cp LICENSE "$stage/LICENSES/ShroudForge-MIT.txt"
cp kfc-parser/LICENSE "$stage/LICENSES/kfc-parser-GPL-3.0.txt"
cp modules/game-runtime/native/json/LICENSE.MIT "$stage/LICENSES/nlohmann-json-MIT.txt"
cp modules/hotkeys/SDL/LICENSE.txt "$stage/LICENSES/SDL3-zlib.txt"
zyd=$(find "$nb" -path '*zydis-src/LICENSE' | head -1); [[ -n $zyd ]] && cp "$zyd" "$stage/LICENSES/zydis-MIT.txt"
zyc=$(find "$nb" -path '*zycore/LICENSE' | head -1); [[ -n $zyc ]] && cp "$zyc" "$stage/LICENSES/zycore-MIT.txt"
commit=$(git rev-parse HEAD)
sfver=$(tr -d "[:space:]" < "$here/UPSTREAM-VERSION")
printf '{"version":"%s","build":"kiln.%s","host":"kiln"}\n' "$sfver" "${commit:0:7}" > "$stage/version.json"
dirty=$(git status --porcelain --ignore-submodules=none | grep -v '^?? kiln/out' | head -1 || true)
cat > "$stage/SOURCE.txt" <<EOF
ShroudForge runtime for Kiln, built from https://github.com/frostdev-ops/shroudforge
commit $commit${dirty:+ (with uncommitted changes)}
upstream https://github.com/bonsaibauer/shroudforge ($(git describe --tags --always upstream/1.7.9 2>/dev/null || echo upstream))
submodules:
$(git submodule status --recursive kfc-parser modules/game-runtime/native/json modules/hotkeys/SDL)
The complete corresponding source, submodules included, ships with each Kiln release as
shroudforge-kiln-<commit>-src.tar.gz.
EOF
cat > "$stage/NOTICE.txt" <<'EOF'
This folder is a separate program shipped with Kiln: ShroudForge's runtime (MIT, bonsaibauer and contributors), built
from the frostdev-ops/shroudforge fork. shroudforge-runtime.dll statically links kfc-parser (Brabb3l, GPL-3.0), so
shroudforge-runtime.dll is distributed under the GNU General Public License version 3 (LICENSES/kfc-parser-GPL-3.0.txt).
kfc-runtime.dll is ShroudForge's own MIT code with zydis/zycore (MIT) and nlohmann/json (MIT). The SDL3 keyboard tables
in shroudforge-runtime.dll are zlib-licensed. Source: SOURCE.txt. Kiln itself (kiln-shroudforge.dll and everything
outside this folder) is MIT and only loads these files through their exported C functions.
EOF
echo "staged $stage ($(du -sh "$stage" | cut -f1)) from $commit"
