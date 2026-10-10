//! Small filters used by the noise and ambient generators.

use crate::rng::Rng;
use core::f32::consts::PI;

/// One-pole low-pass filter.
#[derive(Clone, Debug)]
pub struct OnePole {
    a: f32,
    y: f32,
}

impl OnePole {
    pub fn new(cutoff: f32, sample_rate: f32) -> Self {
        Self {
            a: Self::coefficient(cutoff, sample_rate),
            y: 0.0,
        }
    }

    pub fn coefficient(cutoff: f32, sample_rate: f32) -> f32 {
        1.0 - (-2.0 * PI * cutoff / sample_rate).exp()
    }

    pub fn set_cutoff(&mut self, cutoff: f32, sample_rate: f32) {
        self.a = Self::coefficient(cutoff, sample_rate);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }

    pub fn reset(&mut self) {
        self.y = 0.0;
    }
}

/// RBJ band-pass biquad (0 dB peak gain), transposed direct form II.
#[derive(Clone, Debug)]
pub struct Biquad {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn bandpass(center: f32, q: f32, sample_rate: f32) -> Self {
        let mut f = Self {
            b0: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        };
        f.set_bandpass(center, q, sample_rate);
        f
    }

    /// Changes the filter without clearing its state, so it can sweep without clicks.
    pub fn set_bandpass(&mut self, center: f32, q: f32, sample_rate: f32) {
        // A centre at or above Nyquist makes the filter unstable, so keep it safely below.
        let center = center.min(0.45 * sample_rate);
        let w0 = 2.0 * PI * center / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * w0.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = -self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Random value that drifts smoothly toward a new target every `interval` seconds. Used for natural wobble.
#[derive(Clone, Debug)]
pub struct SmoothRandom {
    value: f32,
    target: f32,
    counter: u32,
    interval: u32,
    coefficient: f32,
    low: f32,
    high: f32,
}

impl SmoothRandom {
    pub fn new(low: f32, high: f32, interval: f32, glide: f32, sample_rate: f32) -> Self {
        let mid = (low + high) / 2.0;
        Self {
            value: mid,
            target: mid,
            counter: 0,
            interval: ((interval * sample_rate) as u32).max(1),
            coefficient: 1.0 - (-1.0 / (glide * sample_rate)).exp(),
            low,
            high,
        }
    }

    #[inline]
    pub fn next(&mut self, rng: &mut Rng) -> f32 {
        if self.counter == 0 {
            self.counter = self.interval;
            self.target = self.low + (self.high - self.low) * rng.unit();
        }
        self.counter -= 1;
        self.value += (self.target - self.value) * self.coefficient;
        self.value
    }
}
