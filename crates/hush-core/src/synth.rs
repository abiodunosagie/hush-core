//! The real-time mixer: tone, noise and ambient layers, then swell, master fade and soft clip.
//!
//! `render_*` never allocates, locks or blocks, so it is safe to call from an audio callback.

use crate::ambient::{AmbientBank, AmbientKind};
use crate::noise::{NoiseChannel, NoiseColor};
use crate::params::{Params, ToneMode};
use core::f64::consts::TAU;
use std::sync::Arc;

/// Tone volume 1.0 maps to this peak amplitude, leaving headroom for the other layers.
pub const TONE_MIX: f32 = 0.5;
/// Breathing swell rate: 6 cycles a minute, a common slow relaxed breathing pace.
pub const SWELL_HZ: f64 = 0.1;
pub use crate::params::SWELL_DEPTH;
/// Turning the swell on or off glides over this long, so switching mid-breath never steps the volume.
pub const SWELL_GLIDE_SECONDS: f32 = 1.5;
/// Spatial mode: one full swing across the head takes this long.
pub const SPATIAL_PERIOD_SECONDS: f64 = 16.0;
/// How far the spatial pan swings toward each ear: 1 would reach fully one side.
pub const SPATIAL_SWING: f64 = 0.85;
/// Isochronic on/off edges are raised-cosine ramps this long, so the pulse never clicks.
pub const ISOCHRONIC_EDGE_SECONDS: f64 = 0.01;
/// Noise colour changes dip to silence and back over this long, so the filter reset never pops.
pub const COLOR_DIP_SECONDS: f32 = 0.05;
/// Knob and switch smoothing time constant.
pub const SMOOTHING_SECONDS: f32 = 0.02;

pub struct Synth {
    params: Arc<Params>,
    sample_rate: f64,
    left: NoiseChannel,
    right: NoiseChannel,
    ambient: AmbientBank,
    active_color: NoiseColor,
    color_gain: f32,
    /// f64, not f32: a 10 minute fade steps about 3.5e-8 per sample, below f32 precision near 1.0.
    master: f64,
    noise_level: f32,
    tone_level: f32,
    swell_depth: f32,
    spatial_amount: f32,
    /// 0 = steady tone, 1 = fully gated isochronic. Smoothed so switching modes is seamless.
    iso_amount: f32,
    ambient_levels: [f32; AmbientKind::COUNT],
    ambient_targets: [f32; AmbientKind::COUNT],
    phase_l: f64,
    phase_r: f64,
    gate_phase: f64,
    swell_phase: f64,
    spatial_phase: f64,
    color_step: f32,
    smoothing: f32,
    swell_smoothing: f32,
}

impl Synth {
    /// A synth at rest (master gain 0).
    pub fn new(sample_rate: f32, params: Arc<Params>) -> Self {
        Self::resume(sample_rate, params, 0.0, 0.0)
    }

