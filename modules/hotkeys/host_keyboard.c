/* SPDX-License-Identifier: MIT
 * Kiln layer. Key translation for a host that feeds ShroudForge's shortcut
 * state from the game window's own input (host_input.rs), in place of the
 * keyboard helper process.
 *
 * The scan-code rules are SDL's own (SDL_windowsevents.c:
 * WIN_HandleRawKeyboardInput and WindowsScanCodeToSDLScanCode) through SDL's
 * table, so a host-fed key gets the SDL:<n> code the helper process reports
 * for it. Nothing here starts an SDL subsystem. */
#include "SDL_internal.h"
#include "events/scancodes_windows.h"
#include <windows.h>

static int scancode_at(unsigned index)
{
    return index < SDL_arraysize(windows_scancode_table)
        ? (int)windows_scancode_table[index]
        : SDL_SCANCODE_UNKNOWN;
}

/* One keyboard record of WM_INPUT: RAWKEYBOARD.MakeCode, .Flags and .VKey.
 * *pending_e1 carries the E1 prefix of Pause from one record to the next.
 * Returns 0 for a prefix record and for a record that names no key. */
int sf_host_keyboard_raw_scancode(unsigned make_code, unsigned flags, unsigned vkey, int *pending_e1)
{
    /* The three fields are 16 bits wide in RAWKEYBOARD. */
    make_code &= 0xffff;
    flags &= 0xffff;
    vkey &= 0xffff;
    if (flags & RI_KEY_E1) {
        *pending_e1 = 1;
        return SDL_SCANCODE_UNKNOWN;
    }
    if ((flags & RI_KEY_E0) && make_code == 0x2a) return SDL_SCANCODE_UNKNOWN;
    /* Some keys (media keys) arrive without a make code. */
    if (!make_code) make_code = LOWORD(MapVirtualKeyW(vkey, MAPVK_VK_TO_VSC_EX));
    if (!make_code) return SDL_SCANCODE_UNKNOWN;
    if (*pending_e1) {
        *pending_e1 = 0;
        return make_code == 0x45 ? SDL_SCANCODE_PAUSE : SDL_SCANCODE_UNKNOWN;
    }
    /* As in SDL, only the record's own E0 flag marks an extended key, also
     * when the make code came from the virtual key above with an 0xE0 high
     * byte. The helper process reports such a key the same way, and the code
     * here has to be the one the helper reports. */
    unsigned index = LOBYTE(make_code);
    if (flags & RI_KEY_E0) index |= 0x80;
    return scancode_at(index);
}

/* wParam and lParam of WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN or WM_SYSKEYUP. */
int sf_host_keyboard_message_scancode(UINT_PTR wparam, LONG_PTR lparam)
{
    unsigned flags = HIWORD(lparam);
    unsigned scan_code = LOBYTE(flags) & ~0x80u;
    if (scan_code) {
        if (flags & KF_EXTENDED) {
            scan_code |= 0xe000;
        } else if (scan_code == 0x45) {
            scan_code = 0xe046; /* Pause */
        }
    } else {
        scan_code = LOWORD(MapVirtualKeyW(LOWORD(wparam), MAPVK_VK_TO_VSC_EX));
        if (scan_code == 0xe11d) scan_code = 0xe046;
    }
    return scancode_at(LOBYTE(scan_code) | (HIBYTE(scan_code) ? 0x80 : 0));
}

/* The key's name in the system keyboard layout. A host-fed process has no SDL
 * keymap, and SDL's scancode names alone would be those of a US keyboard. */
int sf_host_keyboard_label(int scancode, char *output, int size)
{
    if (!output || size <= 0) return 0;
    output[0] = '\0';
    if (scancode <= SDL_SCANCODE_UNKNOWN || scancode >= SDL_SCANCODE_COUNT) return 0;

    LONG key = 0;
    if (scancode == SDL_SCANCODE_PAUSE) {
        /* GetKeyNameText tells these two apart the other way round. */
        key = 0x45L << 16;
    } else if (scancode == SDL_SCANCODE_NUMLOCKCLEAR) {
        key = (0x45L << 16) | (1L << 24);
    } else {
        for (unsigned index = 1; index < SDL_arraysize(windows_scancode_table); ++index) {
            if ((int)windows_scancode_table[index] == scancode) {
                key = ((LONG)(index & 0x7f) << 16) | ((index & 0x80) ? (1L << 24) : 0);
                break;
            }
        }
    }
    if (key) {
        WCHAR name[64];
        int length = GetKeyNameTextW(key, name, (int)SDL_arraysize(name));
        if (length > 0) {
            int written = WideCharToMultiByte(CP_UTF8, 0, name, length, output, size - 1, NULL, NULL);
            if (written > 0) {
                output[written] = '\0';
                return written;
            }
        }
    }
    SDL_strlcpy(output, SDL_GetScancodeName((SDL_Scancode)scancode), (size_t)size);
    return (int)SDL_strlen(output);
}
