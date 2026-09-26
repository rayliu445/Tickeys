//! App entry point: AppDelegate, menu-bar item, Accessibility permission flow.

mod audio;
mod cocoa_util;
mod consts;
mod event_tap;
mod pref;
mod settings_ui;
mod tickeys;

use std::cell::Cell;
use std::io::Read;
use std::os::raw::c_void;

use objc2::class;
use objc2::define_class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::sel;
use objc2::runtime::ProtocolObject;
use objc2::DefinedClass;
use objc2::MainThreadOnly;
use objc2::AnyThread;
use objc2_app_kit::{NSAlert, NSApplicationDelegate, NSImage, NSMenu, NSMenuItem, NSStatusBar};
use objc2_foundation::{NSString, NSUserDefaults};

use crate::cocoa_util::{Id, NIL};
use crate::consts::OPEN_SETTINGS_KEY_SEQ;
use crate::pref::Pref;
use crate::settings_ui::SettingsController;
use crate::tickeys::Tickeys;

extern "C" {
    static NSWorkspaceDidActivateApplicationNotification: Id;
    static NSWorkspaceDidWakeNotification: Id;
    static NSWorkspaceApplicationKey: Id;
}

#[derive(Default)]
pub struct AppDelegateIvars {
    tickeys: Cell<usize>,
    filter_list: Cell<Id>,
    filter_list_mode: Cell<isize>,
    status_item: Cell<Id>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "AppDelegate"]
    #[ivars = AppDelegateIvars]
    pub struct AppDelegate;

    impl AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn application_did_finish_launching(&self, _note: Id) {
            // 菜单栏图标第一个装上：这样即便后面卡在辅助功能授权弹窗上，
            // 用户也能看到程序确实起来了、并且能从菜单里退出。
            Self::install_status_item(self);

            // Blocks in a modal loop until the user grants Accessibility,
            // then relaunches the process (never returns unless trusted).
            Self::request_ax();

            let sch = Self::load_schemes();
            let pref = Pref::load(&sch);
            let mut tickeys = Box::new(Tickeys::new(sch));
            tickeys.load_scheme(
                &cocoa_util::get_res_path(&format!("data/{}", pref.scheme)),
                &pref.scheme,
            );
            tickeys.set_volume(pref.volume);
            tickeys.set_pitch(pref.pitch);
            tickeys.set_on_keydown(Some(Self::handle_keydown)); // handles QAZ123
            tickeys.start();

            // Hand ownership over to the app delegate ivar.
            self.ivars().tickeys.set(Box::into_raw(tickeys) as usize);

            unsafe {
                // Observe workspace notifications: app activation (mute list)
                // and system wake (the event tap can die over sleep, so we
                // relaunch -- same fix as upstream 0.4.2, but via NSWorkspace
                // instead of IOKit).
                let workspace: Id = msg_send![class!(NSWorkspace), sharedWorkspace];
                let center: Id = msg_send![workspace, notificationCenter];

                let _: () = msg_send![center,
                    addObserver: self as *const Self as Id,
                    selector: sel!(workspace_app_activated:),
                    name: NSWorkspaceDidActivateApplicationNotification,
                    object: NIL];

                let _: () = msg_send![center,
                    addObserver: self as *const Self as Id,
                    selector: sel!(workspace_wake:),
                    name: NSWorkspaceDidWakeNotification,
                    object: NIL];

                // Observe FilterListMode changes from the settings UI (KVO)
                let ud_controller: Id = msg_send![class!(NSUserDefaultsController), sharedUserDefaultsController];
                let _: () = msg_send![ud_controller,
                    addObserver: self as *const Self as Id,
                    forKeyPath: cocoa_util::nsstr("values.FilterListMode"),
                    options: 1i64, // NSKeyValueObservingOptionNew
                    context: std::ptr::null::<c_void>()];

                // Apply mute state for the currently frontmost app
                let front_app: Id = msg_send![workspace, frontmostApplication];
                if !front_app.is_null() {
                    let bundle_url: Id = msg_send![front_app, bundleURL];
                    if !bundle_url.is_null() {
                        let name: Id = cocoa_util::nsurl_filename(bundle_url);
                        Self::check_and_apply_mute_for_app(self, name);
                    }
                }
            }

            // 启动完成后把设置窗口显示出来 —— 这是唯一能让用户“看见程序开了”的反馈。
            // 只在启动时显示这一次；applicationDidBecomeActive 里不显示，
            // 避免反复抢走其它程序的焦点。
            Self::show_settings(self);
        }

        #[unsafe(method(applicationDidBecomeActive:))]
        fn application_did_become_active(&self, _note: Id) {
            // 故意不自动弹设置窗口：会反复抢焦点（详见 0.5.0 之后的修改记录）。
            println!("applicationDidBecomeActive (不自动开设置窗口)");
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn application_will_terminate(&self, _note: Id) {
            // Drop the Tickeys instance -- but only if one was actually created.
            // Terminating from the permission alert lands here while the ivar
            // is still 0.
            let ptr = self.ivars().tickeys.get();
            if ptr == 0 {
                println!("applicationWillTerminate: no Tickeys instance yet, nothing to drop");
                return;
            }
            self.ivars().tickeys.set(0);
            unsafe { drop(Box::from_raw(ptr as *mut Tickeys)) };
        }

        #[unsafe(method(workspace_app_activated:))]
        fn workspace_app_activated(&self, noti: Id) {
            unsafe {
                let dict: Id = msg_send![noti, userInfo];
                let app: Id = msg_send![dict, objectForKey: NSWorkspaceApplicationKey];
                if app.is_null() {
                    return;
                }
                let app_url: Id = msg_send![app, bundleURL];
                if app_url.is_null() {
                    return;
                }
                let app_name: Id = cocoa_util::nsurl_filename(app_url);
                Self::check_and_apply_mute_for_app(self, app_name);
            }
        }

        #[unsafe(method(workspace_wake:))]
        fn workspace_wake(&self, _noti: Id) {
            println!("system woke up, relaunching to get a fresh event tap");
            cocoa_util::app_relaunch_self();
        }

        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            _key_path: Id,
            _object: Id,
            _change: Id,
            context: *const c_void,
        ) {
            println!("FilterListMode Changed!");
            if !context.is_null() {
                return;
            }
            unsafe {
                let defaults = NSUserDefaults::standardUserDefaults();
                let new_value: isize = msg_send![&defaults,
                    integerForKey: cocoa_util::nsstr("FilterListMode")];
                self.ivars().filter_list_mode.set(new_value);
                println!("FilterListMode Changed! {}", new_value);
            }
        }

        // ===== 菜单栏（状态栏）图标 =====

        #[unsafe(method(menu_open_settings:))]
        fn menu_open_settings(&self, _sender: Id) {
            println!("menu: open settings");
            unsafe {
                // 即使作为 agent 运行，也把设置窗口带到最前面
                let app: Id = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: true];
            }
            Self::show_settings(self);
        }

        #[unsafe(method(menu_quit:))]
        fn menu_quit(&self, _sender: Id) {
            println!("menu: quit");
            cocoa_util::app_terminate();
        }

        // ===== property accessors (some are called via msg_send from settings_ui) =====

        #[unsafe(method(tickeys))]
        fn get_tickeys(&self) -> usize {
            self.ivars().tickeys.get()
        }

        #[unsafe(method(setTickeys:))]
        fn set_tickeys(&self, val: usize) {
            self.ivars().tickeys.set(val);
        }

        #[unsafe(method(filterList))]
        fn get_filter_list(&self) -> Id {
            self.ivars().filter_list.get()
        }

        #[unsafe(method(setFilterList:))]
        fn set_filter_list(&self, val: Id) {
            self.ivars().filter_list.set(val);
        }

        #[unsafe(method(filterListMode))]
        fn get_filter_list_mode(&self) -> isize {
            self.ivars().filter_list_mode.get()
        }

        #[unsafe(method(setFilterListMode:))]
        fn set_filter_list_mode(&self, val: isize) {
            self.ivars().filter_list_mode.set(val);
        }
    }

    unsafe impl NSObjectProtocol for AppDelegate {}
);

