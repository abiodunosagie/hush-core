//! # hush-core
//!
//! A real-time sound engine for calm and sleep: pure tones, binaural and isochronic beats, seven
//! colours of noise and five procedural nature sounds, mixed with a breathing swell, side-to-side
//! spatial movement, a long master fade and a soft clipper.
//!
//! The render loop never allocates, locks or blocks. Controls live in [`Params`], a set of atomics
//! shared through an `Arc`, so a UI thread can change anything while audio plays.
//!
//! ```
//! use std::sync::Arc;
//! use hush_core::{Params, Synth, ToneMode};
//!
//! let params = Arc::new(Params::default());
//! params.set_tone_mode(ToneMode::Binaural);
//! params.beat_hz.set(6.0);
//! params.play(2.0);
//!
//! let mut synth = Synth::new(48_000.0, params.clone());
//! let mut buffer = vec![0.0f32; 512 * 2];
//! synth.render_interleaved(&mut buffer, 2);
//! ```

pub mod ambient;
pub mod dsp;
pub mod journey;
pub mod noise;
pub mod params;
pub mod presets;
pub mod rng;
pub mod synth;

pub use ambient::AmbientKind;
pub use noise::NoiseColor;
pub use params::{AtomicF32, Params, ToneMode};
pub use synth::Synth;
