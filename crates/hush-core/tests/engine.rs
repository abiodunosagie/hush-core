//! Offline checks through the real `Synth`: loudness, stability over long runs, pitch, pulse rate,
//! spatial loudness, switching, fade timing, device-change resume and the journey schedule.
//! Run with `cargo test --release` (the long runs are slow in debug builds).

use hush_core::journey::{beat_at, JourneyStep, GLIDE_SECONDS};
use hush_core::params::DEFAULT_TONE_HZ;
use hush_core::synth::{ISOCHRONIC_EDGE_SECONDS, SPATIAL_PERIOD_SECONDS};
use hush_core::{AmbientKind, NoiseColor, Params, Synth, ToneMode};
use std::sync::Arc;

const SR: f32 = 48_000.0;
const BLOCK: usize = 512;

fn playing() -> Arc<Params> {
    let p = Params::default();
    p.set_tone_enabled(false);
    p.play(0.5);
    Arc::new(p)
}

struct Stats {
    rms: f64,
    peak: f32,
    mean: f64,
    finite: bool,
}

/// Renders `seconds` after a one-second settle and measures both channels.
fn measure(synth: &mut Synth, seconds: f64) -> Stats {
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..(SR as usize / BLOCK) {
        synth.render_planar(&mut l, &mut r);
    }
    let blocks = (seconds * SR as f64) as usize / BLOCK;
    let (mut sum, mut sum_sq, mut peak, mut finite) = (0.0f64, 0.0f64, 0.0f32, true);
    for _ in 0..blocks {
        synth.render_planar(&mut l, &mut r);
        for i in 0..BLOCK {
            for v in [l[i], r[i]] {
                finite &= v.is_finite();
                peak = peak.max(v.abs());
                sum += v as f64;
                sum_sq += (v as f64) * (v as f64);
            }
        }
    }
    let n = (blocks * BLOCK * 2) as f64;
    let mean = sum / n;
    Stats {
        rms: (sum_sq / n - mean * mean).sqrt(),
        peak,
        mean,
        finite,
    }
}

fn noise_rms(color: NoiseColor) -> f64 {
    let p = playing();
    p.set_noise_enabled(true);
    p.set_noise_level(0.5);
    p.set_noise_color(color);
    let mut s = Synth::new(SR, p);
    measure(&mut s, 10.0).rms
}

fn ambient_rms(kind: AmbientKind) -> f64 {
    let p = playing();
    p.set_ambient(kind, 0.5);
    let mut s = Synth::new(SR, p);
    measure(&mut s, 30.0).rms
}

#[test]
fn every_colour_and_nature_sound_is_level_matched_to_brown() {
    let reference = noise_rms(NoiseColor::Brown);
    for c in NoiseColor::ALL {
        let ratio = noise_rms(c) / reference;
        assert!(
            (0.5..1.6).contains(&ratio),
            "{} is {ratio:.2}x brown",
            c.label()
        );
    }
    for k in AmbientKind::ALL {
        let ratio = ambient_rms(k) / reference;
        assert!(
            (0.5..1.6).contains(&ratio),
            "{} is {ratio:.2}x brown",
            k.label()
        );
    }
}

fn all_layers_max(color: NoiseColor, minutes: f64) -> Stats {
    let p = playing();
    p.set_noise_color(color);
    p.set_noise_enabled(true);
    p.set_noise_level(1.0);
    p.set_tone_enabled(true);
    p.set_tone_level(1.0);
    p.set_tone_mode(ToneMode::Binaural);
    p.set_beat_hz(12.0);
    p.set_swell(true);
    p.set_spatial(true);
    for k in AmbientKind::ALL {
        p.set_ambient(k, 1.0);
    }
    let mut s = Synth::new(SR, p);
    measure(&mut s, minutes * 60.0)
}

#[test]
fn every_layer_at_max_stays_finite_bounded_and_centred() {
    for c in NoiseColor::ALL {
        let st = all_layers_max(c, 5.0);
        // Everything at max saturates on purpose through the soft clip: never past full scale, no NaN, no DC drift.
        assert!(
            st.finite && st.peak <= 1.0 && st.mean.abs() < 0.05,
            "{}: peak {} mean {}",
            c.label(),
            st.peak,
            st.mean
        );
    }
}

