//! Parameters shared between a control thread (writer) and the audio thread (reader).
//!
//! Everything is an atomic, so the audio thread never takes a lock. Relaxed ordering is enough:
//! each value stands alone, and a change landing one block late is inaudible.

use crate::ambient::AmbientKind;
use crate::noise::NoiseColor;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering::Relaxed};

/// An `f32` stored in an `AtomicU32`.
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub const fn new(bits: u32) -> Self {
        Self(AtomicU32::new(bits))
    }
    pub fn from_f32(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    #[inline]
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
    #[inline]
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Relaxed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ToneMode {
    /// One clean tone, the same in both ears.
    Steady = 0,
    /// Each ear gets a slightly different frequency. Headphones only.
    Binaural,
    /// The tone pulses on and off at the beat rate. Works on speakers.
    Isochronic,
}

impl ToneMode {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => ToneMode::Binaural,
            2 => ToneMode::Isochronic,
            _ => ToneMode::Steady,
        }
    }
}

#[derive(Debug)]
pub struct Params {
    pub tone_enabled: AtomicBool,
    pub tone_frequency: AtomicF32,
    pub tone_level: AtomicF32,
    tone_mode: AtomicU8,
    /// Pulse rate in Hz, used by binaural and isochronic modes.
    pub beat_hz: AtomicF32,
    pub noise_enabled: AtomicBool,
    noise_color: AtomicU8,
    pub noise_level: AtomicF32,
    /// Effective level per ambient sound, indexed by `AmbientKind::index`. 0 = off.
    pub ambient_levels: [AtomicF32; AmbientKind::COUNT],
    /// Breathing swell depth. 0 = off.
    pub swell_depth: AtomicF32,
    pub spatial_enabled: AtomicBool,
    /// 1 = playing, 0 = silent. The master gain ramps toward this.
    pub master_target: AtomicF32,
    /// Seconds for the master gain to travel from 0 to 1 (or back).
    pub master_fade_seconds: AtomicF32,

    // Written by the audio thread, read by the control thread.
    /// Last rendered master gain, so a controller knows when a fade-out has finished.
    pub master_gain: AtomicF32,
    /// Breathing swell position in radians: 0 is the top of a breath, pi the bottom.
    pub swell_phase: AtomicF32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            tone_enabled: AtomicBool::new(true),
            tone_frequency: AtomicF32::from_f32(852.0),
            tone_level: AtomicF32::from_f32(0.6),
            tone_mode: AtomicU8::new(ToneMode::Steady as u8),
            beat_hz: AtomicF32::from_f32(6.0),
            noise_enabled: AtomicBool::new(false),
            noise_color: AtomicU8::new(NoiseColor::Brown as u8),
            noise_level: AtomicF32::from_f32(0.4),
            ambient_levels: Default::default(),
            swell_depth: AtomicF32::from_f32(0.0),
            spatial_enabled: AtomicBool::new(false),
            master_target: AtomicF32::from_f32(0.0),
            master_fade_seconds: AtomicF32::from_f32(1.0),
            master_gain: AtomicF32::from_f32(0.0),
            swell_phase: AtomicF32::from_f32(0.0),
        }
    }
}

impl Params {
    pub fn tone_mode(&self) -> ToneMode {
        ToneMode::from_u8(self.tone_mode.load(Relaxed))
    }
    pub fn set_tone_mode(&self, mode: ToneMode) {
        self.tone_mode.store(mode as u8, Relaxed)
    }
    pub fn noise_color(&self) -> NoiseColor {
        NoiseColor::from_u8(self.noise_color.load(Relaxed))
    }
    pub fn set_noise_color(&self, color: NoiseColor) {
        self.noise_color.store(color as u8, Relaxed)
    }
    pub fn set_ambient(&self, kind: AmbientKind, level: f32) {
        self.ambient_levels[kind.index()].set(level)
    }

    /// Start (or resume) playback with a fade-in of `seconds`.
    pub fn play(&self, seconds: f32) {
        self.master_fade_seconds.set(seconds);
        self.master_target.set(1.0);
    }

    /// Fade to silence over `seconds`.
    pub fn fade_out(&self, seconds: f32) {
        self.master_fade_seconds.set(seconds);
        self.master_target.set(0.0);
    }
}
