// SPDX-License-Identifier: MIT
//! Kiln layer: C entry points for a host that feeds ShroudForge's shortcut state.
//!
//! The injected runtime normally takes key state from a hidden
//! `shroudforge.exe --keyboard-input` child. A host that already sees the game
//! window's input (Kiln) can pass the window's own key records here. The
//! runtime then starts no helper process, and `shroudforge.input` answers from
//! what the host passed. Without a call to `shroudforge_input_host_begin`
//! nothing changes.
//!
//! The functions are exported from `shroudforge-runtime.dll` and declared for
//! C in `kiln/abi/input_host.h`. Any thread may call them: one call runs at a
//! time. A host calls `begin` before `shroudforge_create`.

use crate::observe_keyboard::{
    host_begin, host_end, host_key, host_key_is_down, host_mouse_button, host_reset,
};
use std::{
    ffi::{CStr, c_char},
    panic::catch_unwind,
    sync::{Mutex, MutexGuard, PoisonError},
};

unsafe extern "C" {
    fn sf_host_keyboard_raw_scancode(
        make_code: u32,
        flags: u32,
        vkey: u32,
        pending_e1: *mut i32,
    ) -> i32;
    fn sf_host_keyboard_message_scancode(wparam: usize, lparam: isize) -> i32;
    fn sf_host_keyboard_label(scancode: i32, output: *mut c_char, size: i32) -> i32;
    fn sf_keyboard_scancode_count() -> i32;
}

/// Raised when a function is added or one of these changes its meaning.
const INPUT_HOST_VERSION: u32 = 1;
/// RI_KEY_BREAK of RAWKEYBOARD.Flags: the key was released.
const RAW_KEY_BREAK: u32 = 1;
const SDL_SCANCODE_PRINTSCREEN: u16 = 70;

/// What one call leaves for the next. Every function below holds this lock
/// for its whole turn, so two threads cannot interleave their steps: a
/// modifier that one of them recorded would otherwise be overwritten with the
/// other's older view. The mods' queries never take it.
struct Feed {
    /// The E1 prefix of Pause, from one raw record to the next.
    pending_e1: bool,
}

static FEED: Mutex<Feed> = Mutex::new(Feed { pending_e1: false });

fn feed() -> MutexGuard<'static, Feed> {
    FEED.lock().unwrap_or_else(PoisonError::into_inner)
}

#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_version() -> u32 {
    INPUT_HOST_VERSION
}

/// Returns 1 when the host now owns the shortcut state, and 0 when
/// ShroudForge's own keyboard input is already running in this process.
#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_begin() -> u32 {
    catch_unwind(|| {
        let mut feed = feed();
        feed.pending_e1 = false;
        u32::from(host_begin())
    })
    .unwrap_or(0)
}

/// One keyboard record of the game window's WM_INPUT: `RAWKEYBOARD.MakeCode`,
/// `.Flags` and `.VKey`, unchanged.
#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_raw_key(make_code: u32, flags: u32, vkey: u32) {
    let _ = catch_unwind(|| {
        let mut feed = feed();
        let mut pending_e1 = i32::from(feed.pending_e1);
        let scancode =
            unsafe { sf_host_keyboard_raw_scancode(make_code, flags, vkey, &mut pending_e1) };
        feed.pending_e1 = pending_e1 != 0;
        if let Ok(scancode) = u16::try_from(scancode) {
            host_key(scancode, flags & RAW_KEY_BREAK == 0);
        }
    });
}

/// `wParam` and `lParam` of the game window's WM_KEYDOWN, WM_KEYUP,
/// WM_SYSKEYDOWN or WM_SYSKEYUP, for a window that gets no raw keyboard
/// input. Bit 31 of `lParam` tells a release from a press.
#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_window_key(wparam: usize, lparam: isize) {
    let _ = catch_unwind(|| {
        let _feed = feed();
        let scancode = unsafe { sf_host_keyboard_message_scancode(wparam, lparam) };
        if let Ok(scancode) = u16::try_from(scancode) {
            let down = lparam & (1 << 31) == 0;
            // Windows sends a window only the release of Print Screen. As SDL
            // does for its own window, count the press that never arrived.
            if scancode == SDL_SCANCODE_PRINTSCREEN && !down && !host_key_is_down(scancode) {
                host_key(scancode, true);
            }
            host_key(scancode, down);
        }
    });
}

/// `button`: 1 left, 2 middle, 3 right, 4 and 5 the side buttons.
#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_mouse_button(button: u32, down: u32) {
    let _ = catch_unwind(|| {
        let _feed = feed();
        if let Ok(button) = u8::try_from(button) {
            host_mouse_button(button, down != 0);
        }
    });
}

