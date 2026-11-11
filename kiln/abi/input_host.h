/* SPDX-License-Identifier: MIT
 * Kiln layer of frostdev-ops/shroudforge: the host keyboard feed of shroudforge-runtime.dll.
 *
 * ShroudForge's injected runtime takes the key state behind shroudforge.input.is_keybind_down and
 * was_keybind_pressed from a hidden child process, <game>/shroudforge/shroudforge.exe --keyboard-input. A host
 * that already sees the game window's input can pass the window's own key records to these functions instead.
 * The runtime then starts no helper process. A host that never calls shroudforge_input_host_begin gets
 * ShroudForge's behaviour unchanged.
 *
 * All seven are optional exports: look each one up with GetProcAddress and treat a missing one as "this runtime
 * has no host feed" (upstream ShroudForge and fork builds before this header have none).
 *
 * Rules for the host:
 *  - Call shroudforge_input_host_begin before shroudforge_create. If it returns 0, ShroudForge's own keyboard
 *    input already runs in this process: do not feed.
 *  - Pass every keyboard record of the game window, presses and releases, including the ones the host consumes
 *    for itself. Pass each key through one of the two key functions, whichever the window received; a key that
 *    arrives twice is counted once.
 *  - Call shroudforge_input_host_reset when the game window loses the keyboard focus (WM_KILLFOCUS, or
 *    WM_ACTIVATEAPP with wParam 0). The window does not get the releases of keys held at that moment.
 *  - Any thread may call these functions. The runtime runs one call at a time and holds only short locks of its
 *    own; it does not wait for the game or for a mod, and it does not call back.
 *  - Print Screen: Windows sends a window only its release. shroudforge_input_host_window_key counts the missing
 *    press itself, as SDL does.
 *  - Key codes are translated with SDL's own Windows scan-code table, so a key gets the SDL:<n> code that
 *    ShroudForge's settings store for it.
 *
 * Not covered: recording a new shortcut (that is ShroudForge's Modloader window, or the host's own settings
 * page), and the fake left Ctrl that Windows adds to AltGr in window messages (raw input does not have it).
 */
#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SHROUDFORGE_INPUT_HOST_VERSION 1u

/* The version of this interface the runtime implements. */
uint32_t shroudforge_input_host_version(void);

/* The host takes over the shortcut state. 1: taken. 0: ShroudForge's own keyboard input is already running. */
uint32_t shroudforge_input_host_begin(void);

/* One keyboard record of WM_INPUT: RAWKEYBOARD.MakeCode, .Flags and .VKey, unchanged. */
void shroudforge_input_host_raw_key(uint32_t make_code, uint32_t flags, uint32_t vkey);

/* wParam and lParam of WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN or WM_SYSKEYUP, unchanged, for a window that gets no
 * raw keyboard input. */
void shroudforge_input_host_window_key(uintptr_t wparam, intptr_t lparam);

/* button: 1 left, 2 middle, 3 right, 4 and 5 the side buttons (XBUTTON1, XBUTTON2). down: 1 pressed, 0 released. */
void shroudforge_input_host_mouse_button(uint32_t button, uint32_t down);

/* The game window lost the keyboard focus: every key and button counts as released. */
void shroudforge_input_host_reset(void);

/* The host stops feeding. Shortcut queries answer "not held" afterwards; the helper process is not started.
 * shroudforge_input_host_begin may be called again. */
void shroudforge_input_host_end(void);

#ifdef __cplusplus
}
#endif
