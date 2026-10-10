// Main thread: builds the audio graph on the first Play (browsers need a user gesture) and sends controls.
const TONES = [
  [174, "Foundation"], [285, "Restore"], [396, "Release"], [417, "Change"], [432, "Verdi tuning"],
  [528, "Love"], [639, "Connect"], [741, "Clarity"], [852, "Third eye"], [963, "Crown"],
];
const COLORS = ["Brown", "Pink", "White", "Green", "Blue", "Violet", "Grey"];
const NATURE = ["Rain", "Ocean waves", "Wind", "Fireplace", "Stream"];

// Defaults mirror hush-core's Params defaults (852 Hz, tone 0.6, noise 0.4, 6 Hz pulse).
const state = {
  tone: { on: true, hz: 852, level: 0.6 },
  mode: 0, beat: 6,
  noise: { on: false, color: 0, level: 0.4 },
  nature: [0, 0, 0, 0, 0],
  swell: false, spatial: false,
};

let ctx, node, loading = null;
// What the listener asked for. Set immediately on click, so a Stop during the first load still wins.
let playing = false;
const $ = (s) => document.querySelector(s);

function send(fn, ...args) {
  if (node) node.port.postMessage({ fn, args });
}

function pushAll() {
  send("hush_set_tone", state.tone.on ? 1 : 0, state.tone.hz, state.tone.level);
  send("hush_set_mode", state.mode, state.beat);
  send("hush_set_noise", state.noise.on ? 1 : 0, state.noise.color, state.noise.level);
  state.nature.forEach((v, i) => send("hush_set_ambient", i, v));
  send("hush_set_swell", state.swell ? 1 : 0);
  send("hush_set_spatial", state.spatial ? 1 : 0);
}

// Builds the audio graph once. Only assigns ctx and node when everything loaded, so a failed first
// attempt (network error, missing file) can simply be retried by pressing Play again.
async function ensureAudio() {
  if (node) return;
  const context = new AudioContext();
  try {
    const [wasm] = await Promise.all([
      fetch("hush.wasm").then((r) => {
        if (!r.ok) throw new Error(`hush.wasm: HTTP ${r.status}`);
        return r.arrayBuffer();
      }),
      context.audioWorklet.addModule("worklet.js"),
    ]);
    const n = new AudioWorkletNode(context, "hush", { outputChannelCount: [2], processorOptions: { wasm } });
    n.connect(context.destination);
    n.port.onmessage = (e) => onStatus(e.data);
    ctx = context;
    node = n;
  } catch (err) {
    context.close();
    throw err;
  }
}

async function start() {
  playing = true;
  render();
  try {
    loading = loading || ensureAudio();
    await loading;
  } catch (err) {
    console.error(err);
    $("#breath-text").textContent = "Could not load the audio engine. Press Play to try again.";
    playing = false;
    render();
    return;
  } finally {
    loading = null;
  }
  if (!playing) return; // stopped while loading
  await ctx.resume();
  if (!playing) return; // stopped while the context was resuming
  pushAll();
  send("hush_play", 2);
}

function stop() {
  playing = false;
  send("hush_fade_out", 1.5);
  render();
}

function onStatus({ gain, swell }) {
  // Suspend the context once a fade-out has reached silence, so the device can idle.
  if (!playing && gain === 0 && ctx && ctx.state === "running") ctx.suspend();
  const fullness = state.swell ? 0.5 + 0.5 * Math.cos(swell) : 0.5;
  $("#breath-dot").style.transform = `scale(${0.45 + 0.55 * fullness})`;
  $("#breath-text").textContent = !playing || !state.swell ? "Turn on the swell to breathe along" : swell > Math.PI ? "Breathe in" : "Breathe out";
}

function seg(container, items, active, onPick) {
  container.innerHTML = "";
  items.forEach((label, i) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    b.dataset.key = `${container.id}-${i}`;
    b.setAttribute("aria-pressed", String(i === active));
    b.onclick = () => onPick(i);
    container.appendChild(b);
  });
}

function render() {
  const focusedKey = document.activeElement && document.activeElement.dataset.key;
  $("#play").textContent = playing ? "Stop" : "Play";
  const tones = $("#tones");
  tones.innerHTML = "";
  TONES.forEach(([hz, name]) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tile";
    b.setAttribute("aria-pressed", String(state.tone.hz === hz));
    b.dataset.key = `tone-${hz}`;
    b.innerHTML = `<strong>${hz}</strong><span>${name}</span>`;
    b.onclick = () => { state.tone.hz = hz; pushAll(); render(); };
    tones.appendChild(b);
  });
  seg($("#modes"), ["Steady", "Binaural", "Isochronic"], state.mode, (i) => { state.mode = i; pushAll(); render(); });
  $("#beat-row").hidden = state.mode === 0;
  $("#beat-value").textContent = `${state.beat} Hz`;
  const colors = $("#colors");
  colors.innerHTML = "";
  COLORS.forEach((label, i) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tile";
    b.dataset.key = `color-${i}`;
    b.textContent = label;
    b.setAttribute("aria-pressed", String(state.noise.on && state.noise.color === i));
    b.onclick = () => {
      state.noise.color = i;
      state.noise.on = true;
      $("#noise-on").checked = true;
      pushAll();
      render();
    };
    colors.appendChild(b);
  });
  // Buttons were rebuilt; put keyboard focus back where it was.
  if (focusedKey) document.querySelector(`[data-key="${focusedKey}"]`)?.focus();
}

function bind() {
  $("#play").onclick = () => (playing ? stop() : start());
  $("#tone-on").onchange = (e) => { state.tone.on = e.target.checked; pushAll(); };
  $("#tone-level").oninput = (e) => { state.tone.level = +e.target.value; pushAll(); };
  $("#beat").oninput = (e) => { state.beat = +e.target.value; pushAll(); render(); };
  $("#noise-on").onchange = (e) => { state.noise.on = e.target.checked; pushAll(); render(); };
  $("#noise-level").oninput = (e) => { state.noise.level = +e.target.value; pushAll(); };
  $("#swell").onchange = (e) => { state.swell = e.target.checked; pushAll(); };
  $("#spatial").onchange = (e) => { state.spatial = e.target.checked; pushAll(); };
  const nature = $("#nature");
  NATURE.forEach((name, i) => {
    const row = document.createElement("label");
    row.className = "row";
    row.innerHTML = `<span>${name}</span><input type="range" min="0" max="1" step="0.01" value="0" aria-label="${name} volume">`;
    row.querySelector("input").oninput = (e) => { state.nature[i] = +e.target.value; pushAll(); };
    nature.appendChild(row);
  });
  document.addEventListener("keydown", (e) => {
    if (e.code === "Space" && !e.repeat && !["INPUT", "BUTTON"].includes(document.activeElement.tagName)) {
      e.preventDefault();
      playing ? stop() : start();
    }
  });
}

bind();
render();
