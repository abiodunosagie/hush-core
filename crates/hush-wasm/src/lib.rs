//! hush-core for the browser. Exports plain functions (no wasm-bindgen glue), because an AudioWorklet
//! cannot load JS glue modules easily but can instantiate a raw `WebAssembly.Module`.
//!
//! Lifecycle: `hush_init(sample_rate)` once, then setters at any time, and `hush_render(frames)` per
//! audio quantum. Output lands in two f32 buffers whose addresses come from `hush_left` / `hush_right`.

use core::cell::UnsafeCell;
use core::sync::atomic::Ordering::Relaxed;
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
struct Global(UnsafeCell<Option<State>>);
// SAFETY: wasm32-unknown-unknown without threads has exactly one thread of execution.
unsafe impl Sync for Global {}
static STATE: Global = Global(UnsafeCell::new(None));

fn state() -> Option<&'static mut State> {
    // SAFETY: single-threaded, and no exported function holds this reference across a call.
    unsafe { (*STATE.0.get()).as_mut() }
}

/// Creates the engine. Allocates once here, so `hush_render` never allocates.
#[no_mangle]
pub extern "C" fn hush_init(sample_rate: f32) {
    let params = Arc::new(Params::default());
    let synth = Synth::new(sample_rate, params.clone());
    let s = State {
        params,
        synth,
        left: vec![0.0; MAX_FRAMES],
        right: vec![0.0; MAX_FRAMES],
    };
    // SAFETY: see `state`.
    unsafe { *STATE.0.get() = Some(s) };
}

#[no_mangle]
pub extern "C" fn hush_left() -> *const f32 {
    state().map_or(core::ptr::null(), |s| s.left.as_ptr())
}

#[no_mangle]
pub extern "C" fn hush_right() -> *const f32 {
    state().map_or(core::ptr::null(), |s| s.right.as_ptr())
}

/// Renders `frames` (clamped to MAX_FRAMES) into the left and right buffers.
#[no_mangle]
pub extern "C" fn hush_render(frames: u32) {
    if let Some(s) = state() {
        let n = (frames as usize).min(MAX_FRAMES);
        s.synth.render_planar(&mut s.left[..n], &mut s.right[..n]);
    }
}

fn with_params(f: impl FnOnce(&Params)) {
    if let Some(s) = state() {
        f(&s.params)
    }
}

#[no_mangle]
pub extern "C" fn hush_set_tone(enabled: u32, hz: f32, level: f32) {
    with_params(|p| {
        p.tone_enabled.store(enabled != 0, Relaxed);
        p.tone_frequency.set(hz.clamp(20.0, 2000.0));
        p.tone_level.set(level.clamp(0.0, 1.0));
    })
}

/// mode: 0 steady, 1 binaural, 2 isochronic.
#[no_mangle]
pub extern "C" fn hush_set_mode(mode: u32, beat_hz: f32) {
    with_params(|p| {
        p.set_tone_mode(ToneMode::from_u8(mode as u8));
        p.beat_hz.set(beat_hz.clamp(0.5, 40.0));
    })
}

/// color: index into NoiseColor::ALL (0 brown, 1 pink, 2 white, 3 green, 4 blue, 5 violet, 6 grey).
#[no_mangle]
pub extern "C" fn hush_set_noise(enabled: u32, color: u32, level: f32) {
    with_params(|p| {
        p.noise_enabled.store(enabled != 0, Relaxed);
        p.set_noise_color(NoiseColor::from_u8(color as u8));
        p.noise_level.set(level.clamp(0.0, 1.0));
    })
}

/// kind: index into AmbientKind::ALL (0 rain, 1 ocean, 2 wind, 3 fire, 4 stream). Level 0 turns it off.
#[no_mangle]
pub extern "C" fn hush_set_ambient(kind: u32, level: f32) {
    with_params(|p| {
        if let Some(k) = AmbientKind::ALL.get(kind as usize) {
            p.set_ambient(*k, level.clamp(0.0, 1.0));
        }
    })
}

#[no_mangle]
pub extern "C" fn hush_set_swell(on: u32) {
    with_params(|p| {
        p.swell_depth.set(if on != 0 {
            hush_core::synth::SWELL_DEPTH
        } else {
            0.0
        })
    })
}

#[no_mangle]
pub extern "C" fn hush_set_spatial(on: u32) {
    with_params(|p| p.spatial_enabled.store(on != 0, Relaxed))
}

#[no_mangle]
pub extern "C" fn hush_play(fade_seconds: f32) {
    with_params(|p| p.play(fade_seconds.max(0.01)))
}

#[no_mangle]
pub extern "C" fn hush_fade_out(fade_seconds: f32) {
    with_params(|p| p.fade_out(fade_seconds.max(0.01)))
}

/// Current master gain, 0 to 1. A UI polls this to know when a fade-out has finished.
#[no_mangle]
pub extern "C" fn hush_master_gain() -> f32 {
    state().map_or(0.0, |s| s.params.master_gain.get())
}

/// Breathing swell position in radians, for drawing a breathing guide.
#[no_mangle]
pub extern "C" fn hush_swell_phase() -> f32 {
    state().map_or(0.0, |s| s.params.swell_phase.get())
}
