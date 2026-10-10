//! Proves the render loop never touches the heap: a counting allocator wraps the system allocator,
//! and every layer is switched on while blocks render.

use hush_core::{AmbientKind, NoiseColor, Params, Synth, ToneMode};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn render_never_allocates() {
    let p = Arc::new(Params::default());
    p.noise_enabled.store(true, Ordering::Relaxed);
    p.set_tone_mode(ToneMode::Isochronic);
    p.swell_depth.set(0.35);
    p.spatial_enabled.store(true, Ordering::Relaxed);
    for k in AmbientKind::ALL {
        p.set_ambient(k, 0.5);
    }
    p.play(0.1);
    let mut synth = Synth::new(48_000.0, p.clone());
    let mut left = vec![0.0f32; 512];
    let mut right = vec![0.0f32; 512];
    let mut interleaved = vec![0.0f32; 1024];

    let before = ALLOCATIONS.load(Ordering::SeqCst);
    for k in 0..500 {
        p.set_noise_color(NoiseColor::ALL[k % NoiseColor::ALL.len()]);
        synth.render_planar(&mut left, &mut right);
        synth.render_interleaved(&mut interleaved, 2);
    }
    let after = ALLOCATIONS.load(Ordering::SeqCst);
    assert_eq!(
        after - before,
        0,
        "render allocated {} times",
        after - before
    );
}
