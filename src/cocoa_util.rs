//! Small helpers bridging Rust and Cocoa, built on objc2.
//!
//! This replaces the old hand-rolled cocoa/objc 0.1 layer.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSBundle, NSString};

pub type Id = *mut NSObject;
pub const NIL: Id = std::ptr::null_mut();

/// `localizedStringForKey:value:table:` with `value:` bound to the key itself.
///
/// Root-cause fix for the historical "blank rows in the dropdown" bug: the old
/// code passed `value:@""`, so any missing key rendered as an empty string.
/// Falling back to the key means a missing translation shows the raw name
/// (e.g. "Drum") instead of an invisible row.
pub fn l10n_str(key: &str) -> Retained<NSString> {
    unsafe {
        let bundle = NSBundle::mainBundle();
        let key_ns = NSString::from_str(key);
        let s: Option<Retained<NSString>> = msg_send![
            &bundle,
            localizedStringForKey: &*key_ns,
            value: &*key_ns,
            table: NIL
        ];
        s.unwrap_or(key_ns)
    }
}

/// Autoreleased NSString as a raw id -- convenient for msg_send arguments.
pub fn nsstr(s: &str) -> Id {
    let s = NSString::from_str(s);
    // +1 -> +0, registered in the current autorelease pool.
    unsafe { objc2::ffi::objc_autorelease(Retained::into_raw(s).cast()) as Id }
}

/// Path of `sub_path` inside the running bundle's Resources directory.
///
/// Uses argv[0] (…/Contents/MacOS/Tickeys → ../Resources/…), same as the
/// original 0.5.0 code. Running outside an .app falls back to `./<sub_path>`.
pub fn get_res_path(sub_path: &str) -> String {
    let args: Vec<_> = std::env::args().collect();
    let mut data_path = std::path::PathBuf::from(args.first().map(|s| s.as_str()).unwrap_or("."));
    data_path.pop();
    data_path.push("../Resources/");
    data_path.push(sub_path);

    if data_path.exists() {
        data_path.into_os_string().into_string().unwrap_or_default()
    } else {
        sub_path.to_string()
    }
}

pub fn app_run<T: objc2::ClassType + NSApplicationDelegate>(delegate: &T) {
    let mtm = objc2::MainThreadMarker::new().expect("app_run must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // setDelegate retains the delegate for the app's lifetime.
    app.setDelegate(Some(ProtocolObject::from_ref(delegate)));
    app.run();
}

pub fn app_terminate() {
    let mtm = objc2::MainThreadMarker::new().expect("app_terminate must run on the main thread");
    unsafe {
        let app = NSApplication::sharedApplication(mtm);
        let _: () = msg_send![&app, terminate: NIL];
    }
}

/// Relaunch ourselves (used after Accessibility permission is granted and
/// after the system wakes up, where a fresh launch is the simplest way to get
/// a working event tap again).
pub fn app_relaunch_self() {
    let args: Vec<_> = std::env::args().collect();
    if let Some(exe) = args.first() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0);
}

pub fn nsurl_filename(nsurl: Id) -> Id {
    unsafe {
        let path_components: Id = msg_send![nsurl, pathComponents];
        msg_send![path_components, lastObject]
    }
}
