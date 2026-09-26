//! Settings window controller.
//!
//! The UI itself still lives in the 2015 `Settings.nib` (Base.lproj /
//! zh-Hans.lproj). The NIB wires outlets/actions to a class named
//! `SettingsController` by *name*, so every selector below must keep its
//! original Objective-C spelling or the connections silently break.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use objc2::define_class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::MainThreadOnly;
use objc2::runtime::NSObjectProtocol;
use objc2::DefinedClass;
use objc2_app_kit::NSWindowController;
use objc2_foundation::NSString;

use crate::cocoa_util::{self, Id, NIL};
use crate::consts::{CURRENT_VERSION, DONATE_URL, WEBSITE};
use crate::pref::Pref;
use crate::tickeys::Tickeys;

// naive way of making this a singleton
static SHOWING_GUI: AtomicBool = AtomicBool::new(false);
static SETTINGS_INSTANCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

#[derive(Default)]
pub struct SettingsIvars {
    user_data: Cell<usize>,
    popup_audio_scheme: Cell<Id>,
    slide_volume: Cell<Id>,
    slide_pitch: Cell<Id>,
    label_version: Cell<Id>,
    filter_list_table: Cell<Id>,
}

define_class!(
    #[unsafe(super(NSWindowController))]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "SettingsController"]
    #[ivars = SettingsIvars]
    pub struct SettingsController;

    impl SettingsController {
        #[unsafe(method(windowDidLoad))]
        fn window_did_load(&self) {
            println!("windowDidLoad");
            unsafe {
                let window: Id = msg_send![self as *const Self as Id, window];

                // hide window btns
                let btn_min: Id = msg_send![window, standardWindowButton: 1i64];
                let _: () = msg_send![btn_min, setHidden: true];
                let btn_zoom: Id = msg_send![window, standardWindowButton: 2i64];
                let _: () = msg_send![btn_zoom, setHidden: true];

                // kCGFloatingWindowLevelKey == 5
                let level: i64 = CGWindowLevelForKey(5) as i64;
                let _: () = msg_send![window, setLevel: level];

                Self::load_values(self);
            }
        }

        #[unsafe(method(quit:))]
        fn quit_(&self, _sender: Id) {
            println!("Quit");
            cocoa_util::app_terminate();
        }

        #[unsafe(method(follow_link:))]
        fn follow_link_(&self, sender: Id) {
            unsafe {
                let tag: isize = msg_send![sender, tag];
                let url = match tag {
                    0 => WEBSITE,
                    1 => DONATE_URL,
                    _ => return,
                };

                let workspace: Id = msg_send![objc2::class!(NSWorkspace), sharedWorkspace];
                let url: Id = msg_send![objc2::class!(NSURL), URLWithString: cocoa_util::nsstr(url)];
                let _: bool = msg_send![workspace, openURL: url];
            }
        }

        #[unsafe(method(value_changed:))]
        fn value_changed_(&self, sender: Id) {
            println!("SettingsController::value_changed_");

            const TAG_POPUP_SCHEME: isize = 0;
            const TAG_SLIDE_VOLUME: isize = 1;
            const TAG_SLIDE_PITCH: isize = 2;

            unsafe {
                let user_defaults: Id = msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
                let tickeys_ptr: usize = self.ivars().user_data.get();
                if tickeys_ptr == 0 {
                    return;
                }
                let tickeys = &mut *(tickeys_ptr as *mut Tickeys);
                let tag: isize = msg_send![sender, tag];

                match tag {
                    TAG_POPUP_SCHEME => {
                        let value: isize = msg_send![sender, indexOfSelectedItem];

                        let schemes = tickeys.get_schemes();
                        if value < 0 || value as usize >= schemes.len() {
                            return;
                        }
                        let sch = schemes[value as usize].name.clone();

                        let scheme_dir = "data/".to_string() + &sch;
                        tickeys.load_scheme(&cocoa_util::get_res_path(&scheme_dir), &sch);

                        let _: () = msg_send![user_defaults,
                            setObject: cocoa_util::nsstr(&sch),
                            forKey: cocoa_util::nsstr("audio_scheme")];
                    },

                    TAG_SLIDE_VOLUME => {
                        let value: f32 = msg_send![sender, floatValue];
                        tickeys.set_volume(value);

                        let _: () = msg_send![user_defaults,
                            setFloat: value,
                            forKey: cocoa_util::nsstr("volume")];
                    },

                    TAG_SLIDE_PITCH => {
                        let mut value: f32 = msg_send![sender, floatValue];
                        if value > 1.0f32 {
                            // map slider [0, 1.5] -> pitch [0, 2]
                            value = value * (2.0f32 / 1.5f32);
                        }
                        tickeys.set_pitch(value);

                        let _: () = msg_send![user_defaults,
                            setFloat: value,
                            forKey: cocoa_util::nsstr("pitch")];
                    },

                    _ => {}
                }
            }
        }

        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _note: Id) {
            println!("SettingsController::windowWillClose");
            SHOWING_GUI.store(false, Ordering::SeqCst);

            unsafe {
                let user_defaults: Id = msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
                let _: bool = msg_send![user_defaults, synchronize];
            }

            // Release our own strong reference (taken at creation). The object
            // is deallocated as this call returns -- same pattern as the old
            // `msg_send![this, release]` in windowWillClose.
            let ptr = SETTINGS_INSTANCE.swap(std::ptr::null_mut(), Ordering::SeqCst);
            if !ptr.is_null() {
                drop(unsafe { Retained::from_raw(ptr as *mut SettingsController) });
            }
        }

        #[unsafe(method(tableView:shouldEditTableColumn:row:))]
        fn table_should_edit(&self, _tv: Id, _col: Id, _row: isize) -> bool {
            false
        }

        #[unsafe(method(btnAddClicked:))]
        fn btn_add_clicked(&self, _sender: Id) {
            println!("Add Item");
            unsafe {
                let open: Id = msg_send![objc2::class!(NSOpenPanel), openPanel];
                let apps_dir: Id = msg_send![objc2::class!(NSURL), URLWithString: cocoa_util::nsstr("/Applications")];
                let allowed_types: Id = msg_send![objc2::class!(NSMutableArray), arrayWithCapacity: 2usize];
                let _: () = msg_send![allowed_types, addObject: cocoa_util::nsstr("app")];

                let _: () = msg_send![open, setDirectoryURL: apps_dir];
                let _: () = msg_send![open, setAllowedFileTypes: allowed_types];
                let _: () = msg_send![open, setAllowsMultipleSelection: true];

                let ret: isize = msg_send![open, runModal];
                if ret == 1 {
                    let files: Id = msg_send![open, URLs];
                    let n: usize = msg_send![files, count];

                    let filter_list = Self::filter_list();
                    for i in 0..n {
                        let app_name: Id =
                            cocoa_util::nsurl_filename(msg_send![files, objectAtIndex: i]);

                        let contains: bool = msg_send![filter_list, containsObject: app_name];
                        if !contains {
                            let _: () = msg_send![filter_list, addObject: app_name];
                        }
                    }

                    let table: Id = self.ivars().filter_list_table.get();
                    let _: () = msg_send![table, reloadData];

                    let ud: Id = msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
                    let _: () = msg_send![ud, setObject: filter_list, forKey: cocoa_util::nsstr("FilterList")];
                }
            }
        }

        #[unsafe(method(btnRemoveClicked:))]
        fn btn_remove_clicked(&self, _sender: Id) {
            println!("Remove Item");
            unsafe {
                let filter_list = Self::filter_list();
                let table: Id = self.ivars().filter_list_table.get();

                let selected_row: isize = msg_send![table, selectedRow];

                if selected_row >= 0 {
                    let _: () = msg_send![filter_list, removeObjectAtIndex: selected_row];
                    let _: () = msg_send![table, reloadData];

                    let ud: Id = msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
                    let _: () = msg_send![ud, setObject: filter_list, forKey: cocoa_util::nsstr("FilterList")];
                }
            }
        }

        // ===== NIB outlet setters =====
        // The 2015 Settings.nib connects outlets to File's Owner via KVC
        // (setValue:forKey:), which dispatches to these setters. Without them
        // the outlets stay nil: the popup keeps the NIB's default
        // "Item 1/2/3" rows and the sliders/labels are dead.

        #[unsafe(method(setPopup_audio_scheme:))]
        fn set_popup_audio_scheme(&self, v: Id) {
            self.ivars().popup_audio_scheme.set(v);
        }

        #[unsafe(method(setSlide_volume:))]
        fn set_slide_volume(&self, v: Id) {
            self.ivars().slide_volume.set(v);
        }

        #[unsafe(method(setSlide_pitch:))]
        fn set_slide_pitch(&self, v: Id) {
            self.ivars().slide_pitch.set(v);
        }

        #[unsafe(method(setLabel_version:))]
        fn set_label_version(&self, v: Id) {
            self.ivars().label_version.set(v);
        }

        #[unsafe(method(setFilterListTable:))]
        fn set_filter_list_table(&self, v: Id) {
            self.ivars().filter_list_table.set(v);
        }

        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table_view: Id) -> isize {
            unsafe {
                let list = Self::filter_list();
                let n: isize = msg_send![list, count];
                n
            }
        }

        #[unsafe(method(tableView:objectValueForTableColumn:row:))]
        fn table_object_value(&self, _table_view: Id, _col: Id, row: isize) -> Id {
            unsafe {
                let list = Self::filter_list();
                msg_send![list, objectAtIndex: row]
            }
        }
    }

    unsafe impl NSObjectProtocol for SettingsController {}
);

