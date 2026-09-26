//! Sound playback on AVFoundation (AVAudioPlayer).
//!
//! Replaces the old freealut/OpenAL backend: freealut is long gone from
//! Homebrew, its bundled dylib was x86_64-only, and OpenAL is deprecated by
//! Apple. AVAudioPlayer ships with the OS, decodes plain PCM WAV fine, and has
//! a plain `volume` property — which is what the settings slider drives.

use objc2::class;
use objc2::msg_send;
use objc2::runtime::NSObject;
use objc2_foundation::NSString;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

pub struct AudioPlayers {
    /// One round-robin pool of players per sound file, so rapid repeats of the
    /// same key overlap instead of cutting each other off.
    pools: Vec<Vec<*mut NSObject>>,
    next: Vec<usize>,
    volume: f32,
    pitch: f32,
    pool_per_file: usize,
}

unsafe fn player_from_file(path: &str) -> Option<*mut NSObject> {
    let path_ns = NSString::from_str(path);
    let mut error: *mut NSObject = std::ptr::null_mut();
    let player: *mut NSObject = msg_send![class!(AVAudioPlayer), alloc];
    let player: *mut NSObject = msg_send![
        player,
        initWithContentsOfFile: &*path_ns,
        error: &mut error
    ];
    if player.is_null() {
        let _ = error; // error object autoreleases; message is not user-facing
        return None;
    }
    // Allow `rate` (pitch) control; must be enabled before playing.
    let _: () = msg_send![player, setEnableRate: true];
    // Warm up the underlying audio queue to shave latency off first keypress.
    let _: bool = msg_send![player, prepareToPlay];
    Some(player)
}

impl AudioPlayers {
    pub fn new(pool_per_file: usize) -> AudioPlayers {
        AudioPlayers {
            pools: Vec::new(),
            next: Vec::new(),
            volume: 1.0,
            pitch: 1.0,
            pool_per_file: pool_per_file.max(1),
        }
    }

    pub fn load_files(&mut self, paths: &[String]) -> Result<(), String> {
        self.unload();

        let mut loaded: Vec<Vec<*mut NSObject>> = Vec::with_capacity(paths.len());
        for path in paths {
            let mut pool = Vec::with_capacity(self.pool_per_file);
            for _ in 0..self.pool_per_file {
                let player = unsafe { player_from_file(path) }
                    .ok_or_else(|| format!("failed to load audio file: {}", path))?;
                unsafe {
                    let _: () = msg_send![player, setVolume: self.volume];
                    let _: () = msg_send![player, setRate: self.pitch];
                }
                pool.push(player);
            }
            loaded.push(pool);
        }

        self.pools = loaded;
        self.next = vec![0; self.pools.len()];
        Ok(())
    }

    pub fn play(&mut self, index: usize) {
        // split borrows: grab the pool as raw, then advance the round-robin
        let (pool_ptr, pool_len) = {
            let pool = match self.pools.get_mut(index) {
                Some(p) if !p.is_empty() => p,
                _ => return,
            };
            (pool.as_mut_ptr(), pool.len())
        };
        let next_len = self.next.len().max(1);
        let slot = match self.next.get_mut(index % next_len) {
            Some(s) => s,
            None => return,
        };
        *slot = (*slot + 1) % pool_len;
        let player = unsafe { *pool_ptr.add(*slot) };

        unsafe {
            // stop() + rewind so a still-playing instance restarts from the top
            let _: () = msg_send![player, stop];
            let _: () = msg_send![player, setCurrentTime: 0.0f64];
            let _: bool = msg_send![player, play];
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        for pool in &self.pools {
            for &player in pool {
                unsafe {
                    let _: () = msg_send![player, setVolume: volume];
                }
            }
        }
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
        for pool in &self.pools {
            for &player in pool {
                unsafe {
                    let _: () = msg_send![player, setRate: pitch];
                }
            }
        }
    }

    fn unload(&mut self) {
        for pool in self.pools.drain(..) {
            for player in pool {
                unsafe {
                    let _: () = msg_send![player, stop];
                    let _: () = msg_send![player, release];
                }
            }
        }
        self.next.clear();
    }
}

impl Drop for AudioPlayers {
    fn drop(&mut self) {
        self.unload();
    }
}
