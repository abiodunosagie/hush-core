//! hush-core for the browser. Exports plain functions (no wasm-bindgen glue), because an AudioWorklet
//! cannot load JS glue modules easily but can instantiate a raw `WebAssembly.Module`.
//!
//! Lifecycle: `hush_init(sample_rate)` once, then setters at any time, and `hush_render(frames)` per
//! audio quantum. Output lands in two f32 buffers whose addresses come from `hush_left` / `hush_right`.

use core::cell::UnsafeCell;
use hush_core::{AmbientKind, NoiseColor, Params, Synth, ToneMode};
use std::sync::Arc;

/// Largest block rendered in one call. AudioWorklet quanta are 128 frames; this leaves headroom.
const MAX_FRAMES: usize = 1024;

struct State {
    params: Arc<Params>,
    synth: Synth,
    left: Vec<f32>,
    right: Vec<f32>,
}

/// WebAssembly here runs on one thread (the AudioWorklet), so a single unsynchronised slot is sound.
/// Building with the wasm `atomics` target feature would break this assumption.
struct Global(UnsafeCell<Option<State>>);
// SAFETY: wasm32-unknown-unknown without threads has exactly one thread of execution.
unsafe impl Sync for Global {}
static STATE: Global = Global(UnsafeCell::new(None));

/// Runs `f` on the engine state, if initialised. The borrow never escapes the closure, so no two
/// mutable references to the state can be alive at once.
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    // SAFETY: single-threaded, no re-entrancy (exports never call each other), borrow scoped to `f`.
    unsafe { (*STATE.0.get()).as_mut().map(f) }
}

fn with_params(f: impl FnOnce(&Params)) {
    with_state(|s| f(&s.params));
}

/// Creates the engine. Allocates once here, so `hush_render` never allocates.
#[no_mangle]
pub extern "C" fn hush_init(sample_rate: f32) {
    let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    let params = Arc::new(Params::default());
    let synth = Synth::new(sample_rate, params.clone());
    let s = State {
        params,
        synth,
        left: vec![0.0; MAX_FRAMES],
        right: vec![0.0; MAX_FRAMES],
    };
    // SAFETY: see `with_state`; nothing is borrowed while this runs.
    unsafe { *STATE.0.get() = Some(s) };
}

/// Largest block `hush_render` fills; the page renders bigger quanta in chunks of this size.
#[no_mangle]
pub extern "C" fn hush_max_frames() -> u32 {
    MAX_FRAMES as u32
}

#[no_mangle]
pub extern "C" fn hush_left() -> *const f32 {
    with_state(|s| s.left.as_ptr()).unwrap_or(core::ptr::null())
}

#[no_mangle]
pub extern "C" fn hush_right() -> *const f32 {
    with_state(|s| s.right.as_ptr()).unwrap_or(core::ptr::null())
}

/// Renders `frames` (clamped to `hush_max_frames()`) into the left and right buffers.
#[no_mangle]
pub extern "C" fn hush_render(frames: u32) {
    with_state(|s| {
        let n = (frames as usize).min(MAX_FRAMES);
        s.synth.render_planar(&mut s.left[..n], &mut s.right[..n]);
    });
}

/// Setters below go through `Params`, which ignores non-finite values and clamps the rest.
#[no_mangle]
pub extern "C" fn hush_set_tone(enabled: u32, hz: f32, level: f32) {
    with_params(|p| {
        p.set_tone_enabled(enabled != 0);
        p.set_tone_frequency(hz);
        p.set_tone_level(level);
    })
}

/// mode: 0 steady, 1 binaural, 2 isochronic. Unknown modes are ignored.
#[no_mangle]
pub extern "C" fn hush_set_mode(mode: u32, beat_hz: f32) {
    with_params(|p| {
        if let Some(m) = u8::try_from(mode)
            .ok()
            .and_then(|m| ToneMode::try_from(m).ok())
        {
            p.set_tone_mode(m);
        }
        p.set_beat_hz(beat_hz);
    })
}

/// color: index into NoiseColor::ALL (0 brown, 1 pink, 2 white, 3 green, 4 blue, 5 violet, 6 grey).
#[no_mangle]
pub extern "C" fn hush_set_noise(enabled: u32, color: u32, level: f32) {
    with_params(|p| {
        p.set_noise_enabled(enabled != 0);
        if let Some(c) = NoiseColor::ALL.get(color as usize) {
            p.set_noise_color(*c);
        }
        p.set_noise_level(level);
    })
}

/// kind: index into AmbientKind::ALL (0 rain, 1 ocean, 2 wind, 3 fire, 4 stream). Level 0 turns it off.
#[no_mangle]
pub extern "C" fn hush_set_ambient(kind: u32, level: f32) {
    with_params(|p| {
        if let Some(k) = AmbientKind::ALL.get(kind as usize) {
            p.set_ambient(*k, level);
        }
    })
}

#[no_mangle]
pub extern "C" fn hush_set_swell(on: u32) {
    with_params(|p| p.set_swell(on != 0))
}

#[no_mangle]
pub extern "C" fn hush_set_spatial(on: u32) {
    with_params(|p| p.set_spatial(on != 0))
}

#[no_mangle]
pub extern "C" fn hush_play(fade_seconds: f32) {
    with_params(|p| p.play(fade_seconds))
}

#[no_mangle]
pub extern "C" fn hush_fade_out(fade_seconds: f32) {
    with_params(|p| p.fade_out(fade_seconds))
}

/// Current master gain, 0 to 1. A page polls this to know when a fade-out has finished.
#[no_mangle]
pub extern "C" fn hush_master_gain() -> f32 {
    with_state(|s| s.params.master_gain()).unwrap_or(0.0)
}

/// Breathing swell position in radians, for drawing a breathing guide.
#[no_mangle]
pub extern "C" fn hush_swell_phase() -> f32 {
    with_state(|s| s.params.swell_phase()).unwrap_or(0.0)
}
