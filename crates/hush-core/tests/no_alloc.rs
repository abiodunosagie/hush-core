//! Proves the render loop never touches the heap: a counting allocator wraps the system allocator and
//! counts allocations made by the rendering thread only (the test harness allocates on its own threads).

use hush_core::{AmbientKind, NoiseColor, Params, Synth, ToneMode};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    // const-initialised, so reading it inside the allocator never allocates itself.
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        }
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
    p.set_noise_enabled(true);
    p.set_tone_mode(ToneMode::Isochronic);
    p.set_swell(true);
    p.set_spatial(true);
    for k in AmbientKind::ALL {
        p.set_ambient(k, 0.5);
    }
    p.play(0.1);
    let mut synth = Synth::new(48_000.0, p.clone());
    let mut left = vec![0.0f32; 512];
    let mut right = vec![0.0f32; 512];
    let mut interleaved = vec![0.0f32; 1024];

    TRACKING.with(|t| t.set(true));
    for k in 0..500 {
        p.set_noise_color(NoiseColor::ALL[k % NoiseColor::ALL.len()]);
        synth.render_planar(&mut left, &mut right);
        synth.render_interleaved(&mut interleaved, 2);
    }
    TRACKING.with(|t| t.set(false));
    let count = ALLOCATIONS.load(Ordering::SeqCst);
    assert_eq!(count, 0, "render allocated {count} times");
}
