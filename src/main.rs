
extern crate libc;
extern crate openal;
extern crate cocoa;
extern crate time;
extern crate hyper;
extern crate block;
extern crate rustc_serialize;
#[macro_use] extern crate objc;
extern crate IOKit_sys as iokit;

use std::sync::{ONCE_INIT, Once};
use std::thread;
use std::io::Read;
use std::string::String;
use std::fs::File;
use libc::{c_void};
use core_foundation::*;
use objc::*;
use objc::runtime::*;
use cocoa::base::{class,id,nil};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use cocoa::appkit::{NSApp,NSApplication};
use hyper::Client;
use hyper::header::{Connection};
use hyper::status::StatusCode;
use self::block::{ConcreteBlock};
use rustc_serialize::json;

mod core_graphics;
mod core_foundation;
mod alut;
mod event_tap;
mod tickeys;
#[macro_use] mod cocoa_util;
mod consts;
mod settings_ui;
mod pref;

use tickeys::*;
use cocoa_util::*;
use settings_ui::*;
use consts::*;
use pref::*;

extern 
{ 
	static NSWorkspaceDidActivateApplicationNotification: id;
	static NSWorkspaceApplicationKey: id;
	static NSKeyValueChangeNewKey: id;

}

static mut RUNNING: bool = false; 


fn main()
{
	unsafe { NSAutoreleasePool::new(nil); }

	monitor_os_power_event();
	
	let appDelegate = <id as AppDelegate>::new();
	app_run(appDelegate);
}

fn monitor_os_power_event()
{
	println!("monitor_os_power_event()");
	#[allow(unused_variables)]
 	extern fn power_callback(ref_con: *mut c_void, service: iokit::io_service_t,
		msg: u32, msg_args: *mut c_void)
	{
		println!("System Power Callback! ");
		match msg
		{
			iokit::kIOMessageSystemHasPoweredOn =>
			{
				println!("System PoweredOn");
				app_relaunch_self(); //just relaunch;
			},
			_ => {}
		}
	}

	unsafe
	{
		// notification port allocated by IORegisterForSystemPower
	    let mut notify_port_ref: iokit::IONotificationPortRef = std::ptr::null_mut();
	    // notifier object, used to deregister later
	    let mut notifier_object: iokit::io_object_t = 0;
	    // this parameter is passed to the callback
	    let ref_con: *mut c_void = std::ptr::null_mut();
	    // register to receive system sleep notifications
	    let root_port = iokit::IORegisterForSystemPower( ref_con, &mut notify_port_ref as *mut _,
			power_callback, &mut notifier_object as *mut _);

	    if root_port == 0
	    {
	        println!("IORegisterForSystemPower failed\n");
	        return; //ignore for now
	    }
	    // add the notification port to the application runloop
	    core_foundation::CFRunLoopAddSource( core_foundation::CFRunLoopGetCurrent(),
	    	iokit::IONotificationPortGetRunLoopSource(notify_port_ref) as CFRunLoopSourceRef,
	    	core_foundation::kCFRunLoopCommonModes );
	}
}

#[repr(i32)]
enum FilterListMode 
{
	BlackList = 0,
	WhiteList = 1
}

