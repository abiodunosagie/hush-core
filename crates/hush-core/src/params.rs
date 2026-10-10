//! Controls shared between a control thread (writer) and the audio thread (reader).
//!
//! Everything is an atomic, so the audio thread never takes a lock. Relaxed ordering is enough:
//! each value stands alone, and a change landing one block late is inaudible.
//!
//! Setters validate their input. A non-finite value (NaN or infinity) is ignored and the previous
//! value is kept; finite values are clamped to the documented range. One bad input can therefore
//! never poison the oscillators.

use crate::ambient::AmbientKind;
use crate::noise::NoiseColor;
use core::ops::RangeInclusive;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering::Relaxed};

/// Accepted tone frequencies in Hz.
pub const TONE_RANGE: RangeInclusive<f32> = 20.0..=2000.0;
/// Accepted pulse rates in Hz for binaural and isochronic modes.
pub const BEAT_RANGE: RangeInclusive<f32> = 0.5..=40.0;
/// Accepted volume levels.
pub const LEVEL_RANGE: RangeInclusive<f32> = 0.0..=1.0;
/// Shortest fade, so a zero or negative fade still ramps instead of stepping.
pub const MIN_FADE_SECONDS: f32 = 0.01;

/// How far the breathing swell dips the volume at the bottom of each breath.
pub const SWELL_DEPTH: f32 = 0.35;

/// Default tone frequency.
pub const DEFAULT_TONE_HZ: f32 = 852.0;
/// Default tone volume.
pub const DEFAULT_TONE_LEVEL: f32 = 0.6;
/// Default pulse rate.
pub const DEFAULT_BEAT_HZ: f32 = 6.0;
/// Default noise volume.
pub const DEFAULT_NOISE_LEVEL: f32 = 0.4;

/// An `f32` stored in an `AtomicU32`.
#[derive(Debug, Default)]
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    #[inline]
    fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
    #[inline]
    fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Relaxed)
    }
    /// Stores `v` clamped to `range`, or does nothing when `v` is not finite.
    #[inline]
    fn set_in(&self, v: f32, range: RangeInclusive<f32>) {
        if v.is_finite() {
            self.set(v.clamp(*range.start(), *range.end()))
        }
    }
}

/// How the tone is presented.
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
    /// Every mode, in discriminant order.
    pub const ALL: [ToneMode; 3] = [ToneMode::Steady, ToneMode::Binaural, ToneMode::Isochronic];
}

impl TryFrom<u8> for ToneMode {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(ToneMode::Steady),
            1 => Ok(ToneMode::Binaural),
            2 => Ok(ToneMode::Isochronic),
            other => Err(other),
        }
    }
}

/// Every control the engine reads, plus two values the engine writes back.
#[derive(Debug)]
pub struct Params {
    tone_enabled: AtomicBool,
    tone_frequency: AtomicF32,
    tone_level: AtomicF32,
    tone_mode: AtomicU8,
    beat_hz: AtomicF32,
    noise_enabled: AtomicBool,
    noise_color: AtomicU8,
    noise_level: AtomicF32,
    ambient_levels: [AtomicF32; AmbientKind::COUNT],
    swell_depth: AtomicF32,
    spatial_enabled: AtomicBool,
    master_target: AtomicF32,
    master_fade_seconds: AtomicF32,
    // Written by the audio thread.
    master_gain: AtomicF32,
    swell_phase: AtomicF32,
}

impl Default for Params {
    /// An 852 Hz steady tone, everything else off, silent until [`Params::play`].
    fn default() -> Self {
        Self {
            tone_enabled: AtomicBool::new(true),
            tone_frequency: AtomicF32::new(DEFAULT_TONE_HZ),
            tone_level: AtomicF32::new(DEFAULT_TONE_LEVEL),
            tone_mode: AtomicU8::new(ToneMode::Steady as u8),
            beat_hz: AtomicF32::new(DEFAULT_BEAT_HZ),
            noise_enabled: AtomicBool::new(false),
            noise_color: AtomicU8::new(NoiseColor::Brown as u8),
            noise_level: AtomicF32::new(DEFAULT_NOISE_LEVEL),
            ambient_levels: Default::default(),
            swell_depth: AtomicF32::new(0.0),
            spatial_enabled: AtomicBool::new(false),
            master_target: AtomicF32::new(0.0),
            master_fade_seconds: AtomicF32::new(1.0),
            master_gain: AtomicF32::new(0.0),
            swell_phase: AtomicF32::new(0.0),
        }
    }
}

