//! Tickeys core: key event handling, sound scheme loading and playback.

use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::audio::AudioPlayers;
use crate::event_tap::{self, KeyboardMonitor};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioScheme {
    pub name: String,
    pub display_name: String,
    pub files: Vec<String>,
    pub non_unique_count: u8,
    #[serde(rename = "key_audio_map")]
    pub key_audio_map: BTreeMap<u8, u8>,
}

pub struct Tickeys {
    volume: f32,
    pitch: f32,
    mute: bool,

    audio_player: AudioPlayers,
    keymap: BTreeMap<u8, u8>,
    first_n_non_unique: i16,

    last_keys: VecDeque<u8>,

    keyboard_monitor: Option<KeyboardMonitor>,

    on_keydown: Option<fn(sender: &Tickeys, key: u8)>,

    schemes: Vec<AudioScheme>,
}

impl Tickeys {
    pub fn new(schemes: Vec<AudioScheme>) -> Tickeys {
        Tickeys {
            volume: 1.0,
            pitch: 1.0,
            mute: false,
            audio_player: AudioPlayers::new(3),
            keymap: BTreeMap::new(),
            first_n_non_unique: -1,
            last_keys: VecDeque::with_capacity(8),
            keyboard_monitor: None,
            on_keydown: None,
            schemes,
        }
    }

    pub fn start(&mut self) {
        let ptr_to_self: *mut std::os::raw::c_void = self as *mut Tickeys as *mut std::os::raw::c_void;

        let tap = match KeyboardMonitor::new(Self::handle_keyboard_event, ptr_to_self) {
            Ok(t) => t,
            Err(msg) => panic!("error: KeyboardMonitor::new: {}", msg),
        };

        self.keyboard_monitor = Some(tap);
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.keyboard_monitor = None;
    }

    pub fn get_schemes(&self) -> &Vec<AudioScheme> {
        &self.schemes
    }

    fn find_scheme(&self, name: &str) -> AudioScheme {
        self.schemes
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .unwrap_or_else(|| self.schemes[0].clone())
    }

    pub fn load_scheme(&mut self, dir: &str, scheme_name: &str) {
        let scheme = self.find_scheme(scheme_name);

        let paths: Vec<String> = scheme
            .files
            .iter()
            .map(|f| format!("{}/{}", dir, f))
            .collect();

        if let Err(e) = self.audio_player.load_files(&paths) {
            // Do NOT panic here: a single broken sound pack must not take the
            // whole app down (the old version panicked on load failures).
            eprintln!("Tickeys: {}", e);
            return;
        }
        self.audio_player.set_volume(self.volume);
        self.audio_player.set_pitch(self.pitch);

        self.keymap = scheme.key_audio_map.clone();
        self.first_n_non_unique = scheme.non_unique_count as i16;
    }

    pub fn set_volume(&mut self, volume: f32) {
        if volume == self.volume {
            return;
        }
        self.volume = volume;
        self.audio_player.set_volume(volume);
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        if pitch == self.pitch {
            return;
        }
        self.pitch = pitch;
        self.audio_player.set_pitch(pitch);
    }

    pub fn set_mute(&mut self, mute: bool) {
        self.mute = mute;
    }

    #[allow(dead_code)]
    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    #[allow(dead_code)]
    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }

    pub fn get_last_keys(&self) -> &VecDeque<u8> {
        &self.last_keys
    }

    /// Runs on the main run loop (the tap's source lives there). Must never
    /// panic: a panic unwinding through CoreGraphics' C stack can wedge the
    /// whole input system.
    unsafe extern "C" fn handle_keyboard_event(
        _proxy: *mut std::os::raw::c_void,
        _event_type: u32,
        event: event_tap::CGEventRef,
        refcon: *mut std::os::raw::c_void,
    ) -> event_tap::CGEventRef {
        if refcon.is_null() {
            return event;
        }
        let keycode = KeyboardMonitor::keycode_of_event(event);
        let tickeys: &mut Tickeys = std::mem::transmute(refcon);
        tickeys.handle_keydown(keycode as u8);
        event
    }

    fn handle_keydown(&mut self, keycode: u8) {
        self.last_keys.push_back(keycode);
        if self.last_keys.len() > 6 {
            self.last_keys.pop_front();
        }

        if let Some(f) = self.on_keydown {
            f(self, keycode);
        }

        if self.mute {
            return;
        }

        let index: i32 = match self.keymap.get(&keycode) {
            Some(idx) => *idx as i32,
            None => {
                if self.first_n_non_unique <= 0 {
                    -1
                } else {
                    (keycode % (self.first_n_non_unique as u8)) as i32
                }
            }
        };
        if self.is_too_frequent(keycode) {
            return;
        }
        if index == -1 {
            return;
        }

        self.audio_player.play(index as usize);
    }

    pub fn set_on_keydown(&mut self, on_keydown: Option<fn(sender: &Tickeys, key: u8)>) {
        self.on_keydown = on_keydown;
    }

    fn is_too_frequent(&self, keycode: u8) -> bool {
        use std::sync::Mutex;

        static LAST: Mutex<(Option<Instant>, i16)> = Mutex::new((None, -1));

        let mut last = LAST.lock().unwrap();
        let now = Instant::now();

        if let Some(t) = last.0.as_ref() {
            if t.elapsed().as_millis() < 120 && last.1 == keycode as i16 {
                last.0 = Some(now);
                return true;
            }
        }
        last.0 = Some(now);
        last.1 = keycode as i16;
        false
    }
}

impl Drop for Tickeys {
    fn drop(&mut self) {
        println!("Tickeys::drop");
    }
}
