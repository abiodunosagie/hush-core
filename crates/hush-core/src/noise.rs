//! Seven colours of noise, level-matched so switching colour does not change loudness.

use crate::dsp::{Biquad, OnePole};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NoiseColor {
    Brown = 0,
    Pink,
    White,
    Green,
    Blue,
    Violet,
    Grey,
}

impl NoiseColor {
    pub const ALL: [NoiseColor; 7] = [
        NoiseColor::Brown,
        NoiseColor::Pink,
        NoiseColor::White,
        NoiseColor::Green,
        NoiseColor::Blue,
        NoiseColor::Violet,
        NoiseColor::Grey,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NoiseColor::Brown => "Brown",
            NoiseColor::Pink => "Pink",
            NoiseColor::White => "White",
            NoiseColor::Green => "Green",
            NoiseColor::Blue => "Blue",
            NoiseColor::Violet => "Violet",
            NoiseColor::Grey => "Grey",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        *Self::ALL.get(v as usize).unwrap_or(&NoiseColor::Brown)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|c| c.label().eq_ignore_ascii_case(name))
    }
}

// Per-colour trims so every colour sounds roughly equally loud at the same volume setting.
const WHITE_TRIM: f32 = 0.3;
const PINK_TRIM: f32 = 0.11;
const BROWN_TRIM: f32 = 3.0;
const GREEN_TRIM: f32 = 0.22;
const BLUE_TRIM: f32 = 0.17;
const VIOLET_TRIM: f32 = 0.22;
const GREY_TRIM: f32 = 0.2;
/// Grey boosts the lows to roughly invert the ear's A-weighting curve.
const GREY_LOW_BOOST: f32 = 6.0;

/// One channel of coloured noise. Each channel has its own state so left and right are decorrelated.
#[derive(Clone, Debug)]
pub struct NoiseChannel {
    rng: Rng,
    // Paul Kellet pink filter state
    p: [f32; 7],
    brown: f32,
    last_white: f32,
    last_pink: f32,
    green_band: Biquad,
    grey_low: OnePole,
}

impl NoiseChannel {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        Self {
            rng: Rng::new(seed),
            p: [0.0; 7],
            brown: 0.0,
            last_white: 0.0,
            last_pink: 0.0,
            green_band: Biquad::bandpass(500.0, 0.6, sample_rate),
            grey_low: OnePole::new(250.0, sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.p = [0.0; 7];
        self.brown = 0.0;
        self.last_white = 0.0;
        self.last_pink = 0.0;
        self.green_band.reset();
        self.grey_low.reset();
    }

    #[inline]
    fn pink(&mut self, w: f32) -> f32 {
        let p = &mut self.p;
        p[0] = 0.99886 * p[0] + w * 0.0555179;
        p[1] = 0.99332 * p[1] + w * 0.0750759;
        p[2] = 0.96900 * p[2] + w * 0.153852;
        p[3] = 0.86650 * p[3] + w * 0.3104856;
        p[4] = 0.55000 * p[4] + w * 0.5329522;
        p[5] = -0.7616 * p[5] - w * 0.0168980;
        let out = p[0] + p[1] + p[2] + p[3] + p[4] + p[5] + p[6] + w * 0.5362;
        p[6] = w * 0.115926;
        out
    }

    #[inline]
    pub fn next(&mut self, color: NoiseColor) -> f32 {
        let w = self.rng.bipolar();
        match color {
            NoiseColor::White => w * WHITE_TRIM,
            NoiseColor::Pink => self.pink(w) * PINK_TRIM,
            NoiseColor::Brown => {
                // Leaky integrator: the leak keeps it centred over hours instead of drifting into clipping.
                self.brown = (self.brown + 0.02 * w) / 1.02;
                self.brown * BROWN_TRIM
            }
            NoiseColor::Green => {
                let p = self.pink(w);
                self.green_band.process(p) * GREEN_TRIM
            }
            NoiseColor::Blue => {
                // First difference of pink tilts the spectrum up by 6 dB per octave.
                let p = self.pink(w);
                let out = (p - self.last_pink) * BLUE_TRIM;
                self.last_pink = p;
                out
            }
            NoiseColor::Violet => {
                // First difference of white.
                let out = (w - self.last_white) * VIOLET_TRIM;
                self.last_white = w;
                out
            }
            NoiseColor::Grey => (w + self.grey_low.process(w) * GREY_LOW_BOOST) * GREY_TRIM,
        }
    }
}