trait AppDelegate // <NSApplicationDelegate>
{
	fn new() -> id
	{
		static REG_OBJC_CLS: Once = ONCE_INIT;
		REG_OBJC_CLS.call_once(||
		{
			let nsobjcet = objc::runtime::Class::get("NSObject").unwrap();
			let mut decl = objc::declare::ClassDecl::new(nsobjcet, stringify!(AppDelegate)).unwrap();

			unsafe
			{
				decl.add_method(sel!(applicationDidFinishLaunching:), Self::applicationDidFinishLaunching as extern fn(&mut Object, Sel, id));
				decl.add_method(sel!(applicationDidBecomeActive:), Self::applicationDidBecomeActive as extern fn(&mut Object, Sel, id));
				decl.add_method(sel!(applicationWillTerminate:), Self::applicationWillTerminate as extern fn(&mut Object, Sel, id));
				decl.add_method(sel!(userNotificationCenter:didActivateNotification:), Self::userNotificationCenterDidActivateNotification as extern fn(&mut Object, Sel, id, id));
				decl.add_method(sel!(workspace_app_activated:), Self::workspace_app_activated as extern fn(&mut Object, Sel, id));

				decl.add_method(sel!(observeValueForKeyPath:ofObject:change:context:), 
						Self::observeValueForKeyPathOfObjectChangeContext as extern fn(&mut Object, Sel, id, id, id, *const c_void));

				decl_prop!(decl, usize, tickeys);
				decl_prop!(decl, id, filterList);
				decl_prop!(decl, i32, filterListMode);
				decl_prop!(decl, id, statusItem);

				decl.add_method(sel!(menu_open_settings:), Self::menu_open_settings as extern fn(&mut Object, Sel, id));
				decl.add_method(sel!(menu_quit:), Self::menu_quit as extern fn(&mut Object, Sel, id));
			}

			decl.register();
		});

	    unsafe 
	    { 
	    	let inst: id = msg_send![class(stringify!(AppDelegate)), new];

	    	let userDefaults: id = msg_send![class("NSUserDefaults"), standardUserDefaults];

	    	let mut filterList: id = msg_send![userDefaults, objectForKey: nsstr("FilterList")];
	    	if filterList == nil 
	    	{
	    		filterList = msg_send![class("NSMutableArray"), arrayWithCapacity: 8];
	    	}else 
	    	{
	    		filterList = msg_send![class("NSMutableArray"), arrayWithArray: filterList];
	    	}
	    	let _: id = msg_send![inst, setFilterList: filterList];


	    	//get filter mode
	    	let filterListMode: i32 = msg_send![userDefaults, integerForKey: nsstr("FilterListMode")];
	    	let _: id = msg_send![inst, setFilterListMode: filterListMode];
	    	println!("FilterListMode = {:}", filterListMode);

	    	inst
	    }
	}

	extern fn applicationDidFinishLaunching(this: &mut Object, _cmd: Sel, note: id)
	{
		// 菜单栏图标第一个装上：这样即便后面卡在辅助功能授权弹窗上，
		// 用户也能看到程序确实起来了、并且能从菜单里退出。
		Self::install_status_item(this);

		Self::request_ax();
		Self::begin_check_update(this, &nsstring_to_string(l10n_str("check_update_url")));

		let sch = Self::load_schemes();
		let pref = Pref::load(&sch);
		let mut tickeys = Box::new(Tickeys::new(sch));
		tickeys.load_scheme(&get_res_path(&format!("data/{:}", &pref.scheme)), &pref.scheme);
		tickeys.set_volume(pref.volume);
		tickeys.set_pitch(pref.pitch);
		tickeys.set_on_keydown(Some(Self::handle_keydown)); //handle qaz123
		tickeys.start();
		
		unsafe
		{
			let _: id = msg_send![this, setTickeys: tickeys]; //moved

			let noti_center:id = msg_send![class("NSUserNotificationCenter"), defaultUserNotificationCenter];
			let _:id = msg_send![noti_center, setDelegate: this as *mut Object];
		}

		Self::show_noti(l10n_str("Tickeys_Running"), l10n_str("press_qaz123"));

		unsafe
		{
			//observe NSWorkspaceDidActivateApplicationNotification
			let workspace: id = msg_send![class("NSWorkspace"), sharedWorkspace];
			let notiCenter: id = msg_send![workspace, notificationCenter];

			let _: id = msg_send![notiCenter, addObserver:this as *mut Object
										    	selector:sel!(workspace_app_activated:) 
											  	    name:NSWorkspaceDidActivateApplicationNotification 
												  object:nil];

			//observe FilterListMode 
			let ud: id = msg_send![class("NSUserDefaultsController"), sharedUserDefaultsController];
			let _: id = msg_send![ud, addObserver:this as *mut Object
                                       forKeyPath:nsstr("values.FilterListMode")
                                          options:1 /*NSKeyValueObservingOptionNew*/
                                          context:0];

             
            //get current active app 
	    	let workspace: id = msg_send![class("NSWorkspace"), sharedWorkspace];
	    	let frontApp: id = msg_send![workspace, frontmostApplication];
			Self::check_and_apply_mute_for_app(this, nsurl_filename(msg_send![frontApp, bundleURL]));

		}

	}

	extern fn applicationDidBecomeActive(this: &mut Object, _cmd: Sel, note: id)
	{
		// 这里**故意不自动弹设置窗口**。
		//
		// 上游的实现在每次 app 变成活跃时就 show_settings，因为它是纯后台 agent，
		// 几乎不会被动激活。但本构建有菜单栏图标、还会在授权后自我重启，自动弹窗
		// 会反复把焦点从用户正在用的程序（比如浏览器输入框）抢走。
		// 设置窗口改成只由用户主动打开：菜单栏图标里的"打开设置"，或按 QAZ123。
		let _ = (this, note);
		println!("applicationDidBecomeActive (不自动开设置窗口)");
	}

