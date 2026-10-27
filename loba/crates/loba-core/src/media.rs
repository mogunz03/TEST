//! Pause whatever is playing while you dictate, then resume it.
//!
//! macOS has no "pause the current player" call. Media apps follow the
//! keyboard's play/pause key, which macOS routes to the app it last saw
//! playing. Pressing it blind is dangerous: with nothing playing it *starts*
//! Music. So the key is only pressed when Core Audio says another app is
//! producing sound right now, and that app is a media player or browser.
//! Calls are never paused (a video-call app pressing play/pause does
//! nothing useful, and the key would go to Music instead).
//!
//! Resume presses the key again only if we paused and nothing has started
//! playing since, so a user who resumed by hand isn't toggled back off.

use std::sync::atomic::{AtomicBool, Ordering};

static PAUSED_BY_US: AtomicBool = AtomicBool::new(false);

/// Apps whose sound means "media the user would want paused". Matched as
/// bundle-id prefixes, so browser helper processes count too (Chrome plays
/// audio from com.google.Chrome.helper, Safari from com.apple.WebKit.GPU).
const MEDIA_APPS: &[&str] = &[
    "com.apple.Music",
    "com.apple.podcasts",
    "com.apple.TV",
    "com.apple.QuickTimePlayerX",
    "com.apple.Safari",
    "com.apple.WebKit.GPU",
    "com.spotify.client",
    "com.google.Chrome",
    "company.thebrowser.Browser",
    "com.microsoft.edgemac",
    "com.brave.Browser",
    "org.mozilla.firefox",
    "com.operasoftware.Opera",
    "com.vivaldi.Vivaldi",
    "org.videolan.vlc",
    "com.colliderli.iina",
    "com.netease.163music",
    "com.tencent.QQMusicMac",
];

/// If any of these is producing sound, a call is likely on: leave it alone.
const CALL_APPS: &[&str] = &[
    "us.zoom.xos",
    "com.microsoft.teams",
    "com.apple.FaceTime",
    "com.tencent.xinWeChat",
    "com.tencent.meeting",
    "com.hnc.Discord",
    "com.skype.skype",
    "com.cisco.webexmeetingsapp",
    "com.tinyspeck.slackmacgap",
    "com.electron.lark",
    "com.bytedance.feishu",
];

fn matches(list: &[&str], bundle: &str) -> bool {
    list.iter().any(|p| bundle.starts_with(p))
}

/// What to do given the apps producing sound right now.
pub fn should_pause(playing: &[String]) -> bool {
    !playing.iter().any(|b| matches(CALL_APPS, b)) && playing.iter().any(|b| matches(MEDIA_APPS, b))
}

/// Recording started: pause media if some is playing. Returns whether the
/// key was pressed.
pub fn pause() -> bool {
    let Some(playing) = imp::playing_bundle_ids() else {
        log::debug!("Pause media: can't list audio clients on this system");
        return false;
    };
    if !should_pause(&playing) {
        log::debug!("Pause media: nothing to pause ({playing:?})");
        return false;
    }
    if PAUSED_BY_US.swap(true, Ordering::SeqCst) {
        return false; // already paused by an earlier start
    }
    log::info!("Pausing media for dictation ({playing:?})");
    imp::press_play_pause();
    true
}

/// Recording stopped or was cancelled: resume what we paused.
pub fn resume() {
    if !PAUSED_BY_US.swap(false, Ordering::SeqCst) {
        return;
    }
    // Something is playing again (the user resumed it): leave it be.
    if imp::playing_bundle_ids().is_some_and(|p| p.iter().any(|b| matches(MEDIA_APPS, b))) {
        log::info!("Media already playing again; not toggling");
        return;
    }
    log::info!("Resuming media after dictation");
    imp::press_play_pause();
}

