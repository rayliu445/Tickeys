//! Global keyboard monitor built on a listen-only CGEventTap.
//!
//! Minimal hand-written FFI over CoreGraphics / CoreFoundation: the modern
//! `core-graphics` crate's safe wrappers churn across versions, and we need
//! exactly four C functions here.

use core_foundation_sys::base::{CFRelease, kCFAllocatorDefault};
use core_foundation_sys::mach_port::{CFMachPortCreateRunLoopSource, CFMachPortRef};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopRef, CFRunLoopRemoveSource,
    CFRunLoopSourceRef, kCFRunLoopCommonModes,
};
use std::os::raw::{c_void};

pub type CGEventRef = *mut c_void;

type CGEventTapCallBackRaw = Option<
    unsafe extern "C" fn(
        proxy: *mut c_void,
        event_type: u32,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef,
>;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: CGEventTapCallBackRaw,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
}

// CGEventTapLocation
const K_CG_SESSION_EVENT_TAP: u32 = 1;
// CGEventTapPlacement
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
// CGEventTapOptions
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
// CGEventType
const K_CG_EVENT_KEY_DOWN: u32 = 10;
// CGEventField::kCGKeyboardEventKeycode
pub const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

pub struct KeyboardMonitor {
    event_tap: CFMachPortRef,
    runloop_source: CFRunLoopSourceRef,
    runloop: CFRunLoopRef,
}

pub type KeyboardEventHandler = unsafe extern "C" fn(
    proxy: *mut c_void,
    event_type: u32,
    event: CGEventRef,
    refcon: *mut c_void,
) -> CGEventRef;

impl KeyboardMonitor {
    /// The tap is added to the MAIN run loop. `new` must therefore be called
    /// on the main thread before/while the app runs (same as upstream).
    pub fn new(handler: KeyboardEventHandler, user_data: *mut c_void) -> Result<KeyboardMonitor, String> {
        unsafe {
            let event_tap = CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                1u64 << K_CG_EVENT_KEY_DOWN,
                Some(handler),
                user_data,
            );

            if event_tap.is_null() {
                return Err("failed to CGEventTapCreate (is Accessibility permission granted?)".to_string());
            }

            let runloop = CFRunLoopGetMain();
            let runloop_source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, event_tap, 0);
            if runloop_source.is_null() {
                CFRelease(event_tap as *const c_void);
                return Err("failed to CFMachPortCreateRunLoopSource".to_string());
            }

            CFRunLoopAddSource(runloop, runloop_source, kCFRunLoopCommonModes);

            Ok(KeyboardMonitor {
                event_tap,
                runloop_source,
                runloop,
            })
        }
    }

    #[allow(dead_code)]
    pub fn set_enabled(&mut self, enabled: bool) {
        unsafe { CGEventTapEnable(self.event_tap, enabled) }
    }

    #[allow(dead_code)]
    pub fn is_enabled(&mut self) -> bool {
        unsafe { CGEventTapIsEnabled(self.event_tap) }
    }

    /// Read the keycode out of a raw CGEvent (used from the tap callback).
    pub fn keycode_of_event(event: CGEventRef) -> u16 {
        unsafe {
            CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16
        }
    }
}

impl Drop for KeyboardMonitor {
    fn drop(&mut self) {
        self.set_enabled(false);
        unsafe {
            CFRunLoopRemoveSource(self.runloop, self.runloop_source, kCFRunLoopCommonModes);
            CFRelease(self.event_tap as *const c_void);
            CFRelease(self.runloop_source as *const c_void);
        }
    }
}
