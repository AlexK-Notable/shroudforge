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

# Kiln's standalone cross build must not expose absolute build/source paths or wall-clock PE timestamps.
get_filename_component(_kiln_root "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
target_compile_options(kfc-runtime PRIVATE "/clang:-ffile-prefix-map=${_kiln_root}=/shroudforge"
    "/clang:-ffile-prefix-map=${CMAKE_BINARY_DIR}=/build/shroudforge")
target_link_options(kfc-runtime PRIVATE /Brepro /pdbaltpath:%_PDB%)

# Host interface of the engine-hook installer (abi/runtime_host.h): a Wine test with a stand-in game and a fake host.
#   cmake --build <dir> --target kfc-runtime-test-engine-hooks && wine <dir>/bin/kfc-runtime-test-engine-hooks.exe
set(_kiln_native "${CMAKE_CURRENT_SOURCE_DIR}/../../modules/game-runtime/native")
add_executable(kfc-runtime-test-engine-hooks EXCLUDE_FROM_ALL
    tests/engine_hooks_host_test.cpp
    "${_kiln_native}/engine-hooks/install_engine_hooks.cpp"
    "${_kiln_native}/engine-hooks/scan_executable_signature.cpp"
    "${_kiln_native}/engine-hooks/allocate_near_code.cpp"
    "${_kiln_native}/engine-hooks/write_executable_bytes.cpp")
target_include_directories(kfc-runtime-test-engine-hooks PRIVATE "${_kiln_native}" "${_kiln_native}/ecs"
    "${_kiln_native}/engine-context" "${_kiln_native}/json/single_include")
target_compile_features(kfc-runtime-test-engine-hooks PRIVATE cxx_std_20)
target_compile_definitions(kfc-runtime-test-engine-hooks PRIVATE KFC_RUNTIME_BUILD WIN32_LEAN_AND_MEAN NOMINMAX)
set_property(TARGET kfc-runtime-test-engine-hooks PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
set_target_properties(kfc-runtime-test-engine-hooks PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
