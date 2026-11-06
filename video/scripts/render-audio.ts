// Renders the soundtrack from the score with the Breakdance kit's own WAVs, plus the
// per-frame levels and waveforms the scenes animate with.
//   public/soundtrack.wav        stereo PCM16 44.1 kHz
//   src/generated/analysis.json  levels per frame, waveform peaks per sample
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  DURATION_IN_FRAMES,
  FPS,
  SAMPLES_FOLDER,
  SAMPLE_RATE,
  auditions,
  edits,
  files,
  hits,
  stepToSeconds,
  voices,
} from "../src/score.ts";
import { applyEdits } from "./edits.ts";

const here = dirname(fileURLToPath(import.meta.url));
const video = resolve(here, "..");
const samples = resolve(video, "..", SAMPLES_FOLDER);

/** Reads a mono or stereo PCM16 WAV as mono floats. */
function readWav(path: string): Float32Array {
  const buf = readFileSync(path);
  if (buf.toString("ascii", 0, 4) !== "RIFF" || buf.toString("ascii", 8, 12) !== "WAVE")
    throw new Error(`${path} is not a WAV file`);
  let offset = 12;
  let channels = 1;
  let bits = 16;
  let rate = SAMPLE_RATE;
  while (offset + 8 <= buf.length) {
    const id = buf.toString("ascii", offset, offset + 4);
    const size = buf.readUInt32LE(offset + 4);
    const body = offset + 8;
    if (id === "fmt ") {
      channels = buf.readUInt16LE(body + 2);
      rate = buf.readUInt32LE(body + 4);
      bits = buf.readUInt16LE(body + 14);
    } else if (id === "data") {
      if (bits !== 16 || rate !== SAMPLE_RATE)
        throw new Error(`${path}: expected PCM16 at ${SAMPLE_RATE} Hz`);
      const frames = Math.floor(size / 2 / channels);
      const out = new Float32Array(frames);
      for (let i = 0; i < frames; i++) {
        let sum = 0;
        for (let c = 0; c < channels; c++) sum += buf.readInt16LE(body + (i * channels + c) * 2);
        out[i] = sum / channels / 32768;
      }
      return out;
    }
    offset = body + size + (size % 2);
  }
  throw new Error(`${path} has no data chunk`);
}

const cache = new Map<string, Float32Array>();
const sample = (file: string, edit = 0) => {
  const key = `${file}#${edit}`;
  if (!cache.has(key)) {
    const original = readWav(join(samples, file));
    cache.set(key, edit ? applyEdits(original, edit) : original);
  }
  return cache.get(key)!;
};

const length = Math.ceil((DURATION_IN_FRAMES / FPS) * SAMPLE_RATE);
const mix = new Float32Array(length);
const perVoice = Array.from({ length: 5 }, () => new Float32Array(length));
const all = hits();
const CHOKE = Math.round(0.004 * SAMPLE_RATE);

for (const [index, hit] of all.entries()) {
  const data = sample(hit.file, hit.edit ?? 0);
  const start = Math.round(stepToSeconds(hit.at) * SAMPLE_RATE);
  // A voice is monophonic: the next hit on it cuts this one with a short fade.
  let end = start + data.length;
  if (hit.voice !== null) {
    const next = all.slice(index + 1).find((h) => h.voice === hit.voice);
    if (next) end = Math.min(end, Math.round(stepToSeconds(next.at) * SAMPLE_RATE));
  }
  for (let t = start; t < Math.min(end, length); t++) {
    const fade = Math.min(1, (end - t) / CHOKE);
    const value = data[t - start] * hit.gain * fade;
    mix[t] += value;
    if (hit.voice !== null) perVoice[hit.voice][t] += value;
  }
}

// Gentle master: normalise to -1 dBFS through a soft clipper.
let peak = 0;
for (const v of mix) peak = Math.max(peak, Math.abs(v));
const drive = 1.25 / Math.max(peak, 1e-9);
const ceiling = 10 ** (-1 / 20);
const shaped = mix.map((v) => (Math.tanh(v * drive) / Math.tanh(1.25)) * ceiling);

const wav = Buffer.alloc(44 + length * 4);
wav.write("RIFF", 0, "ascii");
wav.writeUInt32LE(36 + length * 4, 4);
wav.write("WAVEfmt ", 8, "ascii");
wav.writeUInt32LE(16, 16);
wav.writeUInt16LE(1, 20);
wav.writeUInt16LE(2, 22);
wav.writeUInt32LE(SAMPLE_RATE, 24);
wav.writeUInt32LE(SAMPLE_RATE * 4, 28);
wav.writeUInt16LE(4, 32);
wav.writeUInt16LE(16, 34);
wav.write("data", 36, "ascii");
wav.writeUInt32LE(length * 4, 40);
for (let i = 0; i < length; i++) {
  const s = Math.max(-32768, Math.min(32767, Math.round(shaped[i] * 32767)));
  wav.writeInt16LE(s, 44 + i * 4);
  wav.writeInt16LE(s, 46 + i * 4);
}
mkdirSync(join(video, "public"), { recursive: true });
writeFileSync(join(video, "public", "soundtrack.wav"), wav);

/** Peak level per video frame with a short release, so glows decay smoothly. */
function levels(signal: Float32Array, gain: number) {
  const per = SAMPLE_RATE / FPS;
  const out: number[] = [];
  let held = 0;
  for (let f = 0; f < DURATION_IN_FRAMES; f++) {
    let p = 0;
    for (let t = Math.floor(f * per); t < Math.min(signal.length, Math.floor((f + 1) * per)); t++)
      p = Math.max(p, Math.abs(signal[t]));
    held = Math.max(p * gain, held * 0.82);
    out.push(Number(Math.min(1, held).toFixed(3)));
  }
  return out;
}

/** Peak per bin over `seconds` (default: the whole sound), divided by `scale` (default: its own peak). */
function waveform(data: Float32Array, bins = 96, seconds?: number, scale?: number) {
  const out: number[] = [];
  const total = seconds ? seconds * SAMPLE_RATE : data.length;
  const per = total / bins;
  let max = 0;
  for (let b = 0; b < bins; b++) {
    let p = 0;
    for (let t = Math.floor(b * per); t < Math.min(data.length, Math.floor((b + 1) * per)); t++)
      p = Math.max(p, Math.abs(data[t]));
    out.push(p);
    max = Math.max(max, p);
  }
  return out.map((p) => Number(Math.min(1, p / (scale ?? (max || 1))).toFixed(3)));
}

const EDITOR_SECONDS = 1.6;
const editorScale = Math.max(...Array.from({ length: edits.length + 1 }, (_, i) => sample(files.snare, i).reduce((m, v) => Math.max(m, Math.abs(v)), 0)));

const analysis = {
  master: levels(shaped, 1),
  voices: perVoice.map((v) => levels(v, drive)),
  waveforms: Object.fromEntries(
    [...voices.map((v) => v.file), files.kickPunch, ...auditions.map((a) => a.file!)].map((f) => [f, waveform(sample(f))]),
  ),
  /** The snare after 0…5 edits, on one time and level scale. */
  editor: Array.from({ length: edits.length + 1 }, (_, i) => waveform(sample(files.snare, i), 160, EDITOR_SECONDS, editorScale)),
};
mkdirSync(join(video, "src", "generated"), { recursive: true });
writeFileSync(join(video, "src", "generated", "analysis.json"), JSON.stringify(analysis));
console.log(
  `soundtrack.wav: ${(length / SAMPLE_RATE).toFixed(2)} s, ${all.length} hits, peak ${peak.toFixed(2)} before limiting`,
);
