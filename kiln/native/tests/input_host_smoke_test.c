/* SPDX-License-Identifier: MIT
 * Kiln layer: calls the host key feed of a built shroudforge-runtime.dll the way a host does (kiln/abi/input_host.h):
 * every function is found with GetProcAddress and called. What the calls do to the shortcut state is tested in
 * modules/hotkeys/host_input.rs; this checks the exported interface of the DLL that ships.
 *   wine shroudforge-runtime-test-input-host.exe <path to shroudforge-runtime.dll> */
#include <stdint.h>
#include <stdio.h>
#include <windows.h>

#include "../../abi/input_host.h"

#define FIND(name) \
    name##_t name##_fn = (name##_t)(void *)GetProcAddress(runtime, #name); \
    if (!name##_fn) { fprintf(stderr, "missing export %s\n", #name); return 1; }

typedef uint32_t (*shroudforge_input_host_version_t)(void);
typedef uint32_t (*shroudforge_input_host_begin_t)(void);
typedef void (*shroudforge_input_host_raw_key_t)(uint32_t, uint32_t, uint32_t);
typedef void (*shroudforge_input_host_window_key_t)(uintptr_t, intptr_t);
typedef void (*shroudforge_input_host_mouse_button_t)(uint32_t, uint32_t);
typedef void (*shroudforge_input_host_reset_t)(void);
typedef void (*shroudforge_input_host_end_t)(void);

int wmain(int argc, wchar_t **argv)
{
    if (argc != 2) { fprintf(stderr, "usage: <shroudforge-runtime.dll>\n"); return 2; }
    HMODULE runtime = LoadLibraryW(argv[1]);
    if (!runtime) { fprintf(stderr, "could not load the runtime: %lu\n", GetLastError()); return 1; }
    FIND(shroudforge_input_host_version)
    FIND(shroudforge_input_host_begin)
    FIND(shroudforge_input_host_raw_key)
    FIND(shroudforge_input_host_window_key)
    FIND(shroudforge_input_host_mouse_button)
    FIND(shroudforge_input_host_reset)
    FIND(shroudforge_input_host_end)

    if (shroudforge_input_host_version_fn() != SHROUDFORGE_INPUT_HOST_VERSION) {
        fprintf(stderr, "the runtime implements version %u, the header is version %u\n",
            shroudforge_input_host_version_fn(), SHROUDFORGE_INPUT_HOST_VERSION);
        return 1;
    }
    /* Calls before begin are ignored. */
    shroudforge_input_host_raw_key_fn(0x43, 0, 0x78);
    shroudforge_input_host_reset_fn();
    shroudforge_input_host_end_fn();
    for (int round = 0; round < 2; ++round) {
        if (shroudforge_input_host_begin_fn() != 1) { fprintf(stderr, "begin was refused in round %d\n", round); return 1; }
        shroudforge_input_host_raw_key_fn(0x1d, 0, 0x11);                              /* left Ctrl down */
        shroudforge_input_host_raw_key_fn(0x43, 0, 0x78);                              /* F9 down */
        shroudforge_input_host_raw_key_fn(0x43, 1, 0x78);                              /* F9 up */
        shroudforge_input_host_raw_key_fn(0x1d, 4, 0x13);                              /* Pause: the E1 record... */
        shroudforge_input_host_raw_key_fn(0x45, 0, 0xff);                              /* ...and its key */
        shroudforge_input_host_raw_key_fn(0, 0, 0xb3);                                 /* a media key without a make code */
        shroudforge_input_host_raw_key_fn(0xffff, 0xffff, 0xffff);                     /* nonsense is no key */
        shroudforge_input_host_window_key_fn(0x78, 1 | (0x43 << 16));                  /* WM_KEYDOWN F9 */
        shroudforge_input_host_window_key_fn(0x78, (intptr_t)0xc0000001 | (0x43 << 16)); /* WM_KEYUP F9 */
        shroudforge_input_host_window_key_fn(0, 0);
        shroudforge_input_host_mouse_button_fn(4, 1);
        shroudforge_input_host_mouse_button_fn(4, 0);
        shroudforge_input_host_mouse_button_fn(0, 1);
        shroudforge_input_host_mouse_button_fn(4000, 1);
        shroudforge_input_host_reset_fn();
        shroudforge_input_host_end_fn();
    }
    puts("ok: host key feed");
    return 0;
}