impl Params {
    /// Turns the tone layer on or off. The change is smoothed (see [`crate::synth::SMOOTHING_SECONDS`]).
    pub fn set_tone_enabled(&self, on: bool) {
        self.tone_enabled.store(on, Relaxed)
    }
    /// Whether the tone layer is on.
    pub fn tone_enabled(&self) -> bool {
        self.tone_enabled.load(Relaxed)
    }
    /// Tone frequency in Hz, clamped to [`TONE_RANGE`].
    pub fn set_tone_frequency(&self, hz: f32) {
        self.tone_frequency.set_in(hz, TONE_RANGE)
    }
    /// Tone frequency in Hz.
    pub fn tone_frequency(&self) -> f32 {
        self.tone_frequency.get()
    }
    /// Tone volume, clamped to [`LEVEL_RANGE`].
    pub fn set_tone_level(&self, level: f32) {
        self.tone_level.set_in(level, LEVEL_RANGE)
    }
    /// Tone volume.
    pub fn tone_level(&self) -> f32 {
        self.tone_level.get()
    }
    /// Steady, binaural or isochronic.
    pub fn set_tone_mode(&self, mode: ToneMode) {
        self.tone_mode.store(mode as u8, Relaxed)
    }
    /// Current tone mode.
    pub fn tone_mode(&self) -> ToneMode {
        ToneMode::try_from(self.tone_mode.load(Relaxed)).unwrap_or(ToneMode::Steady)
    }
    /// Pulse rate in Hz, clamped to [`BEAT_RANGE`].
    pub fn set_beat_hz(&self, hz: f32) {
        self.beat_hz.set_in(hz, BEAT_RANGE)
    }
    /// Pulse rate in Hz.
    pub fn beat_hz(&self) -> f32 {
        self.beat_hz.get()
    }
    /// Turns the noise layer on or off.
    pub fn set_noise_enabled(&self, on: bool) {
        self.noise_enabled.store(on, Relaxed)
    }
    /// Whether the noise layer is on.
    pub fn noise_enabled(&self) -> bool {
        self.noise_enabled.load(Relaxed)
    }
    /// Noise colour. A change dips to silence briefly (see [`crate::synth::COLOR_DIP_SECONDS`]) so the filter reset never pops.
    pub fn set_noise_color(&self, color: NoiseColor) {
        self.noise_color.store(color as u8, Relaxed)
    }
    /// Current noise colour.
    pub fn noise_color(&self) -> NoiseColor {
        NoiseColor::try_from(self.noise_color.load(Relaxed)).unwrap_or(NoiseColor::Brown)
    }
    /// Noise volume, clamped to [`LEVEL_RANGE`].
    pub fn set_noise_level(&self, level: f32) {
        self.noise_level.set_in(level, LEVEL_RANGE)
    }
    /// Noise volume.
    pub fn noise_level(&self) -> f32 {
        self.noise_level.get()
    }
    /// Volume of one nature sound, clamped to [`LEVEL_RANGE`]. 0 turns it off.
    pub fn set_ambient(&self, kind: AmbientKind, level: f32) {
        self.ambient_levels[kind.index()].set_in(level, LEVEL_RANGE)
    }
    /// Volume of one nature sound.
    pub fn ambient_level(&self, kind: AmbientKind) -> f32 {
        self.ambient_levels[kind.index()].get()
    }
    /// Breathing swell on or off. On dips the volume by [`SWELL_DEPTH`] once per breath.
    pub fn set_swell(&self, on: bool) {
        self.swell_depth.set(if on { SWELL_DEPTH } else { 0.0 })
    }
    /// Current swell depth, 0 when off.
    pub fn swell_depth(&self) -> f32 {
        self.swell_depth.get()
    }
    /// Side-to-side movement of the noise and nature sounds.
    pub fn set_spatial(&self, on: bool) {
        self.spatial_enabled.store(on, Relaxed)
    }
    /// Whether spatial movement is on.
    pub fn spatial(&self) -> bool {
        self.spatial_enabled.load(Relaxed)
    }

    /// Starts (or resumes) playback, fading in over `seconds`.
    pub fn play(&self, seconds: f32) {
        self.set_fade(seconds);
        self.master_target.set(1.0);
    }
    /// Fades to silence over `seconds`.
    pub fn fade_out(&self, seconds: f32) {
        self.set_fade(seconds);
        self.master_target.set(0.0);
    }
    fn set_fade(&self, seconds: f32) {
        let s = if seconds.is_finite() {
            seconds.max(MIN_FADE_SECONDS)
        } else {
            1.0
        };
        self.master_fade_seconds.set(s);
    }
    /// 1 while playing or fading in, 0 while stopped or fading out.
    pub fn master_target(&self) -> f32 {
        self.master_target.get()
    }
    /// Seconds a full fade takes.
    pub fn master_fade_seconds(&self) -> f32 {
        self.master_fade_seconds.get()
    }

    /// Last master gain the engine rendered, 0 to 1. Exactly 0.0 once a fade-out has finished.
    pub fn master_gain(&self) -> f32 {
        self.master_gain.get()
    }
    /// Breathing swell position in radians: 0 is the top of a breath, pi the bottom.
    pub fn swell_phase(&self) -> f32 {
        self.swell_phase.get()
    }
    /// Written by the engine after each block.
    pub(crate) fn report(&self, master_gain: f32, swell_phase: f32) {
        self.master_gain.set(master_gain);
        self.swell_phase.set(swell_phase);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_input_is_ignored_and_finite_input_is_clamped() {
        let p = Params::default();
        p.set_tone_frequency(f32::NAN);
        p.set_tone_level(f32::INFINITY);
        p.set_beat_hz(f32::NEG_INFINITY);
        assert_eq!(p.tone_frequency(), DEFAULT_TONE_HZ);
        assert_eq!(p.tone_level(), DEFAULT_TONE_LEVEL);
        assert_eq!(p.beat_hz(), DEFAULT_BEAT_HZ);
        p.set_tone_frequency(1.0e9);
        p.set_noise_level(-3.0);
        assert_eq!(p.tone_frequency(), *TONE_RANGE.end());
        assert_eq!(p.noise_level(), 0.0);
        p.fade_out(f32::NAN);
        assert_eq!(p.master_fade_seconds(), 1.0);
        p.play(0.0);
        assert_eq!(p.master_fade_seconds(), MIN_FADE_SECONDS);
    }

    #[test]
    fn enum_round_trips() {
        for m in ToneMode::ALL {
            assert_eq!(ToneMode::try_from(m as u8), Ok(m));
        }
        assert_eq!(ToneMode::try_from(9), Err(9));
    }
}
