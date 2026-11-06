// The sample-editor edits heard in the video, applied cumulatively to the snare:
// trim, varispeed pitch, EQ, echo and reverb. A sketch of the app's chain in
// editing.rs, good enough for a demo soundtrack.
import { SAMPLE_RATE } from "../src/score.ts";

const SR = SAMPLE_RATE;

function trim(x: Float32Array, seconds: number) {
  const n = Math.min(x.length, Math.round(seconds * SR));
  const fade = Math.round(0.03 * SR);
  const out = x.slice(0, n);
  for (let i = 0; i < fade; i++) out[n - 1 - i] *= i / fade;
  return out;
}

/** Varispeed: lower pitch plays slower and longer. */
function pitch(x: Float32Array, semitones: number) {
  const rate = 2 ** (semitones / 12);
  const out = new Float32Array(Math.floor(x.length / rate));
  for (let i = 0; i < out.length; i++) {
    const p = i * rate;
    const j = Math.floor(p);
    out[i] = (x[j] ?? 0) + ((x[j + 1] ?? 0) - (x[j] ?? 0)) * (p - j);
  }
  return out;
}

/** RBJ shelving biquad (Audio EQ Cookbook, S = 1). */
function shelf(x: Float32Array, kind: "low" | "high", freq: number, gainDb: number) {
  const A = 10 ** (gainDb / 40);
  const w = (2 * Math.PI * freq) / SR;
  const c = Math.cos(w);
  const q = 2 * Math.sqrt(A) * (Math.sin(w) / Math.SQRT2);
  const [b0, b1, b2, a0, a1, a2] =
    kind === "low"
      ? [
          A * (A + 1 - (A - 1) * c + q),
          2 * A * (A - 1 - (A + 1) * c),
          A * (A + 1 - (A - 1) * c - q),
          A + 1 + (A - 1) * c + q,
          -2 * (A - 1 + (A + 1) * c),
          A + 1 + (A - 1) * c - q,
        ]
      : [
          A * (A + 1 + (A - 1) * c + q),
          -2 * A * (A - 1 + (A + 1) * c),
          A * (A + 1 + (A - 1) * c - q),
          A + 1 - (A - 1) * c + q,
          2 * (A - 1 - (A + 1) * c),
          A + 1 - (A - 1) * c - q,
        ];
  return biquad(x, b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
}

function biquad(x: Float32Array, b0: number, b1: number, b2: number, a1: number, a2: number) {
  const out = new Float32Array(x.length);
  let x1 = 0, x2 = 0, y1 = 0, y2 = 0;
  for (let i = 0; i < x.length; i++) {
    const y = b0 * x[i] + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
    x2 = x1; x1 = x[i]; y2 = y1; y1 = y;
    out[i] = y;
  }
  return out;
}

function pad(x: Float32Array, seconds: number) {
  const out = new Float32Array(x.length + Math.round(seconds * SR));
  out.set(x);
  return out;
}

function echo(x: Float32Array, seconds: number, amount: number, feedback: number) {
  const d = Math.round(seconds * SR);
  const out = pad(x, seconds * 5);
  const wet = new Float32Array(out.length);
  for (let i = d; i < out.length; i++) wet[i] = (out[i - d] ?? 0) + wet[i - d] * feedback;
  for (let i = 0; i < out.length; i++) out[i] += wet[i] * amount;
  return out;
}

/** Freeverb-style: parallel damped combs into series allpasses. */
function reverb(x: Float32Array, amount: number, decay: number) {
  const out = pad(x, decay * 1.2);
  const wet = new Float32Array(out.length);
  for (const len of [1116, 1188, 1277, 1356, 1422, 1491]) {
    const buf = new Float32Array(len);
    let idx = 0;
    let low = 0;
    const fb = 10 ** ((-3 * len) / SR / decay);
    for (let i = 0; i < out.length; i++) {
      const y = buf[idx];
      low = y * 0.6 + low * 0.4;
      buf[idx] = out[i] + low * fb;
      idx = (idx + 1) % len;
      wet[i] += y / 6;
    }
  }
  let sig = wet;
  for (const len of [556, 441]) {
    const buf = new Float32Array(len);
    let idx = 0;
    const next = new Float32Array(sig.length);
    for (let i = 0; i < sig.length; i++) {
      const b = buf[idx];
      buf[idx] = sig[i] + b * 0.5;
      next[i] = b - sig[i] * 0.5;
      idx = (idx + 1) % len;
    }
    sig = next;
  }
  for (let i = 0; i < out.length; i++) out[i] += sig[i] * amount * 2.2;
  return out;
}

/** The snare after the first `count` edits. */
export function applyEdits(x: Float32Array, count: number) {
  let y = x;
  if (count >= 1) y = trim(y, 0.24);
  if (count >= 2) y = pitch(y, -4);
  if (count >= 3) y = shelf(shelf(y, "low", 180, 5), "high", 6000, -4);
  if (count >= 4) y = echo(y, 0.205, 0.35, 0.35);
  if (count >= 5) y = reverb(y, 0.38, 1.5);
  return y;
}
