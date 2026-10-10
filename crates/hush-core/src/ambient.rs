//! Nature sounds generated live from filtered noise. No recordings, so they never loop.

use crate::dsp::{Biquad, OnePole, SmoothRandom};
use crate::rng::Rng;
use core::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AmbientKind {
    Rain = 0,
    Ocean,
    Wind,
    Fire,
    Stream,
}

impl AmbientKind {
    pub const ALL: [AmbientKind; 5] = [
        AmbientKind::Rain,
        AmbientKind::Ocean,
        AmbientKind::Wind,
        AmbientKind::Fire,
        AmbientKind::Stream,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn label(self) -> &'static str {
        match self {
            AmbientKind::Rain => "Rain",
            AmbientKind::Ocean => "Ocean waves",
            AmbientKind::Wind => "Wind",
            AmbientKind::Fire => "Fireplace",
            AmbientKind::Stream => "Stream",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
struct Rain {
    rng: Rng,
    hiss: Biquad,
    drop_tone: Biquad,
    drop_env: f32,
    drop_decay: f32,
    drop_chance: f32,
}

impl Rain {
    fn new(seed: u64, sr: f32) -> Self {
        Self {
            rng: Rng::new(seed),
            hiss: Biquad::bandpass(3000.0, 0.5, sr),
            drop_tone: Biquad::bandpass(4500.0, 1.2, sr),
            drop_env: 0.0,
            drop_decay: (-1.0 / (0.004 * sr)).exp(),
            drop_chance: 45.0 / sr,
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.rng.unit() < self.drop_chance {
            let a = 0.3 + 0.7 * self.rng.unit();
            self.drop_env = self.drop_env.max(a * a);
        }
        self.drop_env *= self.drop_decay;
        let w = self.rng.bipolar();
        let d = self.rng.bipolar();
        self.hiss.process(w) * 0.5 + self.drop_tone.process(d) * self.drop_env * 1.6
    }
}

#[derive(Clone, Debug)]
struct Ocean {
    rng_l: Rng,
    rng_r: Rng,
    lp_l: OnePole,
    lp_r: OnePole,
    phase: f32,
    step: f32,
    counter: u32,
    sr: f32,
}

impl Ocean {
    fn new(seed: u64, sr: f32) -> Self {
        Self {
            rng_l: Rng::new(seed),
            rng_r: Rng::new(seed.wrapping_add(77)),
            lp_l: OnePole::new(500.0, sr),
            lp_r: OnePole::new(500.0, sr),
            phase: 0.0,
            step: 1.0 / (9.0 * sr),
            counter: 0,
            sr,
        }
    }

    /// Wave swell shape: quiet trough, rising crash, long wash back.
    #[inline]
    fn envelope(p: f32) -> f32 {
        let s = 0.5 - 0.5 * (2.0 * PI * p).cos();
        0.12 + 0.88 * s * s
    }

    #[inline]
    fn next(&mut self) -> (f32, f32) {
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            // Each wave gets its own length, 7 to 12 seconds, so the pattern never repeats.
            self.step = 1.0 / ((7.0 + 5.0 * self.rng_l.unit()) * self.sr);
        }
        let env_l = Self::envelope(self.phase);
        let shifted = self.phase + 0.04;
        let env_r = Self::envelope(if shifted >= 1.0 {
            shifted - 1.0
        } else {
            shifted
        });
        if self.counter == 0 {
            self.counter = 32;
            self.lp_l.set_cutoff(250.0 + 2200.0 * env_l, self.sr);
            self.lp_r.set_cutoff(250.0 + 2200.0 * env_r, self.sr);
        }
        self.counter -= 1;
        let l = self.lp_l.process(self.rng_l.bipolar()) * env_l * 1.6;
        let r = self.lp_r.process(self.rng_r.bipolar()) * env_r * 1.6;
        (l, r)
    }
}

#[derive(Clone, Debug)]
struct Wind {
    rng: Rng,
    band: Biquad,
    center: SmoothRandom,
    gust: SmoothRandom,
    counter: u32,
    sr: f32,
}

impl Wind {
    fn new(seed: u64, sr: f32) -> Self {
        Self {
            rng: Rng::new(seed),
            band: Biquad::bandpass(500.0, 2.5, sr),
            center: SmoothRandom::new(250.0, 850.0, 2.5, 1.5, sr),
            gust: SmoothRandom::new(0.3, 1.0, 1.8, 1.2, sr),
            counter: 0,
            sr,
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        let center = self.center.next(&mut self.rng);
        if self.counter == 0 {
            self.counter = 64;
            self.band.set_bandpass(center, 2.5, self.sr);
        }
        self.counter -= 1;
        let w = self.rng.bipolar();
        let g = self.gust.next(&mut self.rng);
        self.band.process(w) * g * 4.0
    }
}

#[derive(Clone, Debug)]
struct Fire {
    rng: Rng,
    rumble: OnePole,
    pop_env: f32,
    last_noise: f32,
    pop_decay: f32,
    pop_chance: f32,
}

impl Fire {
    fn new(seed: u64, sr: f32) -> Self {
        Self {
            rng: Rng::new(seed),
            rumble: OnePole::new(160.0, sr),
            pop_env: 0.0,
            last_noise: 0.0,
            pop_decay: (-1.0 / (0.0012 * sr)).exp(),
            pop_chance: 10.0 / sr,
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.rng.unit() < self.pop_chance {
            let a = 0.2 + 0.8 * self.rng.unit();
            self.pop_env = self.pop_env.max(a * a);
        }
        self.pop_env *= self.pop_decay;
        let w = self.rng.bipolar();
        // Differenced noise gives the pop a sharp, woody click.
        let crackle = (w - self.last_noise) * self.pop_env;
        self.last_noise = w;
        let r = self.rng.bipolar();
        self.rumble.process(r) * 2.4 + crackle * 3.0
    }
}

#[derive(Clone, Debug)]
struct Stream {
    rng: Rng,
    high: Biquad,
    low: Biquad,
    bubble: SmoothRandom,
    flow: SmoothRandom,
}

impl Stream {
    fn new(seed: u64, sr: f32) -> Self {
        Self {
            rng: Rng::new(seed),
            high: Biquad::bandpass(1800.0, 0.8, sr),
            low: Biquad::bandpass(600.0, 0.7, sr),
            bubble: SmoothRandom::new(0.15, 1.0, 0.03, 0.012, sr),
            flow: SmoothRandom::new(0.5, 1.0, 0.6, 0.4, sr),
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        let b = self.bubble.next(&mut self.rng);
        let f = self.flow.next(&mut self.rng);
        let h = self.rng.bipolar();
        let l = self.rng.bipolar();
        self.high.process(h) * b * 1.25 + self.low.process(l) * f * 0.8
    }
}

/// Every ambient generator in stereo. Fixed set, no allocation, safe for the audio thread.
#[derive(Clone, Debug)]
pub struct AmbientBank {
    rain: (Rain, Rain),
    ocean: Ocean,
    wind: (Wind, Wind),
    fire: (Fire, Fire),
    stream: (Stream, Stream),
}

impl AmbientBank {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate;
        Self {
            rain: (Rain::new(11, sr), Rain::new(12, sr)),
            ocean: Ocean::new(21, sr),
            wind: (Wind::new(31, sr), Wind::new(32, sr)),
            fire: (Fire::new(41, sr), Fire::new(42, sr)),
            stream: (Stream::new(51, sr), Stream::new(52, sr)),
        }
    }

    #[inline]
    pub fn next(&mut self, kind: AmbientKind) -> (f32, f32) {
        match kind {
            AmbientKind::Rain => (self.rain.0.next(), self.rain.1.next()),
            AmbientKind::Ocean => self.ocean.next(),
            AmbientKind::Wind => (self.wind.0.next(), self.wind.1.next()),
            AmbientKind::Fire => (self.fire.0.next(), self.fire.1.next()),
            AmbientKind::Stream => (self.stream.0.next(), self.stream.1.next()),
        }
    }
}
