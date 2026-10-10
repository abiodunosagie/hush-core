// AudioWorklet processor: runs hush-core (Rust, compiled to WebAssembly) on the browser's audio thread.
// The main thread sends the .wasm bytes once, then small control messages.

class HushProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const module = new WebAssembly.Module(options.processorOptions.wasm);
    this.ex = new WebAssembly.Instance(module, {}).exports;
    this.ex.hush_init(sampleRate);
    this.frame = 0;
    this.port.onmessage = (e) => {
      const { fn, args } = e.data;
      const f = this.ex[fn];
      // Only the control functions are callable from the page, never render or init.
      const allowed = fn.startsWith("hush_set_") || fn === "hush_play" || fn === "hush_fade_out";
      if (allowed && typeof f === "function") {
        f(...args);
      }
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const n = out[0].length;
    const max = this.ex.hush_max_frames();
    // Render in chunks the engine can hold, in case the browser ever hands over a larger quantum.
    for (let at = 0; at < n; at += max) {
      const len = Math.min(max, n - at);
      this.ex.hush_render(len);
      // Memory can grow, so build views from the current buffer every time.
      const mem = this.ex.memory.buffer;
      out[0].set(new Float32Array(mem, this.ex.hush_left(), len), at);
      if (out[1]) out[1].set(new Float32Array(mem, this.ex.hush_right(), len), at);
    }
    // About 20 status updates a second for the breathing guide and the stop button.
    if ((this.frame += n) >= sampleRate / 20) {
      this.frame = 0;
      this.port.postMessage({ gain: this.ex.hush_master_gain(), swell: this.ex.hush_swell_phase() });
    }
    return true;
  }
}

registerProcessor("hush", HushProcessor);