unsafe impl NSApplicationDelegate for AppDelegate {}

impl AppDelegate {
    pub fn new() -> Retained<AppDelegate> {
        let mtm = objc2::MainThreadMarker::new().expect("AppDelegate::new on main thread");
        let partial = AppDelegate::alloc(mtm).set_ivars(AppDelegateIvars::default());
        let inst: Retained<AppDelegate> = unsafe { msg_send![super(partial), init] };

        // load filter list from user defaults
        unsafe {
            let defaults = NSUserDefaults::standardUserDefaults();
            let key = NSString::from_str("FilterList");
            let stored: Id = msg_send![&defaults, objectForKey: &*key];
            let list: Id = if stored.is_null() {
                msg_send![class!(NSMutableArray), arrayWithCapacity: 8usize]
            } else {
                msg_send![class!(NSMutableArray), arrayWithArray: stored]
            };
            inst.ivars().filter_list.set(list);

            let mode: isize = msg_send![&defaults,
                integerForKey: cocoa_util::nsstr("FilterListMode")];
            inst.ivars().filter_list_mode.set(mode);
            println!("FilterListMode = {}", mode);
        }

        inst
    }

    fn check_and_apply_mute_for_app(this: &AppDelegate, app_name: Id) {
        unsafe {
            let filter_list: Id = msg_send![this as *const AppDelegate as Id, filterList];
            let is_in_list: bool = msg_send![filter_list, containsObject: app_name];
            let filter_list_mode: isize =
                msg_send![this as *const AppDelegate as Id, filterListMode];

            let should_mute = match filter_list_mode {
                0 => is_in_list,
                1 => !is_in_list,
                _ => false,
            };

            let tickeys_ptr: usize = msg_send![this as *const AppDelegate as Id, tickeys];
            if tickeys_ptr != 0 {
                let tickeys = &mut *(tickeys_ptr as *mut Tickeys);
                tickeys.set_mute(should_mute);
            }
        }
    }

