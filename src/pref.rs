//! User preferences stored in NSUserDefaults.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSUserDefaults};

use crate::cocoa_util;
use crate::tickeys::AudioScheme;

pub struct Pref {
    pub scheme: String,
    pub volume: f32,
    pub pitch: f32,
}

impl Pref {
    pub fn load(schemes: &[AudioScheme]) -> Pref {
        let defaults = NSUserDefaults::standardUserDefaults();
        let default = Pref {
            scheme: schemes[0].name.clone(),
            volume: 0.5,
            pitch: 1.0,
        };

        let pref_exists: Option<Retained<NSString>> = unsafe {
            let key = cocoa_util::nsstr("pref_exists");
            let r: Option<Retained<NSString>> = msg_send![&defaults, stringForKey: &*key];
            r
        };
        if pref_exists.is_none() {
            // first run
            let p = default.clone_pref();
            p.save();
            return p;
        }
        let audio_scheme: Option<Retained<NSString>> = unsafe {
            let key = cocoa_util::nsstr("audio_scheme");
            let r: Option<Retained<NSString>> = msg_send![&defaults, stringForKey: &*key];
            r
        };
        let volume: f32 = unsafe {
            let key = cocoa_util::nsstr("volume");
            let r: f32 = msg_send![&defaults, floatForKey: &*key];
            r
        };
        let pitch: f32 = unsafe {
            let key = cocoa_util::nsstr("pitch");
            let r: f32 = msg_send![&defaults, floatForKey: &*key];
            r
        };

        let mut scheme_str = audio_scheme
            .map(|s| s.to_string())
            .unwrap_or_else(|| default.scheme.clone());

        // validate scheme
        if !schemes.iter().any(|s| s.name == scheme_str) {
            scheme_str = default.scheme;
        }
        Pref {
            scheme: scheme_str,
            volume,
            pitch,
        }
    }

    fn clone_pref(&self) -> Pref {
        Pref {
            scheme: self.scheme.clone(),
            volume: self.volume,
            pitch: self.pitch,
        }
    }

    pub fn save(&self) {
        let defaults = NSUserDefaults::standardUserDefaults();
        unsafe {
            let scheme = cocoa_util::nsstr(&self.scheme);
            let audio_scheme_key = cocoa_util::nsstr("audio_scheme");
            let _: () = msg_send![&defaults, setObject: &*scheme, forKey: &*audio_scheme_key];

            let volume_key = cocoa_util::nsstr("volume");
            let _: () = msg_send![&defaults, setFloat: self.volume, forKey: &*volume_key];

            let pitch_key = cocoa_util::nsstr("pitch");
            let _: () = msg_send![&defaults, setFloat: self.pitch, forKey: &*pitch_key];

            let pref_exists_key = cocoa_util::nsstr("pref_exists");
            let _: () = msg_send![&defaults, setObject: &*pref_exists_key, forKey: &*pref_exists_key];
        }
    }
}