	extern fn applicationWillTerminate(this: &mut Object, _cmd: Sel, _note: id)
	{
		// Let the Tickeys instance drop -- but only if one was actually created.
		//
		// `setTickeys:` happens near the end of applicationDidFinishLaunching, whereas
		// request_ax() runs at the very beginning and its "quit" button calls
		// [NSApp terminate:]. Terminating from there lands us here while the ivar is
		// still NULL; rebuilding a Box from NULL and dropping it segfaults
		// (EXC_BAD_ACCESS at 0x18 inside drop_in_place).
		let ptr: usize = unsafe { msg_send![this, tickeys] };
		if ptr == 0
		{
			println!("applicationWillTerminate: no Tickeys instance yet, nothing to drop");
			return;
		}
		unsafe { drop(Box::from_raw(ptr as *mut Tickeys)); }
	}

	extern fn userNotificationCenterDidActivateNotification(this: &mut Object, _cmd: Sel, center: id, note: id)
	{
		println!("userNotificationCenterDidActivateNotification");

		unsafe
		{
			let workspace: id = msg_send![class("NSWorkspace"), sharedWorkspace];
			let url:id = msg_send![class("NSURL"), URLWithString: NSString::alloc(nil).init_str(WEBSITE)];
			let _:bool = msg_send![workspace, openURL: url];

			msg_send![center, removeDeliveredNotification:note]
		}
	}

	extern fn workspace_app_activated(this: &mut Object, cmd: Sel, noti: id)
	{
		unsafe 
		{
			let dict: id = msg_send![noti, userInfo];
			let app: id = msg_send![dict, objectForKey: NSWorkspaceApplicationKey];

			let app_url: id = msg_send![app, bundleURL];
			let path_components: id = msg_send![app_url, pathComponents];
			
			let app_name: id = msg_send![path_components, lastObject];

			Self::check_and_apply_mute_for_app(this, app_name);
		}
	}

	fn check_and_apply_mute_for_app(this: &Object, app_name: id)
	{
		unsafe
		{
			let filterList: id = msg_send![this, filterList];

			////=========
			let isInList: bool = msg_send![filterList, containsObject: app_name];
			let filterListMode: i32 = msg_send![this, filterListMode];

			println!("filterlistmode = {:?}", filterListMode);
			let shouldMute = match filterListMode
			{
				0 => isInList,
				1 => !isInList,
				_ => false,
			};

			let mut tickeys: Box<Tickeys> = msg_send![this, tickeys];
			tickeys.set_mute(shouldMute);
			std::mem::forget(tickeys);

			//println!("workspace_app_activated: {:}, shouldMute: {:}", nsstring_to_string(app_name), shouldMute);
		}
			
	}

	//- (void)observeValueForKeyPath:(NSString *)keyPath ofObject:(id)object change:(NSDictionary *)change context:(void *)context
	extern fn observeValueForKeyPathOfObjectChangeContext(this: &mut Object, cmd: Sel, keypath: id, object: id, change: id, context: *const c_void)
	{
		println!("FilterListMode Changed!");

		unsafe
		{
			if context == (0 as *const c_void)
			{
				let ud: id = msg_send![class("NSUserDefaults"), standardUserDefaults];
				let newValue: i32 = msg_send![ud, integerForKey: nsstr("FilterListMode")];

				let _: id = msg_send![this, setFilterListMode: newValue];

				println!("FilterListMode Changed! {:}", newValue);

			}else 
			{
				//... super call ?
			}
		}

	}

	// ===== 菜单栏（状态栏）图标 =====
	//
	// 上游 0.5.0 是个没有任何可见界面的后台程序：只能靠弹通知和按 QAZ123 打开设置，
	// 用户根本判断不出它有没有在运行。这里补上一个菜单栏图标，行为对齐 1.1.0。

	extern fn menu_open_settings(this: &mut Object, _cmd: Sel, _sender: id)
	{
		println!("menu: open settings");
		unsafe
		{
			// 即使作为 agent 运行，也把设置窗口带到最前面
			let _: id = msg_send![NSApp(), activateIgnoringOtherApps: 1i8];
		}
		Self::show_settings(this);
	}

	extern fn menu_quit(_this: &mut Object, _cmd: Sel, _sender: id)
	{
		println!("menu: quit");
		app_terminate();
	}