/// The game window lost the keyboard focus: every key and button counts as
/// released, because the window will not see their releases.
#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_reset() {
    let _ = catch_unwind(|| {
        let mut feed = feed();
        feed.pending_e1 = false;
        host_reset();
    });
}

/// The host stops feeding. Shortcut queries answer "not held" afterwards.
#[unsafe(no_mangle)]
pub extern "C" fn shroudforge_input_host_end() {
    let _ = catch_unwind(|| {
        let _feed = feed();
        host_end();
    });
}

/// Key names by SDL scancode, as the helper process sends them.
pub(crate) fn labels() -> Vec<String> {
    (0..unsafe { sf_keyboard_scancode_count() })
        .map(|scancode| {
            let mut name = [0 as c_char; 96];
            unsafe {
                sf_host_keyboard_label(scancode, name.as_mut_ptr(), name.len() as i32);
                CStr::from_ptr(name.as_ptr())
            }
            .to_string_lossy()
            .into_owned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hotkey_press_revision, initialize_runtime, is_hotkey_down, parse_hotkey};
    use std::path::Path;

    const E0: u32 = 2;
    const E1: u32 = 4;

    fn raw(make_code: u32, flags: u32) -> i32 {
        let mut pending = 0;
        unsafe { sf_host_keyboard_raw_scancode(make_code, flags, 0, &mut pending) }
    }

    fn message(scan_code: isize, extended: bool) -> i32 {
        let lparam = 1 | scan_code << 16 | isize::from(extended) << 24;
        unsafe { sf_host_keyboard_message_scancode(0, lparam) }
    }

    #[test]
    fn window_key_records_get_sdl_scancodes() {
        assert_eq!(raw(0x43, 0), 66, "F9");
        assert_eq!(raw(0x1e, 0), 4, "A");
        assert_eq!(raw(0x1d, 0), 224, "left Ctrl");
        assert_eq!(raw(0x1d, E0), 228, "right Ctrl");
        assert_eq!(raw(0x38, E0), 230, "right Alt");
        assert_eq!(raw(0x48, E0), 82, "Up");
        assert_eq!(raw(0x48, 0), 96, "keypad 8");
        assert_eq!(raw(0x45, 0), 83, "Num Lock");
        assert_eq!(raw(0x2a, E0), 0, "the fake shift before Print Screen is no key");
        assert_eq!(raw(0x37, E0), 70, "Print Screen");

        let mut pending = 0;
        let prefix = unsafe { sf_host_keyboard_raw_scancode(0x1d, E1, 0, &mut pending) };
        assert_eq!((prefix, pending), (0, 1), "the first record of Pause is a prefix");
        let pause = unsafe { sf_host_keyboard_raw_scancode(0x45, 0, 0, &mut pending) };
        assert_eq!((pause, pending), (72, 0), "Pause");

        assert_eq!(message(0x43, false), 66, "F9");
        assert_eq!(message(0x1d, true), 228, "right Ctrl");
        assert_eq!(message(0x45, false), 72, "Pause has no extended bit in a window message");
        assert_eq!(message(0x45, true), 83, "Num Lock has it");
    }

    // One test: the shortcut state is one per process.
    #[test]
    fn a_host_feeds_the_shortcut_state_without_a_helper_process() {
        let key = |text: &str| parse_hotkey(text).unwrap().unwrap();
        let f9 = key("SDL:66");
        let ctrl_f9 = key("Ctrl+SDL:66");
        let right_ctrl = key("SDL:228");
        let mouse4 = key("Mouse:4");
        let shift_mouse4 = key("Shift+Mouse:4");
        const UP: u32 = RAW_KEY_BREAK;
        const KEY_UP: isize = 1 << 31;

        shroudforge_input_host_raw_key(0x43, 0, 0);
        assert!(!is_hotkey_down(&f9), "nothing is recorded before the host begins");

        assert_eq!(shroudforge_input_host_version(), 1);
        assert_eq!(shroudforge_input_host_begin(), 1);
        assert_eq!(
            initialize_runtime(Path::new("Z:\\missing\\shroudforge.exe")),
            Ok(()),
            "the runtime asks for no helper process while a host feeds it"
        );
        assert_eq!(crate::observe_keyboard::key_label(66), "F9");
        assert!(!crate::observe_keyboard::key_label(4).is_empty());

        // A held key: down once, auto-repeat does not count as a new press.
        let before = hotkey_press_revision(&f9);
        shroudforge_input_host_raw_key(0x43, 0, 0);
        assert!(is_hotkey_down(&f9));
        assert!(!is_hotkey_down(&ctrl_f9));
        let pressed = hotkey_press_revision(&f9);
        assert!(pressed > before);
        shroudforge_input_host_raw_key(0x43, 0, 0);
        assert_eq!(hotkey_press_revision(&f9), pressed);
        shroudforge_input_host_raw_key(0x43, UP, 0);
        assert!(!is_hotkey_down(&f9));
        assert_eq!(hotkey_press_revision(&f9), pressed, "a release is no press");

        // A chord, through window messages; either Ctrl key counts.
        shroudforge_input_host_window_key(0, 1 | 0x1d << 16 | 1 << 24);
        assert!(is_hotkey_down(&right_ctrl), "a Ctrl key alone is a binding too");
        shroudforge_input_host_window_key(0, 1 | 0x43 << 16);
        assert!(is_hotkey_down(&ctrl_f9));
        assert!(!is_hotkey_down(&f9), "F9 alone is another binding");
        assert!(hotkey_press_revision(&ctrl_f9) > pressed);
        assert_eq!(hotkey_press_revision(&f9), pressed);
        shroudforge_input_host_window_key(0, KEY_UP | 1 | 0x1d << 16 | 1 << 24);
        assert!(!is_hotkey_down(&ctrl_f9), "the chord ends with its modifier");
        shroudforge_input_host_window_key(0, KEY_UP | 1 | 0x43 << 16);

        // Mouse buttons take the modifiers held at their press.
        shroudforge_input_host_mouse_button(4, 1);
        assert!(is_hotkey_down(&mouse4));
        shroudforge_input_host_mouse_button(4, 0);
        assert!(!is_hotkey_down(&mouse4));
        shroudforge_input_host_raw_key(0x36, 0, 0);
        shroudforge_input_host_mouse_button(4, 1);
        assert!(is_hotkey_down(&shift_mouse4));
        assert!(!is_hotkey_down(&mouse4));

        // Focus loss: the releases never arrive.
        shroudforge_input_host_reset();
        assert!(!is_hotkey_down(&shift_mouse4));
        shroudforge_input_host_raw_key(0x43, 0, 0);
        assert!(is_hotkey_down(&f9), "the Shift held before the focus loss is forgotten");

        // Print Screen in window messages: Windows sends its release only.
        let print_screen = key("SDL:70");
        let before = hotkey_press_revision(&print_screen);
        shroudforge_input_host_window_key(0x2c, KEY_UP | 1 | 0x37 << 16 | 1 << 24);
        let pressed_once = hotkey_press_revision(&print_screen);
        assert!(pressed_once > before, "the release alone counts as one press");
        assert!(!is_hotkey_down(&print_screen));
        // Through raw input both arrive, and the release adds no second press.
        shroudforge_input_host_raw_key(0x37, E0, 0x2c);
        let pressed_twice = hotkey_press_revision(&print_screen);
        assert!(pressed_twice > pressed_once);
        shroudforge_input_host_window_key(0x2c, KEY_UP | 1 | 0x37 << 16 | 1 << 24);
        assert_eq!(hotkey_press_revision(&print_screen), pressed_twice);

        // Two threads: whatever the order, both modifiers are held at the end.
        shroudforge_input_host_reset();
        let hammer = |make_code: u32| {
            std::thread::spawn(move || {
                for _ in 0..20_000 {
                    shroudforge_input_host_raw_key(make_code, 0, 0);
                    shroudforge_input_host_raw_key(make_code, UP, 0);
                }
                shroudforge_input_host_raw_key(make_code, 0, 0);
            })
        };
        let (ctrl, shift) = (hammer(0x1d), hammer(0x2a));
        ctrl.join().unwrap();
        shift.join().unwrap();
        assert_eq!(crate::observe_keyboard::host_modifiers(), 1 | 4, "Ctrl and Shift");

        let last_press = hotkey_press_revision(&f9);
        assert!(last_press > 0);
        shroudforge_input_host_end();
        assert!(!is_hotkey_down(&f9));
        assert_eq!(hotkey_press_revision(&f9), 0);
        assert!(initialize_runtime(Path::new("Z:\\missing\\shroudforge.exe")).is_err());
        shroudforge_input_host_raw_key(0x43, 0, 0);
        assert!(!is_hotkey_down(&f9), "nothing is recorded after the host ended");

        assert_eq!(shroudforge_input_host_begin(), 1, "a host may begin again");
        assert!(!is_hotkey_down(&f9), "a new feed starts with nothing held");
        assert_eq!(
            hotkey_press_revision(&f9),
            0,
            "a press from before the end is not a new press: a mod that asked in between saw 0"
        );
        shroudforge_input_host_raw_key(0x43, 0, 0);
        assert!(is_hotkey_down(&f9));
        assert!(hotkey_press_revision(&f9) > last_press, "a new press is newer than any before");
        shroudforge_input_host_end();
    }
}