    /// A synth that continues from a previous engine's master gain and breathing position,
    /// for example after the output device changed sample rate mid-fade.
    pub fn resume(
        sample_rate: f32,
        params: Arc<Params>,
        start_master: f64,
        start_swell_phase: f64,
    ) -> Self {
        let sr = sample_rate;
        let active_color = params.noise_color();
        Self {
            params,
            sample_rate: sr as f64,
            left: NoiseChannel::new(0xA5A5_1234_DEAD_BEEF, sr),
            right: NoiseChannel::new(0x0123_4567_89AB_CDEF, sr),
            ambient: AmbientBank::new(sr),
            active_color,
            color_gain: 1.0,
            master: start_master,
            noise_level: 0.0,
            tone_level: 0.0,
            swell_depth: 0.0,
            spatial_amount: 0.0,
            iso_amount: 0.0,
            ambient_levels: [0.0; AmbientKind::COUNT],
            ambient_targets: [0.0; AmbientKind::COUNT],
            phase_l: 0.0,
            phase_r: 0.0,
            gate_phase: 0.0,
            swell_phase: start_swell_phase,
            spatial_phase: 0.0,
            color_step: 1.0 / (COLOR_DIP_SECONDS * sr),
            smoothing: 1.0 - (-1.0 / (SMOOTHING_SECONDS * sr)).exp(),
            swell_smoothing: 1.0 - (-1.0 / (SWELL_GLIDE_SECONDS * sr)).exp(),
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate as f32
    }

    /// One-pole step that snaps to the target once close, so values never decay into slow subnormal floats.
    #[inline]
    fn smooth(value: f32, target: f32, k: f32) -> f32 {
        let next = value + (target - value) * k;
        if (target - next).abs() < 1e-6 {
            target
        } else {
            next
        }
    }

    /// Leaves normal levels untouched so a pure tone stays pure, and rounds off only loud peaks.
    /// A hard clamp would crackle.
    #[inline]
    pub fn soft_clip(x: f32) -> f32 {
        const KNEE: f32 = 0.7;
        let a = x.abs();
        if a <= KNEE {
            return x;
        }
        let shaped = KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh();
        shaped.copysign(x)
    }

    /// Isochronic gate for a phase in [0, 1): on for the first half, off for the second, raised-cosine edges.
    #[inline]
    pub fn isochronic_gate(p: f64, edge: f64) -> f32 {
        use core::f64::consts::PI;
        if p < edge {
            (0.5 - 0.5 * (PI * p / edge).cos()) as f32
        } else if p < 0.5 {
            1.0
        } else if p < 0.5 + edge {
            (0.5 + 0.5 * (PI * (p - 0.5) / edge).cos()) as f32
        } else {
            0.0
        }
    }

    /// Renders into separate left and right buffers of equal length.
    pub fn render_planar(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        self.render_with(n, |i, l, r| {
            left[i] = l;
            right[i] = r;
        });
    }

    /// Renders into an interleaved buffer with `channels` channels. Channels past the second get silence;
    /// a mono buffer gets the average of left and right.
    pub fn render_interleaved(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        self.render_with(frames, |i, l, r| {
            let frame = &mut out[i * channels..(i + 1) * channels];
            if channels == 1 {
                frame[0] = 0.5 * (l + r);
            } else {
                frame[0] = l;
                frame[1] = r;
                for s in &mut frame[2..] {
                    *s = 0.0;
                }
            }
        });
    }

    fn render_with(&mut self, frames: usize, mut write: impl FnMut(usize, f32, f32)) {
        let p = &*self.params;
        let requested = p.noise_color();
        let mode = p.tone_mode();
        let noise_target = if p.noise_enabled() {
            p.noise_level()
        } else {
            0.0
        };
        let tone_target = if p.tone_enabled() {
            p.tone_level()
        } else {
            0.0
        };
        let swell_target = p.swell_depth();
        let spatial_target = if p.spatial() { 1.0 } else { 0.0 };
        let iso_target = if mode == ToneMode::Isochronic {
            1.0
        } else {
            0.0
        };
        let beat = p.beat_hz() as f64;
        let freq_l = p.tone_frequency() as f64;
        let freq_r = freq_l
            + if mode == ToneMode::Binaural {
                beat
            } else {
                0.0
            };
        let sr = self.sample_rate;
        let inc_l = TAU * freq_l / sr;
        let inc_r = TAU * freq_r / sr;
        let inc_gate = beat / sr;
        // The edge cannot exceed a quarter cycle, or fast pulses would never reach full on or off.
        let gate_edge = (ISOCHRONIC_EDGE_SECONDS * beat).min(0.25);
        let inc_swell = TAU * SWELL_HZ / sr;
        let inc_spatial = TAU / (SPATIAL_PERIOD_SECONDS * sr);
        let master_target = p.master_target() as f64;
        let master_step = 1.0 / (p.master_fade_seconds() as f64 * sr);
        for (target, kind) in self.ambient_targets.iter_mut().zip(AmbientKind::ALL) {
            *target = p.ambient_level(kind);
        }

        for i in 0..frames {
            if requested != self.active_color {
                self.color_gain -= self.color_step;
                if self.color_gain <= 0.0 {
                    self.color_gain = 0.0;
                    self.active_color = requested;
                    self.left.reset();
                    self.right.reset();
                }
            } else if self.color_gain < 1.0 {
                self.color_gain = (self.color_gain + self.color_step).min(1.0);
            }

            if self.master < master_target {
                self.master = (self.master + master_step).min(master_target);
            } else if self.master > master_target {
                self.master = (self.master - master_step).max(master_target);
            }

            self.noise_level = Self::smooth(self.noise_level, noise_target, self.smoothing);
            self.tone_level = Self::smooth(self.tone_level, tone_target, self.smoothing);
            self.swell_depth = Self::smooth(self.swell_depth, swell_target, self.swell_smoothing);
            self.spatial_amount = Self::smooth(self.spatial_amount, spatial_target, self.smoothing);
            self.iso_amount = Self::smooth(self.iso_amount, iso_target, self.smoothing);

            // Bed: noise and ambient layers. Spatial mode moves the bed; the tone stays centred,
            // because panning a binaural tone would destroy the beat.
            let mut bed_l = 0.0f32;
            let mut bed_r = 0.0f32;
            if self.noise_level > 0.0001 {
                let gain = self.color_gain * self.noise_level;
                bed_l += self.left.next(self.active_color) * gain;
                bed_r += self.right.next(self.active_color) * gain;
            }
            for (k, kind) in AmbientKind::ALL.iter().enumerate() {
                self.ambient_levels[k] = Self::smooth(
                    self.ambient_levels[k],
                    self.ambient_targets[k],
                    self.smoothing,
                );
                if self.ambient_levels[k] > 0.0001 {
                    let (a, b) = self.ambient.next(*kind);
                    bed_l += a * self.ambient_levels[k];
                    bed_r += b * self.ambient_levels[k];
                }
            }
            if self.spatial_amount > 0.0001 {
                // Equal-power pan swing: total loudness stays constant as the sound moves side to side.
                let theta =
                    core::f64::consts::FRAC_PI_4 * (1.0 + SPATIAL_SWING * self.spatial_phase.sin());
                let g_l = (theta.cos() * core::f64::consts::SQRT_2) as f32;
                let g_r = (theta.sin() * core::f64::consts::SQRT_2) as f32;
                bed_l *= 1.0 + self.spatial_amount * (g_l - 1.0);
                bed_r *= 1.0 + self.spatial_amount * (g_r - 1.0);
            }

            let mut l = bed_l;
            let mut r = bed_r;
            if self.tone_level > 0.0001 {
                let gate = Self::isochronic_gate(self.gate_phase, gate_edge);
                let gain = self.tone_level * TONE_MIX * (1.0 - self.iso_amount * (1.0 - gate));
                l += self.phase_l.sin() as f32 * gain;
                r += self.phase_r.sin() as f32 * gain;
            }
            self.phase_l = wrap(self.phase_l + inc_l, TAU);
            self.phase_r = wrap(self.phase_r + inc_r, TAU);
            self.gate_phase = wrap(self.gate_phase + inc_gate, 1.0);
            self.swell_phase = wrap(self.swell_phase + inc_swell, TAU);
            self.spatial_phase = wrap(self.spatial_phase + inc_spatial, TAU);

            // Swell dips the volume and comes back once per breath: 1 at the top, 1 - depth at the bottom.
            let swell = 1.0 - self.swell_depth * (0.5 - 0.5 * self.swell_phase.cos()) as f32;
            // A squared master curve sounds more natural than linear for fades.
            let g = (self.master * self.master) as f32 * swell;
            write(i, Self::soft_clip(l * g), Self::soft_clip(r * g));
        }

        p.report(self.master as f32, self.swell_phase as f32);
    }
}

#[inline]
fn wrap(x: f64, period: f64) -> f64 {
    if x >= period {
        x - period
    } else {
        x
    }
}