	fn install_status_item(this: &mut Object)
	{
		unsafe
		{
			let status_bar: id = msg_send![class("NSStatusBar"), systemStatusBar];
			// NSVariableStatusItemLength == -1.0
			let item: id = msg_send![status_bar, statusItemWithLength: -1.0f64];

			let icon_path = get_res_path("menubar.png");
			let image: id = msg_send![class("NSImage"), alloc];
			let image: id = msg_send![image, initWithContentsOfFile: nsstr(&icon_path)];
			if image != nil
			{
				let _: id = msg_send![item, setImage: image];
			}

			let button: id = msg_send![item, button];
			if button != nil
			{
				let _: id = msg_send![button, setToolTip: l10n_str("menu_tooltip")];
			}

			let menu: id = msg_send![class("NSMenu"), new];

			let open_item: id = msg_send![class("NSMenuItem"), alloc];
			let open_item: id = msg_send![open_item,
				initWithTitle: l10n_str("menu_open_settings")
				action: sel!(menu_open_settings:)
				keyEquivalent: nsstr("")];
			let _: id = msg_send![open_item, setTarget: this as *mut Object];
			let _: id = msg_send![menu, addItem: open_item];

			let quit_item: id = msg_send![class("NSMenuItem"), alloc];
			let quit_item: id = msg_send![quit_item,
				initWithTitle: l10n_str("menu_quit")
				action: sel!(menu_quit:)
				keyEquivalent: nsstr("")];
			let _: id = msg_send![quit_item, setTarget: this as *mut Object];
			let _: id = msg_send![menu, addItem: quit_item];

			let _: id = msg_send![item, setMenu: menu];

			// 存一份引用，免得被回收
			let _: id = msg_send![this, setStatusItem: item];
			println!("install_status_item: done");
		}
	}

	fn show_noti(title: id, msg: id)
	{
		unsafe
		{
			let note:id = NSUserNotification::new(nil).autorelease();
			note.setTitle(title);
			note.setInformativeText(msg);

			let center:id = msg_send![class("NSUserNotificationCenter"), defaultUserNotificationCenter];

			msg_send![center, deliverNotification: note]
		}
	}

	fn handle_keydown(tickeys: &Tickeys, key: u8)
	{
		let last_keys = tickeys.get_last_keys();
		let last_keys_len = last_keys.len();

		let mut pass = false;
		for seq in OPEN_SETTINGS_KEY_SEQ
		{
			let seq_len = seq.len();
			if last_keys_len < seq_len {return;}

			pass = true;
			//cmp from tail to head
			for i in 1..(seq_len+1)
			{
				if last_keys[last_keys_len - i] != seq[seq_len - i]
				{
					pass = false;
					break;
				}
			}

			if pass { break;}
		}

		if pass
		{
			Self::show_settings( unsafe{ msg_send![NSApp(), delegate] } );
		}
	}

	fn begin_check_update(this: &mut Object, url: &str)
	{
		#[derive(RustcDecodable, RustcEncodable)]
		#[allow(non_snake_case)]
		struct Version
		{
			Version: String,
			WhatsNew: String,
		}

		let run_loop_ref = unsafe { CFRunLoopGetCurrent() as usize };
		let check_update_url = url.to_string();
		let ptr_to_this: usize = unsafe { std::mem::transmute(this) };
		thread::spawn(move ||
		{
			thread::sleep_ms(1000 * 30); //do it xx seconds later.
			println!("begin_check_update do_job!");
			match do_job(ptr_to_this, check_update_url, run_loop_ref)
			{
				Ok(()) => println!("begin_check_update(): Ok"),
				Err(e) => println!("begin_check_update() Error: {:}", e)
			}
		});

		fn do_job(this: usize, check_update_url: String, run_loop_ref: usize) -> Result<(), hyper::Error>
		{
			let client = Client::new();
		    let mut resp = try!{ client.get(&check_update_url).header(Connection::close()).send() };
		    if resp.status == StatusCode::Ok
		    {
		    	let mut content = String::new();
				try!{ resp.read_to_string(&mut content) };
		    	println!("Response: {}", content);

		    	if content.contains("Version")
		    	{
		    		let ver:Version = json::decode(&content).unwrap();
		    		println!("ver={}",ver.Version);
		    		if ver.Version != CURRENT_VERSION
		    		{
		    			let cblock : ConcreteBlock<(),(),_> = ConcreteBlock::new(move ||
				    	{
				    		let this_ptr: &mut Object = unsafe{ std::mem::transmute(this) };
				    		<id as AppDelegate>::handle_update_info(this_ptr, ver.Version.clone(), ver.WhatsNew.clone());
				    	});

				    	let block = & *cblock.copy();
				    	unsafe { CFRunLoopPerformBlock(run_loop_ref as *mut c_void, kCFRunLoopDefaultMode, block); }
			    	}
		    	}
				return Ok(());
		    }else
		    {
		    	println!("Failed to check for update: Status {}", resp.status);
				return Err(hyper::Error::Status);
		    }
		}
	}

