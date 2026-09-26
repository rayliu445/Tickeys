//! Sound playback on AVFoundation (AVAudioPlayer).
//!
//! Replaces the old freealut/OpenAL backend: freealut is long gone from
//! Homebrew, its bundled dylib was x86_64-only, and OpenAL is deprecated by
//! Apple. AVAudioPlayer ships with the OS, decodes plain PCM WAV fine, and has
//! a plain `volume` property — which is what the settings slider drives.
//!
//! AVAudioPlayer is declared here by hand: objc2-av-foundation doesn't ship
//! it (the class lives in AVFAudio), and raw msg_send calls must NOT be used
//! for init-family methods (objc2 gives them special retain semantics —
//! getting that wrong sends messages to freed objects).

use objc2::extern_class;
use objc2::extern_methods;
use objc2::AnyThread;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::NSObject;
use objc2_foundation::{NSData, NSError};

extern_class!(
    /// AVAudioPlayer from the AVFAudio part of AVFoundation.
    #[unsafe(super(NSObject))]
    pub struct AVAudioPlayer;
);

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

// Objective-C selector spellings are intentional
#[allow(non_snake_case)]
impl AVAudioPlayer {
    extern_methods!(
        #[unsafe(method(initWithData:error:))]
        pub unsafe fn initWithData_error(
            this: Allocated<Self>,
            data: &NSData,
            error: *mut *mut NSError,
        ) -> Option<Retained<Self>>;

        #[unsafe(method(prepareToPlay))]
        pub unsafe fn prepareToPlay(&self) -> bool;

        #[unsafe(method(play))]
        pub unsafe fn play(&self) -> bool;

        #[unsafe(method(stop))]
        pub unsafe fn stop(&self);

        #[unsafe(method(setVolume:))]
        pub unsafe fn setVolume(&self, volume: f32);

        #[unsafe(method(setEnableRate:))]
        pub unsafe fn setEnableRate(&self, flag: bool);

        #[unsafe(method(setRate:))]
        pub unsafe fn setRate(&self, rate: f32);

        #[unsafe(method(setCurrentTime:))]
        pub unsafe fn setCurrentTime(&self, time: f64);
    );
}

pub struct AudioPlayers {
    /// One round-robin pool of players per sound file, so rapid repeats of the
    /// same key overlap instead of cutting each other off.
    pools: Vec<Vec<Retained<AVAudioPlayer>>>,
    next: Vec<usize>,
    volume: f32,
    pitch: f32,
    pool_per_file: usize,
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

        let mut loaded: Vec<Vec<Retained<AVAudioPlayer>>> = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read audio file {}: {}", path, e))?;
            let data = NSData::from_vec(bytes);
            let mut pool = Vec::with_capacity(self.pool_per_file);
            for _ in 0..self.pool_per_file {
                let mut error: *mut NSError = std::ptr::null_mut();
                let player = unsafe {
                    AVAudioPlayer::initWithData_error(
                        AVAudioPlayer::alloc(),
                        &data,
                        &mut error,
                    )
                };
                let player = player
                    .ok_or_else(|| format!("failed to load audio file {}", path))?;

                unsafe {
                    player.setVolume(self.volume);
                    player.setEnableRate(true);
                    player.setRate(self.pitch);
                    player.prepareToPlay();
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
        let (pool, pool_len) = {
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
        let player = unsafe { &*pool.add(*slot) };

        unsafe {
            // stop() + rewind so a still-playing instance restarts from the top
            player.stop();
            player.setCurrentTime(0.0);
            player.play();
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        for pool in &self.pools {
            for player in pool {
                unsafe { player.setVolume(volume) }
            }
        }
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
        for pool in &self.pools {
            for player in pool {
                unsafe { player.setRate(pitch) }
            }
        }
    }

    fn unload(&mut self) {
        for pool in self.pools.drain(..) {
            for player in pool {
                unsafe { player.stop() }
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
