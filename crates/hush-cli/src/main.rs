//! `hush`: calm and sleep sounds from the terminal, synthesized live by hush-core.

use clap::{Parser, ValueEnum};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use hush_core::journey::{beat_at, WIND_DOWN};
use hush_core::params::DEFAULT_TONE_HZ;
use hush_core::presets::{band_name, BINAURAL, ISOCHRONIC, TONES};
use hush_core::{AmbientKind, NoiseColor, Params, Synth, ToneMode};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Fade length when Ctrl-C is pressed.
const CTRL_C_FADE_SECONDS: f32 = 1.5;
/// Longest accepted timer, so absurd input cannot overflow `Duration`.
const MAX_TIMER_SECONDS: f64 = 48.0 * 3600.0;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Steady,
    Binaural,
    Isochronic,
}

#[derive(Parser, Debug)]
#[command(
    name = "hush",
    version,
    about = "Calm and sleep sounds, synthesized live. Ctrl-C to stop."
)]
struct Args {
    /// Tone frequency in Hz. Use --no-tone for noise or nature only.
    #[arg(long, default_value_t = DEFAULT_TONE_HZ)]
    tone: f32,
    /// Turn the tone off.
    #[arg(long)]
    no_tone: bool,
    /// Tone volume, 0 to 1.
    #[arg(long, default_value_t = 0.6)]
    volume: f32,
    /// Tone mode. Binaural needs headphones; isochronic works on speakers.
    #[arg(long, value_enum, default_value_t = Mode::Steady)]
    mode: Mode,
    /// Pulse rate in Hz for binaural and isochronic modes.
    #[arg(long, default_value_t = 6.0)]
    beat: f32,
    /// Use a named binaural or isochronic preset (see --list). Sets tone, mode and beat.
    #[arg(long)]
    preset: Option<String>,
    /// Add noise under the tone: brown, pink, white, green, blue, violet or grey.
    #[arg(long)]
    noise: Option<String>,
    /// Noise volume, 0 to 1.
    #[arg(long, default_value_t = 0.4)]
    noise_volume: f32,
    /// Rain volume, 0 to 1.
    #[arg(long, default_value_t = 0.0)]
    rain: f32,
    /// Ocean waves volume, 0 to 1.
    #[arg(long, default_value_t = 0.0)]
    ocean: f32,
    /// Wind volume, 0 to 1.
    #[arg(long, default_value_t = 0.0)]
    wind: f32,
    /// Fireplace volume, 0 to 1.
    #[arg(long, default_value_t = 0.0)]
    fire: f32,
    /// Stream volume, 0 to 1.
    #[arg(long, default_value_t = 0.0)]
    stream: f32,
    /// Breathing swell: the volume rises and falls six times a minute.
    #[arg(long)]
    swell: bool,
    /// Noise and nature sounds sweep slowly from side to side.
    #[arg(long)]
    spatial: bool,
    /// Stop after this long, fading out slowly over the end: 30m, 1h30m, 90s, or plain minutes.
    #[arg(long, value_parser = parse_duration)]
    timer: Option<Duration>,
    /// Walk the pulse down from 10 Hz to 1.5 Hz over 75 minutes (binaural unless --mode isochronic).
    #[arg(long)]
    wind_down: bool,
    /// List tones and presets, then exit.
    #[arg(long)]
    list: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim().to_lowercase();
    let seconds = if let Ok(minutes) = s.parse::<f64>() {
        minutes * 60.0
    } else {
        let (mut total, mut number) = (0.0f64, String::new());
        for c in s.chars() {
            if c.is_ascii_digit() || c == '.' {
                number.push(c);
                continue;
            }
            let n: f64 = number.parse().map_err(|_| format!("bad duration: {s}"))?;
            number.clear();
            total += n * match c {
                'h' => 3600.0,
                'm' => 60.0,
                's' => 1.0,
                _ => return Err(format!("bad unit '{c}' in {s}; use h, m or s")),
            };
        }
        if !number.is_empty() {
            return Err(format!("missing unit after {number} in {s}"));
        }
        total
    };
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("duration must be a positive length of time".into());
    }
    if seconds > MAX_TIMER_SECONDS {
        return Err("duration must be 48 hours or less".into());
    }
    Ok(Duration::from_secs_f64(seconds))
}

fn list() {
    println!("Tones:");
    for t in TONES {
        println!("  {:>4} Hz  {}", t.hz, t.name);
    }
    println!("\nBinaural presets (--preset, headphones):");
    for p in BINAURAL {
        println!(
            "  {:<14} {:>4} Hz carrier, {:>4} Hz beat ({})",
            p.name,
            p.carrier,
            p.beat,
            band_name(p.beat)
        );
    }
    println!("\nIsochronic presets (--preset, speakers):");
    for p in ISOCHRONIC {
        println!(
            "  {:<14} {:>4} Hz carrier, {:>4} Hz beat ({})",
            p.name,
            p.carrier,
            p.beat,
            band_name(p.beat)
        );
    }
    println!("\nNoise: brown, pink, white, green, blue, violet, grey");
}

