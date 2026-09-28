//! Physical modifier state. Command mode sends a synthetic ⌘C to grab the
//! selection; if the user is still holding ⌥/⇧ from the hotkey, apps see
//! ⌥⇧⌘C instead. So we wait for the hand to come off the keys first.

use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
fn modifiers_held() -> bool {
    type CGEventSourceStateID = i32;
    const HID_SYSTEM_STATE: CGEventSourceStateID = 1;
    const SHIFT: u64 = 0x0002_0000;
    const CONTROL: u64 = 0x0004_0000;
    const OPTION: u64 = 0x0008_0000;
    const COMMAND: u64 = 0x0010_0000;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state: CGEventSourceStateID) -> u64;
    }
    // SAFETY: pure query of global keyboard state.
    let flags = unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) };
    flags & (SHIFT | CONTROL | OPTION | COMMAND) != 0
}

#[cfg(not(target_os = "macos"))]
fn modifiers_held() -> bool {
    false
}

/// Block until no modifier is physically down, or `limit` passes. Returns
/// whether the keys came up in time.
pub fn wait_for_release(limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while modifiers_held() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}
