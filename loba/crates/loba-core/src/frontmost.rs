//! Which app (and window) the user is dictating into.
//!
//! macOS: NSWorkspace for the app, the Accessibility API for the focused
//! window's title (L.O.B.A already holds Accessibility permission for paste).
//! The AX call gets a 250 ms messaging timeout — a hung target app must never
//! stall the hotkey. Elsewhere this returns an empty context.

use crate::styles::AppContext;

#[cfg(target_os = "macos")]
pub fn frontmost() -> AppContext {
    objc2::rc::autoreleasepool(|_| mac::frontmost())
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost() -> AppContext {
    AppContext::default()
}

/// System Settings → Accessibility → Display → Reduce motion. Animated
/// affordances (the pulsing menu bar glyph) stay still when this is on.
#[cfg(target_os = "macos")]
pub fn reduce_motion() -> bool {
    objc2::rc::autoreleasepool(|_| {
        #[allow(unused_unsafe)]
        unsafe {
            objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
        }
    })
}

#[cfg(not(target_os = "macos"))]
pub fn reduce_motion() -> bool {
    false
}

#[cfg(target_os = "macos")]
mod mac {
    use super::AppContext;
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use objc2_app_kit::NSWorkspace;
    use std::ffi::c_void;

    type AXUIElementRef = *const c_void;
    type AXError = i32;
    const AX_SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> AXError;
    }

    #[allow(unused_unsafe)]
    pub fn frontmost() -> AppContext {
        let workspace = unsafe { NSWorkspace::sharedWorkspace() };
        let Some(app) = (unsafe { workspace.frontmostApplication() }) else {
            return AppContext::default();
        };
        let name = unsafe { app.localizedName() }.map(|s| s.to_string());
        let bundle_id = unsafe { app.bundleIdentifier() }.map(|s| s.to_string());
        let pid = unsafe { app.processIdentifier() };
        AppContext {
            name,
            bundle_id,
            window_title: window_title(pid),
        }
    }

    fn copy_attr(element: AXUIElementRef, name: &'static str) -> Option<CFType> {
        let attr = CFString::from_static_string(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: element is a live AXUIElement; attr outlives the call; on
        // success we own `value` (Copy rule) and hand it to CFType.
        let err = unsafe {
            AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value)
        };
        if err != AX_SUCCESS || value.is_null() {
            return None;
        }
        Some(unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn window_title(pid: i32) -> Option<String> {
        // SAFETY: Create rule — released by CFType's Drop below.
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return None;
        }
        let app = unsafe { CFType::wrap_under_create_rule(app as CFTypeRef) };
        let app_ref = app.as_CFTypeRef() as AXUIElementRef;
        unsafe { AXUIElementSetMessagingTimeout(app_ref, 0.25) };

        let window = copy_attr(app_ref, "AXFocusedWindow")?;
        let title = copy_attr(window.as_CFTypeRef() as AXUIElementRef, "AXTitle")?;
        title.downcast_into::<CFString>().map(|s| s.to_string())
    }
}