    fn install_status_item(this: &AppDelegate) {
        unsafe {
            let mtm = objc2::MainThreadMarker::new().unwrap();
            let status_bar = NSStatusBar::systemStatusBar();
            // NSVariableStatusItemLength == -1.0
            let item: Id = msg_send![&status_bar, statusItemWithLength: -1.0f64];

            let icon_path = cocoa_util::get_res_path("menubar.png");
            let icon_path_ns = NSString::from_str(&icon_path);
            let image: Retained<NSImage> = NSImage::initWithContentsOfFile(
                NSImage::alloc(),
                &icon_path_ns,
            )
            .expect("missing menubar.png resource");

            let _: () = msg_send![item, setImage: &*image];

            let button: Id = msg_send![item, button];
            if !button.is_null() {
                let _: () = msg_send![button, setToolTip: &*cocoa_util::l10n_str("menu_tooltip")];
            }

            let menu: Retained<NSMenu> = NSMenu::new(mtm);

            let open_item: Retained<NSMenuItem> = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &*cocoa_util::l10n_str("menu_open_settings"),
                Some(sel!(menu_open_settings:)),
                &NSString::from_str(""),
            );
            let target: *const objc2::runtime::AnyObject = ProtocolObject::<dyn NSApplicationDelegate>::from_ref(this) as *const _ as *const objc2::runtime::AnyObject;
            open_item.setTarget(Some(&*target));
            menu.addItem(&open_item);

            let quit_item: Retained<NSMenuItem> = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &*cocoa_util::l10n_str("menu_quit"),
                Some(sel!(menu_quit:)),
                &NSString::from_str(""),
            );
            let target: *const objc2::runtime::AnyObject = ProtocolObject::<dyn NSApplicationDelegate>::from_ref(this) as *const _ as *const objc2::runtime::AnyObject;
            quit_item.setTarget(Some(&*target));
            menu.addItem(&quit_item);

            let _: () = msg_send![item, setMenu: &*menu];

