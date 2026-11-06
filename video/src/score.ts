// The musical score of the video. The soundtrack renderer (scripts/render-audio.ts)
// and the scenes both read it, so every pad, glow and cut lands on the beat.

export const FPS = 60;
export const BPM = 110;
export const SAMPLE_RATE = 44100;
/** Length of one 1/16 step in seconds. */
export const STEP_SECONDS = 60 / BPM / 4;

/** Scene boundaries, in steps from the start of the video. */
export const SCENES = {
  chaos: { from: 0, to: 32 },
  title: { from: 32, to: 64 },
  sequencer: { from: 64, to: 160 },
  outro: { from: 160, to: 192 },
} as const;
export const END_STEP = SCENES.outro.to;

export const stepToSeconds = (step: number) => step * STEP_SECONDS;
export const stepToFrame = (step: number) => step * STEP_SECONDS * FPS;
export const frameToStep = (frame: number) => frame / FPS / STEP_SECONDS;
export const DURATION_IN_FRAMES = Math.ceil(stepToFrame(END_STEP));

/** Folder of the Breakdance kit, relative to the repository root. */
/** Length of the rewind that leads into the title, in steps. */
export const REWIND_STEPS = 1.886009 / STEP_SECONDS;

export const KIT_FOLDER = "Generated Samples/Breakdance/SD Kits/D0";

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

/** The four voices programmed in the sequencer scene, one per bar. */
export const voices: SequencerVoice[] = [
  {
    name: "Kick 01 boom",
    file: "2_01_drum-01-kick-boom.wav",
    steps: [0, 7, 10],
    velocity: () => 120,
    enters: seq,
  },
  {
    name: "Snare 03 crack",
    file: "2_03_drum-03-snare-crack.wav",
    steps: [4, 12],
    velocity: () => 118,
    enters: seq + 16,
  },
  {
    name: "Hat 07 closed",
    file: "2_07_drum-07-hat-closed.wav",
    steps: [0, 2, 4, 6, 8, 10, 12, 14, 15],
    velocity: (s) => (s % 4 === 0 ? 104 : s === 15 ? 60 : 78),
    enters: seq + 32,
  },
  {
    name: "Bass 01 finger C2",
    file: "3_01_bass-01-finger-C2.wav",
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

export interface Hit {
  /** Absolute step (may be fractional for the off-grid hits of the opening). */
  at: number;
  file: string;
  gain: number;
  /** Hits on the same voice cut each other, like the Rample's monophonic voices. */
  voice: number | null;
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

export function hits(): Hit[] {
  const out: Hit[] = [];
  const kick = voices[0].file;
  const fx = (at: number, file: string, gain: number) =>
    out.push({ at, file, gain, voice: null });

  // Opening: scattered rim clicks, then a rewind that lands on the title.
  chaosClicks().forEach((at, i) =>
    fx(at, i % 3 === 2 ? "2_07_drum-07-hat-closed.wav" : "2_06_drum-06-rim.wav", 0.32 + (i % 2) * 0.12),
  );
  fx(SCENES.title.from - REWIND_STEPS, "4_11_color-11-rewind-110.wav", 0.55);

  // Title: impact, then a four-on-the-floor heartbeat.
  fx(SCENES.title.from, "2_12_drum-12-crash.wav", 0.55);
  fx(SCENES.title.from, "4_04_color-04-orch-hit-C.wav", 0.6);
  for (let at = SCENES.title.from; at < SCENES.title.to; at += 4)
    out.push({ at, file: kick, gain: at === SCENES.title.from ? 1 : 0.85, voice: 0 });

  // Sequencer: every step sounds from the moment it is written.
  const written = writes();
  for (let at = SCENES.sequencer.from; at < SCENES.sequencer.to; at++) {
    const step = (at - SCENES.sequencer.from) % 16;
    voices.forEach((voice, index) => {
      const w = written.find((w) => w.voice === index && w.step === step);
      if (w && w.at <= at) out.push({ at, file: voice.file, gain: voice.velocity(step) / 127, voice: index });
    });
  }

  // Ending: one last hit under the outro card.
  fx(SCENES.outro.from, "2_12_drum-12-crash.wav", 0.6);
  fx(SCENES.outro.from, "4_04_color-04-orch-hit-C.wav", 0.7);
  out.push({ at: SCENES.outro.from, file: kick, gain: 1, voice: 0 });
  out.push({ at: SCENES.outro.from, file: voices[3].file, gain: 0.95, voice: 3 });
  return out.sort((a, b) => a.at - b.at);
}
