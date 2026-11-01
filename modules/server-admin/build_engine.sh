#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
admin_sdk=${EMF_SDK_ROOT:-/home/jame/Downloads/EMF-0.3.3-manual}
admin_xwin=${XWIN_ROOT:-/home/jame/.cache/cargo-xwin/xwin}
mkdir -p build/server-admin
admin_source=modules/server-admin/native/engine_adapter.cpp
admin_output=build/server-admin/server-admin-engine.dll
admin_mode=/LD
if [[ ${1:-} == --test ]]; then
 admin_source=modules/server-admin/tests/engine_adapter_test.cpp
 admin_output=build/server-admin/engine-adapter-tests.exe
 admin_mode=
fi
clang-cl -fuse-ld=lld /nologo /std:c++20 /EHsc /O2 /MT $admin_mode /D_CRT_SECURE_NO_WARNINGS \
 /imsvc "$admin_xwin/crt/include" /imsvc "$admin_xwin/sdk/include/ucrt" \
 /imsvc "$admin_xwin/sdk/include/um" /imsvc "$admin_xwin/sdk/include/shared" \
 /I "$admin_sdk/sdk/include" /I modules/server-admin/vendor \
 /Fo"build/server-admin/engine_adapter.obj" "$admin_source" \
 /link /libpath:"$admin_xwin/crt/lib/x86_64" /libpath:"$admin_xwin/sdk/lib/um/x86_64" \
 /libpath:"$admin_xwin/sdk/lib/ucrt/x86_64" /out:"$admin_output"