            // store a reference so the item is not reclaimed
            this.ivars().status_item.set(item);
            println!("install_status_item: done");
        }
    }

    fn show_settings(this: &AppDelegate) {
        println!("Settings!");
        let tickeys_ptr: usize =
            unsafe { msg_send![this as *const AppDelegate as Id, tickeys] };
        if tickeys_ptr == 0 {
            eprintln!("show_settings: no Tickeys instance yet");
            return;
        }
        SettingsController::get_instance(tickeys_ptr);
    }

    fn handle_keydown(tickeys: &Tickeys, _key: u8) {
        let last_keys = tickeys.get_last_keys();
        let last_keys_len = last_keys.len();

        let mut pass = false;
        for seq in OPEN_SETTINGS_KEY_SEQ {
            let seq_len = seq.len();
            if last_keys_len < seq_len {
                return;
            }

            pass = true;
            // compare from tail to head
            for i in 1..(seq_len + 1) {
                if last_keys[last_keys_len - i] != seq[seq_len - i] {
                    pass = false;
                    break;
                }
            }

            if pass {
                break;
            }
        }

        if pass {
            let app: Id = unsafe { msg_send![class!(NSApplication), sharedApplication] };
            let delegate: Id = unsafe { msg_send![app, delegate] };
            if !delegate.is_null() {
                Self::show_settings(unsafe { &*(delegate as *const AppDelegate) });
            }
        }
    }

    fn load_schemes() -> Vec<crate::tickeys::AudioScheme> {
        let path = cocoa_util::get_res_path("data/schemes.json");
        let mut file = std::fs::File::open(&path)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        let mut json_str = String::with_capacity(512);
        match file.read_to_string(&mut json_str) {
            Ok(_) => {}
            Err(e) => panic!("Failed to read json: {}", e),
        }
        serde_json::from_str(&json_str).expect("failed to parse schemes.json")
    }

    fn request_ax() {
        println!("request_ax");
        unsafe {
            // AXIsProcessTrusted(): no-options variant, used as a cross-check
            // against AXIsProcessTrustedWithOptions to tell apart "system
            // really says no" from "our options dictionary is broken".
            let plain: u8 = AXIsProcessTrusted();
            println!("request_ax: AXIsProcessTrusted() = {}", plain != 0);
            println!("request_ax: WithOptions(false) = {}", is_accessibility_trusted(false));

            if is_accessibility_trusted(false) || plain != 0 {
                println!("request_ax: trusted, continuing");
                return;
            }

            // 这里故意不调用 is_accessibility_trusted(true)（系统自己的弹窗会和
            // 我们的 NSAlert 叠在一起抢焦点）。也不放轮询占死主线程 ——
            // NSAlert 是模态的、可响应，用户照提示去系统设置勾选后回来点“继续”。
            while !is_accessibility_trusted(false) {
                let mtm = objc2::MainThreadMarker::new().unwrap();
                let alert = NSAlert::new(mtm);
                alert.setMessageText(&cocoa_util::l10n_str("ax_tip"));
                alert.addButtonWithTitle(&cocoa_util::l10n_str("quit"));
                alert.addButtonWithTitle(&cocoa_util::l10n_str("doneWithThis"));

                let btn: isize = alert.runModal();
                println!("request_ax alert: {}", btn);
                if btn == 1000 {
                    // NSAlertFirstButtonReturn
                    cocoa_util::app_terminate();
                    return;
                }
            }

            // macOS only applies a freshly granted Accessibility permission to
            // a newly launched process, so restart ourselves now.
            println!("request_ax: accessibility granted, relaunching");
            cocoa_util::app_relaunch_self();
        }
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

unsafe fn is_accessibility_trusted(prompt: bool) -> bool {
    use core_foundation_sys::base::{kCFAllocatorDefault, CFRelease};
    use core_foundation_sys::number::{kCFBooleanFalse, kCFBooleanTrue};
    use core_foundation_sys::dictionary::{CFDictionaryCreate, CFDictionaryRef, kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks};
    use core_foundation_sys::string::{CFStringCreateWithCString, kCFStringEncodingUTF8};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    }

    let key = CFStringCreateWithCString(
        kCFAllocatorDefault,
        b"AXTrustedCheckOptionPrompt\0".as_ptr() as *const std::os::raw::c_char,
        kCFStringEncodingUTF8,
    );
    let value = if prompt { kCFBooleanTrue } else { kCFBooleanFalse };
    let dict = CFDictionaryCreate(
        kCFAllocatorDefault,
        &key as *const _ as *const *const c_void,
        &value as *const _ as *const *const c_void,
        1,
        &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks,
    );

    let trusted = AXIsProcessTrustedWithOptions(dict) != 0;

    CFRelease(key as *const c_void);
    CFRelease(dict as *const c_void);

    trusted
}

fn main() {
    // Everything before NSApplication::run() needs a pool: our helpers
    // return autoreleased objects (nsstr), and autorelease without any pool
    // on the thread crashes inside the next ObjC call.
    let delegate = objc2::rc::autoreleasepool(|_| AppDelegate::new());
    cocoa_util::app_run(&*delegate);
}
