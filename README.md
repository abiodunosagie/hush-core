# hush-core

[![CI](https://github.com/abiodunosagie/hush-core/actions/workflows/ci.yml/badge.svg)](https://github.com/abiodunosagie/hush-core/actions/workflows/ci.yml)

A real-time sound engine for calm and sleep, written in Rust. It plays pure tones, binaural and isochronic beats, seven colours of noise and five procedural nature sounds, all synthesized sample by sample. Nothing is a recording, so nothing loops.

It ships in three pieces:

| Crate | What it is |
|---|---|
| `hush-core` | The engine library. The render loop never allocates, locks or blocks. |
| `hush-cli` | `hush`, a terminal player for macOS, Linux and Windows (via cpal). |
| `hush-wasm` | The engine compiled to WebAssembly, running inside an AudioWorklet. |

**Try it in your browser:** [hush-core.smitho.workers.dev](https://hush-core.smitho.workers.dev)

This is the Rust port of the engine inside [Hush](https://github.com/abiodunosagie/hush), my native macOS app, with its test suite ported alongside.

## Quick start

```bash
git clone https://github.com/abiodunosagie/hush-core.git
cd hush-core
cargo install --path crates/hush-cli   # installs the `hush` command
hush --help
```

Some examples:

```bash
hush                                               # 852 Hz pure tone
hush --preset "Deep sleep"                         # binaural 1 Hz on a 136 Hz carrier (headphones)
hush --mode isochronic --beat 10 --tone 400        # pulsing tone, works on speakers
hush --no-tone --noise brown --rain 0.5 --spatial  # brown noise and rain, sweeping side to side
hush --wind-down --swell --timer 75m               # pulse walks from 10 Hz down to 1.5 Hz, slow fade at the end
hush --list                                        # every tone and preset
```

On Linux, install the ALSA headers first (`sudo apt install libasound2-dev`).

## Using the library

```rust
use std::sync::Arc;
use hush_core::{NoiseColor, Params, Synth, ToneMode};

let params = Arc::new(Params::default());   // 852 Hz steady tone by default
params.set_tone_mode(ToneMode::Binaural);
params.set_beat_hz(6.0);
params.set_noise_enabled(true);
params.set_noise_color(NoiseColor::Pink);
params.play(2.0);                            // two-second fade-in

let mut synth = Synth::new(48_000.0, params.clone());

// In your audio callback:
let mut buffer = vec![0.0f32; 512 * 2];
synth.render_interleaved(&mut buffer, 2);

// From any other thread, while audio plays:
params.fade_out(600.0);                      // ten-minute fade to silence
```

`Params` is a set of atomics behind an `Arc`, so a UI thread can change anything while the audio thread renders. Setters ignore non-finite values and clamp the rest to the engine's ranges, so one bad input can never poison the oscillators.

## Features

- **Tones.** Any frequency, with the Solfeggio set (174 to 963 Hz) and 432 Hz as presets.
- **Three tone modes.**
  - Steady: the same tone in both ears.
  - Binaural: a slightly different frequency in each ear.
  - Isochronic: the tone pulses on and off with raised-cosine edges.
  - There are 15 binaural presets and 5 isochronic presets, from 1 to 40 Hz.
- **Noise.** Brown, pink, white, green, blue, violet and grey, level-matched to each other.
- **Nature sounds.** Rain, ocean waves, wind, fireplace and stream, made from filtered noise. You can layer as many as you like.
- **Movement.** A breathing swell at six cycles a minute, and an equal-power side-to-side pan for the noise and nature sounds. The tone stays centred, because panning a binaural tone would destroy the beat.
- **Journeys.** The pulse rate steps down in stages and glides between them.
- **Long fades.** The master gain ramps over any length, from a two-second fade-in to a ten-minute sleep fade. In the terminal player, Ctrl-C fades out instead of cutting the sound.

Tone and preset names (Third eye, Deep sleep and so on) are the traditional Solfeggio and brainwave labels people search for. This is not a medical device and makes no health claims.

## How it is built

- **The render loop does not allocate. A test proves it.** `tests/no_alloc.rs` installs a counting global allocator, switches every layer on, renders 500 blocks while changing noise colour, and asserts zero allocations on the rendering thread. The working state is fixed-size arrays, filled in place.
- **No locks on the audio thread.** Controls are atomics with relaxed ordering. Each value stands alone, and a change landing one block late is inaudible.
- **Validated controls.** NaN or infinity is ignored at the setter, and filter centres are kept below Nyquist, so a bad value or a low sample rate cannot make the output blow up.
- **Every gain change ramps.**
  - Knobs and switches go through a one-pole smoother that snaps to its target once close, so values never decay into slow subnormal floats.
  - A noise colour change dips to silence before the filter state resets, then comes back.
- **The master fade is an `f64`.** A ten-minute fade moves about 3.5e-8 per sample, which is below `f32` precision near 1.0. With an `f32`, the fade ends early. A test checks that a 600-second fade lasts 600 seconds.
- **Soft clipping instead of a hard clamp.** Output below 0.7 is untouched, so a pure tone stays pure. Only peaks above that are rounded off with `tanh`.
- **Seamless device changes.** `Synth::resume` starts a new engine from the old master gain and breathing position. When the output device changes sample rate mid-fade, the sound continues where it was.
- **WebAssembly without glue.** `hush-wasm` exports plain `extern "C"` functions and imports nothing. The AudioWorklet instantiates the 50 KB module directly and reads the left and right buffers straight out of linear memory.

## Tests

```bash
cargo test --workspace --release
cargo test --release -p hush-core --test engine -- --include-ignored   # adds the three-hour soak (about 30 s)
```

CI runs formatting, Clippy with warnings denied, and the full test suite on Linux, macOS and Windows, plus the WebAssembly build. The engine tests check:

- Every noise colour and nature sound is level-matched to brown noise.
- Every layer at maximum stays finite, under full scale and free of DC drift. The ignored soak test runs this for three hours.
- The default tone measures as 852 Hz and is clearly audible.
- An isochronic beat at 10 Hz pulses exactly 100 times in 10 seconds, with smooth gate edges.
- Spatial movement keeps the overall loudness constant.
- Rapid colour and mode switching stays clean, then fades to exact silence.
- A ten-minute fade lasts ten minutes.
- A rebuilt engine continues the fade and the breathing position.
- The journey schedule holds, glides and keeps the last stage.
- Interleaved output handles mono, stereo and extra channels.
- `render` never allocates. This runs in a separate test binary whose counting allocator watches only the rendering thread.

## Browser demo

```bash
./scripts/build-web.sh                 # builds hush-wasm into web/hush.wasm
python3 -m http.server -d web 8000     # then open http://localhost:8000
```

The page sends the WebAssembly bytes to an AudioWorklet once, then posts small control messages. The worklet renders each 128-frame quantum in Rust. Only the control functions can be called from the page; render and init cannot. `web/hush.wasm` is committed so the demo deploys as plain files; rebuild it with the script after changing the engine. The demo is deployed as static assets on Cloudflare Workers (`wrangler.jsonc`).

## Project layout

```
crates/hush-core/src/
  synth.rs     real-time mixer: tone, noise and ambient layers, swell, spatial pan, fade, soft clip
  params.rs    atomic controls shared with the audio thread
  noise.rs     seven noise colours
  ambient.rs   procedural rain, ocean, wind, fire and stream
  dsp.rs       one-pole and biquad filters, smoothed random
  rng.rs       xorshift64* generator
  presets.rs   tones, binaural and isochronic presets, brainwave band names
  journey.rs   journey schedule (pure function)
crates/hush-core/tests/
  engine.rs    the engine checks above
  no_alloc.rs  counting-allocator proof
crates/hush-cli/src/main.rs   terminal player (clap + cpal)
crates/hush-wasm/src/lib.rs   C ABI for the browser
web/                          demo page, AudioWorklet processor
```

## License

MIT. See [LICENSE](LICENSE).

## Author

Abiodun Osagie, mobile, front-end and full-stack developer.
[Portfolio](https://smithportfolio.smitho.workers.dev) · [GitHub](https://github.com/abiodunosagie) · [LinkedIn](https://linkedin.com/in/osagieabiodun)
