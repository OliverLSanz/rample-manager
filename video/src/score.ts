// The musical score of the video. The soundtrack renderer (scripts/render-audio.ts)
// and the scenes both read it, so every pad, glow and cut lands on the beat.

export const FPS = 60;
export const BPM = 110;
export const SAMPLE_RATE = 44100;
/** Length of one 1/16 step in seconds. */
export const STEP_SECONDS = 60 / BPM / 4;

/** Scene boundaries, in steps from the start of the video (16 steps = one bar). */
export const SCENES = {
  chaos: { from: 0, to: 32 },
  title: { from: 32, to: 64 },
  library: { from: 64, to: 128 },
  sequencer: { from: 128, to: 224 },
  editor: { from: 224, to: 272 },
  build: { from: 272, to: 336 },
  outro: { from: 336, to: 368 },
} as const;
export const END_STEP = SCENES.outro.to;

export const stepToSeconds = (step: number) => step * STEP_SECONDS;
export const stepToFrame = (step: number) => step * STEP_SECONDS * FPS;
export const frameToStep = (frame: number) => frame / FPS / STEP_SECONDS;
export const DURATION_IN_FRAMES = Math.ceil(stepToFrame(END_STEP));

/** Sample paths are relative to this git-ignored folder at the repository root. */
export const SAMPLES_FOLDER = "Generated Samples";
const D0 = "Breakdance/SD Kits/D0/";
const C0 = "Industrial/SD Kits/C0/";

export const files = {
  kickBoom: D0 + "2_01_drum-01-kick-boom.wav",
  kickPunch: D0 + "2_02_drum-02-kick-punch.wav",
  snare: D0 + "2_03_drum-03-snare-crack.wav",
  rim: D0 + "2_06_drum-06-rim.wav",
  hat: D0 + "2_07_drum-07-hat-closed.wav",
  crash: D0 + "2_12_drum-12-crash.wav",
  bass: D0 + "3_01_bass-01-finger-C2.wav",
  orchHit: D0 + "4_04_color-04-orch-hit-C.wav",
  rewind: D0 + "4_11_color-11-rewind-110.wav",
};

/** Length of the rewind that leads into the title, in steps. */
export const REWIND_STEPS = 1.886009 / STEP_SECONDS;

// ── Library: browsing kicks with the arrow keys, each one heard on the beat ──

export interface LibraryRow {
  name: string;
  tags: string;
  seconds: number;
  file?: string;
}
/** The kicks the search finds, auditioned one per beat; the last is added to Voice 1. */
export const auditions: LibraryRow[] = [
  { name: "Kick 01 hardclip C", tags: "#kick #industrial", seconds: 0.45, file: C0 + "1_01_kick-01-hardclip-C.wav" },
  { name: "Kick 04 fuzz C", tags: "#kick #industrial", seconds: 0.37, file: C0 + "1_04_kick-04-fuzz-C.wav" },
  { name: "Pulse 02 felt kick C2", tags: "#kick #ambient", seconds: 0.62, file: "Ambient/SD Kits/B0/3_02_pulse-02-felt-kick-C2.wav" },
  { name: "Kick 07 metal C", tags: "#kick #industrial", seconds: 0.47, file: C0 + "1_07_kick-07-metal-C.wav" },
  { name: "Kick 09 pounder G", tags: "#kick #industrial", seconds: 0.8, file: C0 + "1_09_kick-09-pounder-G.wav" },
  { name: "Kick 06 folded C", tags: "#kick #industrial", seconds: 0.5, file: C0 + "1_06_kick-06-folded-C.wav" },
  { name: "Kick 02 punch", tags: "#kick #breakdance", seconds: 0.55, file: files.kickPunch },
  { name: "Kick 01 boom", tags: "#kick #breakdance", seconds: 0.63, file: files.kickBoom },
];
/** Step of each audition: one per beat over the second half of the Library scene. */
export const auditionAt = (i: number) => SCENES.library.from + 32 + i * 4;

// ── Sequencer: the kit in context ──

export interface SequencerVoice {
  name: string;
  file: string;
  /** Steps (0–15) the voice plays in each bar. */
  steps: number[];
  velocity: (step: number) => number;
  /** Absolute step where the voice starts playing in the sequencer scene. */
  enters: number;
}

const seq = SCENES.sequencer.from;
/** Voice 1 switches from Layer 01 (boom) to Layer 02 (punch) here. */
export const LAYER_SWAP = seq + 64;

export const voices: SequencerVoice[] = [
  { name: "Kick 01 boom", file: files.kickBoom, steps: [0, 7, 10], velocity: () => 120, enters: seq },
  { name: "Snare 03 crack", file: files.snare, steps: [4, 12], velocity: () => 118, enters: seq + 16 },
  {
    name: "Hat 07 closed",
    file: files.hat,
    steps: [0, 2, 4, 6, 8, 10, 12, 14, 15],
    velocity: (s) => (s % 4 === 0 ? 104 : s === 15 ? 60 : 78),
    enters: seq + 32,
  },
  {
    name: "Bass 01 finger C2",
    file: files.bass,
    steps: [0, 3, 10, 14],
    velocity: (s) => (s === 0 ? 120 : 100),
    enters: seq + 48,
  },
];