impl SettingsController {
    /// Create (or reuse) the settings window and show it.
    /// `ptr_to_tickeys` is the raw `*mut Tickeys` owned by the app delegate.
    pub fn get_instance(ptr_to_tickeys: usize) {
        if SHOWING_GUI.load(Ordering::SeqCst) {
            // already open; just bring it to front
            unsafe {
                let ptr = SETTINGS_INSTANCE.load(Ordering::SeqCst);
                if !ptr.is_null() {
                    let inst = ptr as Id;
                    let window: Id = msg_send![inst, window];
                    let _: () = msg_send![window, makeKeyAndOrderFront: NIL];
                }
            }
            return;
        }

        unsafe {
            let mtm = objc2::MainThreadMarker::new().expect("main thread");
            let nib_name = NSString::from_str("Settings");
            let partial: objc2::rc::PartialInit<SettingsController> =
                SettingsController::alloc(mtm).set_ivars(SettingsIvars {
                    user_data: Cell::new(ptr_to_tickeys),
                    ..Default::default()
                });
            let initialized: Option<Retained<SettingsController>> =
                msg_send![super(partial), initWithWindowNibName: &*nib_name];
            let Some(inst) = initialized else {
                eprintln!("SettingsController: failed to load Settings.nib");
                return;
            };

            let _: () = msg_send![&inst, showWindow: NIL];

            let raw_for_store = Retained::into_raw(inst);
            SETTINGS_INSTANCE.store(raw_for_store as *mut c_void, Ordering::SeqCst);
            SHOWING_GUI.store(true, Ordering::SeqCst);
        }
    }