#[test]
#[ignore = "three-hour soak, run with --include-ignored"]
fn three_hour_soak_with_every_layer_at_max() {
    let st = all_layers_max(NoiseColor::Brown, 180.0);
    assert!(st.finite && st.peak <= 1.0 && st.mean.abs() < 0.05);
}

#[test]
fn default_tone_is_852_hz_and_clearly_audible() {
    let p = Arc::new(Params::default());
    p.play(0.5);
    let mut s = Synth::new(SR, p);
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..(SR as usize / BLOCK) {
        s.render_planar(&mut l, &mut r);
    }
    let blocks = (10.0 * SR) as usize / BLOCK;
    let (mut crossings, mut last, mut peak) = (0usize, l[BLOCK - 1], 0.0f32);
    for _ in 0..blocks {
        s.render_planar(&mut l, &mut r);
        for &v in &l {
            if (last < 0.0) != (v < 0.0) {
                crossings += 1;
            }
            last = v;
            peak = peak.max(v.abs());
        }
    }
    let hz = crossings as f64 / 2.0 / ((blocks * BLOCK) as f64 / SR as f64);
    assert!(
        (hz - DEFAULT_TONE_HZ as f64).abs() / (DEFAULT_TONE_HZ as f64) < 0.01,
        "measured {hz:.1} Hz"
    );
    assert!(peak > 0.2 && peak < 0.9, "peak {peak}");
}

#[test]
fn isochronic_at_10_hz_pulses_100_times_in_10_seconds_with_smooth_edges() {
    let p = playing();
    p.set_tone_enabled(true);
    p.set_tone_frequency(400.0);
    p.set_tone_level(1.0);
    p.set_tone_mode(ToneMode::Isochronic);
    p.set_beat_hz(10.0);
    let mut s = Synth::new(SR, p);
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..(SR as usize / BLOCK) {
        s.render_planar(&mut l, &mut r);
    }
    let window = (SR / 1000.0) as usize;
    let (mut count, mut n, mut peak, mut was_on) = (0, 0, 0.0f32, false);
    for _ in 0..(10.0 * SR) as usize / BLOCK {
        s.render_planar(&mut l, &mut r);
        for &v in &l {
            peak = peak.max(v.abs());
            n += 1;
            if n == window {
                let on = peak > 0.25;
                if on && !was_on {
                    count += 1;
                }
                was_on = on;
                n = 0;
                peak = 0.0;
            }
        }
    }
    assert!((count - 100i32).abs() <= 1, "{count} pulses");
    let edge = ISOCHRONIC_EDGE_SECONDS * 10.0;
    let max_step = (0..1000)
        .map(|k| {
            (Synth::isochronic_gate((k + 1) as f64 / 1000.0, edge)
                - Synth::isochronic_gate(k as f64 / 1000.0, edge))
            .abs()
        })
        .fold(0.0f32, f32::max);
    assert!(max_step < 0.2, "gate step {max_step}");
}

#[test]
fn spatial_movement_keeps_overall_loudness() {
    let rms = |spatial: bool| {
        let p = playing();
        p.set_noise_enabled(true);
        p.set_noise_level(0.5);
        p.set_noise_color(NoiseColor::Pink);
        p.set_spatial(spatial);
        let mut s = Synth::new(SR, p);
        measure(&mut s, SPATIAL_PERIOD_SECONDS * 2.0).rms
    };
    let (off, on) = (rms(false), rms(true));
    assert!((on / off - 1.0).abs() < 0.03, "{on} vs {off}");
}