/** When each programmed step appears on screen (and from when it sounds). */
export interface Write {
  voice: number;
  step: number;
  at: number;
}
export function writes(): Write[] {
  return voices.flatMap((voice, index) => {
    const start = index === 0 ? voice.enters : voice.enters - 4;
    const spacing = Math.min(0.75, 4 / voice.steps.length);
    return voice.steps.map((step, i) => ({ voice: index, step, at: start + i * spacing }));
  });
}

// ── Sample editor: the snare gains one edit per beat, heard as it changes ──

export const edits = ["Trim.", "Pitch.", "EQ.", "Echo.", "Reverb."] as const;
/** Step where edit `i` is applied (and the snare is auditioned with it). */
export const editAt = (i: number) => SCENES.editor.from + i * 4;
/** Number of edits applied at a step (0 before the editor). */
export const editStage = (step: number) =>
  step < SCENES.editor.from ? 0 : Math.min(edits.length, Math.floor((step - SCENES.editor.from) / 4) + 1);

// ── Build ──

export const BUILD_APPLY = SCENES.build.from + 20;
export const BUILD_DONE = SCENES.build.from + 44;

export interface Hit {
  /** Absolute step (may be fractional for the off-grid hits of the opening). */
  at: number;
  file: string;
  gain: number;
  /** Hits on the same voice cut each other, like the Rample's monophonic voices. */
  voice: number | null;
  /** Number of sample-editor edits applied to the snare. */
  edit?: number;
}

/** Deterministic pseudo-random numbers so every render is identical. */
export function random(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

/** Off-grid clicks of the opening: a folder full of sounds, none in time. */
export function chaosClicks() {
  const rand = random(7);
  const clicks: number[] = [];
  let at = 1.5;
  while (at < SCENES.title.from - 15) {
    clicks.push(at);
    at += 0.6 + rand() * 1.9;
  }
  return clicks;
}

/** Voice 5 in the mix is the Library's audition player, also monophonic. */
const AUDITION = 4;

export function hits(): Hit[] {
  const out: Hit[] = [];
  const fx = (at: number, file: string, gain: number) => out.push({ at, file, gain, voice: null });

  // Opening: scattered rim clicks, then a rewind that lands on the title.
  chaosClicks().forEach((at, i) => fx(at, i % 3 === 2 ? files.hat : files.rim, 0.32 + (i % 2) * 0.12));
  fx(SCENES.title.from - REWIND_STEPS, files.rewind, 0.55);

  // Title: impact, then a four-on-the-floor heartbeat that carries into the Library.
  fx(SCENES.title.from, files.crash, 0.55);
  fx(SCENES.title.from, files.orchHit, 0.6);
  for (let at = SCENES.title.from; at < auditionAt(0); at += 4)
    out.push({ at, file: files.kickBoom, gain: at === SCENES.title.from ? 1 : 0.85, voice: 0 });

  // Library: soft hats join, then each kick is heard as the selection reaches it.
  for (let at = SCENES.library.from + 16; at < SCENES.library.to; at += 2)
    out.push({ at, file: files.hat, gain: at % 4 === 0 ? 0.42 : 0.3, voice: 2 });
  auditions.forEach((row, i) => out.push({ at: auditionAt(i), file: row.file!, gain: 0.95, voice: AUDITION }));

  // Sequencer to the end: every step sounds from the moment it is written.
  const written = writes();
  for (let at = SCENES.sequencer.from; at < SCENES.outro.from; at++) {
    const step = (at - SCENES.sequencer.from) % 16;
    voices.forEach((voice, index) => {
      const w = written.find((w) => w.voice === index && w.step === step);
      if (!w || w.at > at) return;
      const file = index === 0 && at >= LAYER_SWAP ? files.kickPunch : voice.file;
      const stage = editStage(at);
      // In the editor the snare is auditioned on every edit instead of its pattern.
      if (index === 1 && at >= SCENES.editor.from && at < editAt(edits.length)) return;
      out.push({ at, file, gain: voice.velocity(step) / 127, voice: index, edit: index === 1 ? stage : undefined });
    });
  }
  edits.forEach((_, i) => out.push({ at: editAt(i), file: files.snare, gain: 0.93, voice: 1, edit: i + 1 }));

  // Build complete.
  fx(BUILD_DONE, files.crash, 0.45);

  // Ending: one last hit under the outro card.
  fx(SCENES.outro.from, files.crash, 0.6);
  fx(SCENES.outro.from, files.orchHit, 0.7);
  out.push({ at: SCENES.outro.from, file: files.kickPunch, gain: 1, voice: 0 });
  out.push({ at: SCENES.outro.from, file: files.bass, gain: 0.95, voice: 3 });
  return out.sort((a, b) => a.at - b.at);
}