fn configure(args: &Args) -> Result<Arc<Params>, String> {
    let p = Params::default();
    let (mut hz, mut mode, mut beat) = (args.tone, args.mode, args.beat);
    if let Some(name) = &args.preset {
        if let Some(b) = BINAURAL.iter().find(|b| b.name.eq_ignore_ascii_case(name)) {
            (hz, mode, beat) = (b.carrier, Mode::Binaural, b.beat);
        } else if let Some(b) = ISOCHRONIC
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
        {
            (hz, mode, beat) = (b.carrier, Mode::Isochronic, b.beat);
        } else {
            return Err(format!("unknown preset '{name}', see --list"));
        }
    }
    if args.wind_down && matches!(mode, Mode::Steady) {
        mode = Mode::Binaural;
    }
    let numbers = [
        ("--tone", hz),
        ("--volume", args.volume),
        ("--beat", beat),
        ("--noise-volume", args.noise_volume),
        ("--rain", args.rain),
        ("--ocean", args.ocean),
        ("--wind", args.wind),
        ("--fire", args.fire),
        ("--stream", args.stream),
    ];
    if let Some((name, _)) = numbers.iter().find(|(_, v)| !v.is_finite()) {
        return Err(format!("{name} must be a number"));
    }
    // Params setters clamp to the engine's ranges.
    p.set_tone_enabled(!args.no_tone);
    p.set_tone_frequency(hz);
    p.set_tone_level(args.volume);
    p.set_tone_mode(match mode {
        Mode::Steady => ToneMode::Steady,
        Mode::Binaural => ToneMode::Binaural,
        Mode::Isochronic => ToneMode::Isochronic,
    });
    p.set_beat_hz(beat);
    if let Some(name) = &args.noise {
        let color =
            NoiseColor::from_name(name).ok_or_else(|| format!("unknown noise colour '{name}'"))?;
        p.set_noise_color(color);
        p.set_noise_enabled(true);
        p.set_noise_level(args.noise_volume);
    }
    p.set_ambient(AmbientKind::Rain, args.rain);
    p.set_ambient(AmbientKind::Ocean, args.ocean);
    p.set_ambient(AmbientKind::Wind, args.wind);
    p.set_ambient(AmbientKind::Fire, args.fire);
    p.set_ambient(AmbientKind::Stream, args.stream);
    p.set_swell(args.swell);
    p.set_spatial(args.spatial);
    Ok(Arc::new(p))
}

/// Opens the default output device and starts rendering into it in whatever sample format it wants.
fn start_stream(params: Arc<Params>, failed: Arc<AtomicBool>) -> Result<cpal::Stream, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no audio output device found")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    match format {
        SampleFormat::F32 => build::<f32>(&device, config, params, failed),
        SampleFormat::I16 => build::<i16>(&device, config, params, failed),
        SampleFormat::U16 => build::<u16>(&device, config, params, failed),
        SampleFormat::I32 => build::<i32>(&device, config, params, failed),
        other => Err(format!("unsupported sample format {other}")),
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    params: Arc<Params>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let mut synth = Synth::new(config.sample_rate as f32, params);
    // Scratch for the f32 render, allocated here once so the callback never allocates.
    let mut scratch = vec![0.0f32; 8192 * channels];
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for chunk in data.chunks_mut(scratch.len()) {
                    let buf = &mut scratch[..chunk.len()];
                    synth.render_interleaved(buf, channels);
                    for (out, s) in chunk.iter_mut().zip(buf.iter()) {
                        *out = T::from_sample(*s);
                    }
                }
            },
            move |err| {
                // Report once; the main loop sees the flag and exits instead of waiting on a dead stream.
                if !failed.swap(true, Relaxed) {
                    eprintln!("hush: audio stopped: {err}");
                }
            },
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn main() {
    let args = Args::parse();
    if args.list {
        list();
        return;
    }
    let params = configure(&args).unwrap_or_else(|e| {
        eprintln!("hush: {e}");
        std::process::exit(2);
    });
    let failed = Arc::new(AtomicBool::new(false));
    let _stream = start_stream(params.clone(), failed.clone()).unwrap_or_else(|e| {
        eprintln!("hush: {e}");
        std::process::exit(1);
    });
    // Ctrl-C fades out over a moment instead of cutting the sound mid-waveform.
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        if let Err(e) = ctrlc::set_handler(move || stop.store(true, Relaxed)) {
            eprintln!("hush: Ctrl-C will stop abruptly ({e})");
        }
    }
    params.play(2.0);
    let started = Instant::now();
    println!("Playing. Ctrl-C to stop.");

    // Longest slow fade at the end of a timer; shorter timers fade over their last third.
    let fade = args.timer.map(|t| (t.as_secs_f32() / 3.0).min(600.0));
    let mut stopping_since: Option<Instant> = None;
    let mut ctrl_c_handled = false;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if failed.load(Relaxed) {
            std::process::exit(1);
        }
        let elapsed = started.elapsed();
        if stopping_since.is_none() {
            if args.wind_down {
                if let Some(b) = beat_at(elapsed.as_secs_f32(), &WIND_DOWN) {
                    params.set_beat_hz(b);
                }
            }
            if let (Some(total), Some(fade)) = (args.timer, fade) {
                let left = total.saturating_sub(elapsed).as_secs_f32();
                if left <= fade {
                    params.fade_out(left.max(1.0));
                    stopping_since = Some(Instant::now());
                }
            }
        }
        // Ctrl-C always wins, even during a long timer fade: shorten it to a quick fade once.
        if stop.load(Relaxed) && !ctrl_c_handled {
            ctrl_c_handled = true;
            params.fade_out(CTRL_C_FADE_SECONDS);
            stopping_since = Some(Instant::now());
        }
        if let Some(since) = stopping_since {
            // Done when the engine reports silence, or by the clock if the audio thread has stalled.
            let deadline = params.master_fade_seconds() + 2.0;
            if params.master_gain() == 0.0 || since.elapsed().as_secs_f32() > deadline {
                break;
            }
        }
    }
}
