# Kiln layer (MIT; frostdev-ops/shroudforge branch `kiln`): builds ShroudForge's native runtime kfc-runtime.dll from
# the untouched upstream tree with clang-cl/lld-link (cross from Linux, ../cmake/clang-cl-xwin.cmake).
#   cmake -S kiln/native -B <dir> -G Ninja -DCMAKE_TOOLCHAIN_FILE=kiln/cmake/clang-cl-xwin.cmake -DCMAKE_BUILD_TYPE=Release
cmake_minimum_required(VERSION 3.24)
project(shroudforge_kiln_native LANGUAGES C CXX)

add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../modules/game-runtime/native" upstream-native)

# llvm-rc refuses the quoted resource names upstream's generated license script uses ("ZYDIS_LICENSE" RCDATA ...;
# MSVC's rc.exe accepts them). Rewrite that generated file after upstream wrote it, instead of patching upstream.
set(_kiln_rc "${CMAKE_BINARY_DIR}/upstream-native/generated/runtime_licenses.rc")
if(EXISTS "${_kiln_rc}")
    file(READ "${_kiln_rc}" _kiln_rc_text)
    string(REGEX REPLACE "\"([A-Z_]+)\" RCDATA" "\\1 RCDATA" _kiln_rc_text "${_kiln_rc_text}")
    file(WRITE "${_kiln_rc}" "${_kiln_rc_text}")
endif()