    unsafe fn filter_list() -> Id {
        // strong coupling: the list lives on the app delegate, like upstream
        let app: Id = msg_send![objc2::class!(NSApplication), sharedApplication];
        let delegate: Id = msg_send![app, delegate];
        msg_send![delegate, filterList]
    }

    unsafe fn load_values(this: &SettingsController) {
        println!("loadValues");
        let tickeys_ptr: usize = this.ivars().user_data.get();
        if tickeys_ptr == 0 {
            return;
        }
        let tickeys = &mut *(tickeys_ptr as *mut Tickeys);

        let popup_audio_scheme: Id = this.ivars().popup_audio_scheme.get();
        let _: () = msg_send![popup_audio_scheme, removeAllItems];

        let schemes = tickeys.get_schemes();
        let pref = Pref::load(schemes);

        for (i, s) in schemes.iter().enumerate() {
            let _: () = msg_send![popup_audio_scheme,
                addItemWithTitle: &*cocoa_util::l10n_str(&s.display_name)];
            if s.name == pref.scheme {
                let _: () = msg_send![popup_audio_scheme, selectItemAtIndex: i as isize];
            }
        }

        let slide_volume: Id = this.ivars().slide_volume.get();
        let _: () = msg_send![slide_volume, setFloatValue: pref.volume];

        let slide_pitch: Id = this.ivars().slide_pitch.get();
        let value = if pref.pitch > 1.0f32 {
            pref.pitch * (1.5f32 / 2.0f32)
        } else {
            pref.pitch
        };
        let _: () = msg_send![slide_pitch, setFloatValue: value];

        let label_version: Id = this.ivars().label_version.get();
        let _: () = msg_send![label_version,
            setStringValue: cocoa_util::nsstr(CURRENT_VERSION)];

        println!("makeKeyAndOrderFront:");
        let window: Id = msg_send![this as *const SettingsController as Id, window];
        let _: () = msg_send![window, makeKeyAndOrderFront: NIL];

        let app: Id = msg_send![objc2::class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowLevelForKey(key: i32) -> i32;
}