#[test]
fn rapid_colour_and_mode_switching_stays_clean_then_fades_to_exact_silence() {
    let p = playing();
    p.set_noise_enabled(true);
    p.set_noise_level(1.0);
    let mut s = Synth::new(SR, p.clone());
    let modes = [ToneMode::Steady, ToneMode::Binaural, ToneMode::Isochronic];
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for k in 0..2000 {
        if k % 10 == 0 {
            p.set_noise_color(NoiseColor::ALL[(k / 10) % NoiseColor::ALL.len()]);
            p.set_tone_mode(modes[(k / 10) % modes.len()]);
            p.set_tone_enabled(k % 20 == 0);
        }
        s.render_planar(&mut l, &mut r);
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.0));
    }
    p.fade_out(0.5);
    for _ in 0..(SR as usize / BLOCK + 2) {
        s.render_planar(&mut l, &mut r);
    }
    assert_eq!(p.master_gain(), 0.0);
    assert!(l.iter().chain(&r).all(|v| *v == 0.0));
}

#[test]
fn a_ten_minute_fade_lasts_ten_minutes() {
    let fade = 600.0;
    let p = Arc::new(Params::default());
    p.fade_out(fade as f32);
    let mut s = Synth::resume(SR, p.clone(), 1.0, 0.0);
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    let mut rendered = 0usize;
    let mut silent_at = None;
    while rendered < ((fade + 5.0) * SR as f64) as usize {
        s.render_planar(&mut l, &mut r);
        rendered += BLOCK;
        if silent_at.is_none() && p.master_gain() == 0.0 {
            silent_at = Some(rendered as f64 / SR as f64);
        }
    }
    let at = silent_at.expect("never reached silence");
    assert!((at - fade).abs() < 1.0, "silent at {at:.1}s");
}

#[test]
fn a_rebuilt_engine_continues_the_fade_and_breath() {
    let p = Arc::new(Params::default());
    p.fade_out(60.0);
    let mut old = Synth::resume(SR, p.clone(), 1.0, 0.0);
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..(10.0 * SR) as usize / BLOCK {
        old.render_planar(&mut l, &mut r);
    }
    let (before, breath_before) = (p.master_gain(), p.swell_phase());
    let mut rebuilt = Synth::resume(44_100.0, p.clone(), before as f64, breath_before as f64);
    rebuilt.render_planar(&mut l, &mut r);
    let breath_gap = ((p.swell_phase() - breath_before) as f64).rem_euclid(std::f64::consts::TAU);
    let breath_gap = breath_gap.min(std::f64::consts::TAU - breath_gap);
    assert!(before > 0.5 && (p.master_gain() - before).abs() < 0.01 && breath_gap < 0.01);
}

#[test]
fn journey_holds_glides_and_keeps_the_last_stage() {
    let steps = [
        JourneyStep {
            beat: 10.0,
            minutes: 10.0,
        },
        JourneyStep {
            beat: 6.0,
            minutes: 15.0,
        },
        JourneyStep {
            beat: 2.0,
            minutes: 5.0,
        },
    ];
    let checks = [
        (0.0, 10.0),
        (599.0, 10.0),
        (600.0, 10.0),
        (600.0 + GLIDE_SECONDS / 2.0, 8.0),
        (600.0 + GLIDE_SECONDS, 6.0),
        (1400.0, 6.0),
        (1500.0 + GLIDE_SECONDS, 2.0),
        (36_000.0, 2.0),
    ];
    for (t, want) in checks {
        let got = beat_at(t, &steps).unwrap();
        assert!((got - want).abs() < 0.001, "at {t}s got {got} want {want}");
    }
    assert_eq!(beat_at(100.0, &[]), None);
}

#[test]
fn interleaved_output_handles_mono_stereo_and_extra_channels() {
    for channels in [1usize, 2, 6] {
        let p = Arc::new(Params::default());
        p.play(0.01);
        let mut s = Synth::new(SR, p);
        let mut buf = vec![1.0f32; BLOCK * channels];
        for _ in 0..20 {
            s.render_interleaved(&mut buf, channels);
        }
        assert!(buf.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        if channels > 2 {
            assert!(buf
                .chunks(channels)
                .all(|f| f[2..].iter().all(|v| *v == 0.0)));
        }
        assert!(
            buf.iter().any(|v| v.abs() > 0.01),
            "{channels} channels rendered silence"
        );
    }
}