#[cfg(target_os = "macos")]
mod imp {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_core_graphics::{CGEvent, CGEventTapLocation};
    use objc2_foundation::NSPoint;
    use std::ffi::c_void;
    use std::ptr;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const SYSTEM_OBJECT: u32 = 1;
    const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const ELEMENT_MAIN: u32 = 0;
    // macOS 14.2+: one audio object per client process.
    const PROCESS_LIST: u32 = u32::from_be_bytes(*b"prs#");
    const PROCESS_PID: u32 = u32::from_be_bytes(*b"ppid");
    const PROCESS_BUNDLE_ID: u32 = u32::from_be_bytes(*b"pbid");
    const PROCESS_RUNNING_OUTPUT: u32 = u32::from_be_bytes(*b"piro");

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    fn address(selector: u32) -> Address {
        Address {
            selector,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        }
    }

    /// Read a fixed-size property. `None` if Core Audio refuses.
    fn get<T: Copy + Default>(object: u32, selector: u32) -> Option<T> {
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address(selector),
                0,
                ptr::null(),
                &mut size,
                &mut value as *mut T as *mut c_void,
            )
        };
        (status == 0).then_some(value)
    }

    /// Bundle ids of other processes producing sound right now. `None` on
    /// macOS older than 14.2, where the process list doesn't exist.
    pub fn playing_bundle_ids() -> Option<Vec<String>> {
        let list = address(PROCESS_LIST);
        let mut size = 0u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &list, 0, ptr::null(), &mut size)
        };
        if status != 0 {
            return None;
        }
        let mut ids = vec![0u32; size as usize / std::mem::size_of::<u32>()];
        if !ids.is_empty() {
            let status = unsafe {
                AudioObjectGetPropertyData(
                    SYSTEM_OBJECT,
                    &list,
                    0,
                    ptr::null(),
                    &mut size,
                    ids.as_mut_ptr() as *mut c_void,
                )
            };
            if status != 0 {
                return None;
            }
        }
        let me = std::process::id() as i32;
        let mut out = Vec::new();
        for id in ids {
            if get::<i32>(id, PROCESS_PID) == Some(me) {
                continue;
            }
            if get::<u32>(id, PROCESS_RUNNING_OUTPUT).unwrap_or(0) == 0 {
                continue;
            }
            // Returned retained; the wrapper releases it.
            let raw: usize = get::<usize>(id, PROCESS_BUNDLE_ID).unwrap_or(0);
            if raw == 0 {
                continue;
            }
            let s = unsafe { CFString::wrap_under_create_rule(raw as CFStringRef) };
            out.push(s.to_string());
        }
        Some(out)
    }

    const NX_KEYTYPE_PLAY: isize = 16;

    /// The keyboard's play/pause key, down then up, as a system-defined
    /// event at the HID level (needs Accessibility, which paste already has).
    pub fn press_play_pause() {
        objc2::rc::autoreleasepool(|_| {
            for down in [true, false] {
                let state: isize = if down { 0xa } else { 0xb };
                let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                    NSEventType::SystemDefined,
                    NSPoint::new(0.0, 0.0),
                    NSEventModifierFlags((state as usize) << 8),
                    0.0,
                    0,
                    None,
                    8,
                    (NX_KEYTYPE_PLAY << 16) | (state << 8),
                    -1,
                );
                if let Some(cg) = event.as_ref().and_then(|e| e.CGEvent()) {
                    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&cg));
                }
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn playing_bundle_ids() -> Option<Vec<String>> {
        None
    }
    pub fn press_play_pause() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn pauses_media_and_browsers_only() {
        assert!(should_pause(&s(&["com.spotify.client"])));
        assert!(should_pause(&s(&["com.google.Chrome.helper"])));
        assert!(should_pause(&s(&["com.apple.WebKit.GPU"])));
        assert!(!should_pause(&s(&[])));
        // A notification sound must not trigger the key (it would start Music).
        assert!(!should_pause(&s(&["com.apple.notificationcenterui"])));
    }

    #[test]
    fn never_pauses_during_a_call() {
        assert!(!should_pause(&s(&["us.zoom.xos", "com.spotify.client"])));
        assert!(!should_pause(&s(&["com.tencent.xinWeChat"])));
    }

    #[test]
    fn off_macos_nothing_is_pressed() {
        assert!(!pause());
        resume();
    }
}