	fn handle_update_info(this: &mut Object, ver: String, whatsNew: String)
	{
	    println!("New Version Available!");
		let title = l10n_str("newVersion");
		let whats_new = unsafe
		{
			NSString::alloc(nil).init_str(
				&format!("{} -> {}: {}",CURRENT_VERSION, ver, whatsNew)
			).autorelease()
		};
		Self::show_noti(title, whats_new);
	}

	fn request_ax()
	{
		println!("request_ax");
		#[link(name = "ApplicationServices", kind = "framework")]
		extern "system"
		{
		 	fn AXIsProcessTrustedWithOptions (options: id) -> bool;
		}

	 	unsafe fn is_enabled(prompt: bool) -> bool
	 	{
			let dict: id = msg_send![class("NSDictionary"),
				dictionaryWithObject: (if prompt {kCFBooleanTrue}else{kCFBooleanFalse})
				forKey: kAXTrustedCheckOptionPrompt];

			return AXIsProcessTrustedWithOptions(dict);
		}

		unsafe
		{
			if is_enabled(false) 
			{ 
				RUNNING = true;
				return; 
			}

			// 这里**故意不调用** is_enabled(true)。
			//
			// AXIsProcessTrustedWithOptions(prompt: true) 会弹出系统自己的授权询问框，
			// 而紧接着我们又弹出下面这个 NSAlert —— 两个模态框叠在一起抢焦点，
			// 用户点哪个都"没反应"（实测截图确认过）。只留一个框，用户照着文字去
			// 系统设置里勾选即可，勾完回来点"继续"。
			//
			// 也不要在这里放 sleep/轮询：那会占死主线程，整个 app 变成一块石头，
			// 连 Dock 和 Cmd-Q 都按不动（那是上一版的错误尝试）。

			// Report where to click, then wait for the user -- with a modal alert, not
			// with a busy loop.
			//
			// A previous attempt polled `loop { if is_enabled(false) { break }
			// thread::sleep_ms(500) }` after this alert. That blocks the main thread,
			// which kills the app's event loop: the process stays alive but the UI is
			// frozen and it cannot even be quit. The alert below is modal, so it is the
			// app's own (responsive) UI and it does NOT block System Settings -- the
			// user can grant the permission and then come back and click "继续".
			//
			// Do not put a sleep/poll loop on the main thread here.
			while !is_enabled(false)
			{
				let alert:id = msg_send![class("NSAlert"), new];
				alert.autorelease();
				let _:id = msg_send![alert, setMessageText: l10n_str("ax_tip")];
				let _:id = msg_send![alert, addButtonWithTitle: l10n_str("quit")];
				let _:id = msg_send![alert, addButtonWithTitle: l10n_str("doneWithThis")];

				let btn:i32 = msg_send![alert, runModal];
				println!("request_ax alert: {}", btn);
				if btn == 1000
				{
					app_terminate();
					return;
				}
			}

			// macOS only applies a freshly granted Accessibility permission to a newly
			// launched process, so restart ourselves now that it is in.
			println!("request_ax: granted, relaunching");
			println!("request_ax: accessibility granted, relaunching");

			app_relaunch_self();
		}
	}

	fn show_settings(this: &mut Object)
	{
		println!("Settings!");
		unsafe
		{
			let tickeys: Box<Tickeys> =  msg_send![this, tickeys];
			SettingsController::get_instance(nil, std::mem::transmute(tickeys));
		}
	}

	fn load_schemes() -> Vec<AudioScheme>
	{
		let path = get_res_path("data/schemes.json");
		let mut file = File::open(path).unwrap();

		let mut json_str = String::with_capacity(512);
		match file.read_to_string(&mut json_str)
		{
			Ok(_) => {},
			Err(e) => panic!("Failed to read json:{}",e)
		}
		json::decode(&json_str).unwrap()
	}

}

impl AppDelegate for id
{}
