export type LayerMode =
  "manual" | "random" | "cyclic" | "reverseCyclic" | "velocity";
export interface LibrarySample {
  id: string;
  originalId: string;
  name: string;
  sha256: string;
  duration: number;
  rate: number;
  channels: number;
  bits: number;
  readable: boolean;
  error: string | null;
}
export interface LibraryImportReply {
  imported: number;
  recognized: number;
  failed: number;
  complete: boolean;
  samples: LibrarySample[];
  results: {
    source: string;
    status: "imported" | "recognized" | "failed";
    sample: LibrarySample | null;
    error: string | null;
  }[];
}
export interface Voice {
  kitID: string;
  samplePath: string;
  steps: boolean[];
  volume: number;
  muted: boolean;
  solo: boolean;
  layerMode: LayerMode;
  velocities: number[];
}
export interface Session {
  schemaVersion: number;
  rootPath: string;
  selectedKit: string;
  bpm: number;
  voices: Voice[];
  kitConfigurations: {
    libraries: Record<
      string,
      Record<
        string,
        Record<string, { sampleName: string; layerMode: LayerMode }>
      >
    >;
  };
}
export interface Sample {
  id: string;
  name: string;
  voice: number;
  revision: string;
  duration: number;
  rate: number;
  channels: number;
  bits: number;
  warnings: string[];
  readable: boolean;
  needsConversion: boolean;
}
export interface Kit {
  id: string;
  bank: string;
  path: string;
  samples: Sample[];
  warnings: string[];
}
export interface ReviewedFile {
  id: string;
  fingerprint: string;
}
export interface ImportCandidate extends ReviewedFile {
  name: string;
  kit: string;
  issues: string[];
  convertible: boolean;
  target: string;
}
export interface ConversionResult {
  name: string;
  target: string | null;
  backup: string | null;
  error: string | null;
}
export interface ConversionReply {
  snapshot: Snapshot;
  results: ConversionResult[];
  backupPath: string;
}
export interface Snapshot {
  revision: number;
  session: Session;
  catalog: { kits: Kit[]; error: string | null; unassigned: Sample[] };
  waveforms: Record<string, number[]>;
  banks: string[][];
  isDemo: boolean;
  watching: boolean;
  supportPath: string;
  message: string | null;
  playing: boolean;
  imports: ImportCandidate[];
  editorInvalidated?: boolean;
}
export interface Transport {
  running: boolean;
  step: number;
  peak: number;
  editorPeak?: number;
  activeLayers: number[];
  underruns: number;
  renderedFrames: number;
  sampleRate: number;
  error: string | null;
}
export type Action =
  | { type: "root"; path: string }
  | {
      type:
        | "demo"
        | "basicPattern"
        | "clearPattern"
        | "refresh"
        | "reviewFormats"
        | "togglePlayback"
        | "stop";
    }
  | { type: "selectKit"; kit: string }
  | { type: "selectSource"; channel: number; kit: string }
  | { type: "selectSample"; channel: number; path: string }
  | { type: "setMode"; channel: number; mode: LayerMode }
  | { type: "toggleStep"; channel: number; step: number }
  | { type: "setVolume"; channel: number; value: number }
  | { type: "toggleMute" | "toggleSolo" | "audition"; channel: number }
  | { type: "setVelocity"; channel: number; step: number; value: number }
  | { type: "setBpm"; value: number }
  | { type: "adjustBpm"; delta: number }
  | { type: "dismissImports"; files: ReviewedFile[] };
export const modes: { id: LayerMode; title: string; description: string }[] = [
  {
    id: "manual",
    title: "Single sample",
    description: "MANUAL: trigger only the selected sample.",
  },
  {
    id: "random",
    title: "Random",
    description: "RANDOM: choose a layer per trigger. Repeats are possible.",
  },
  {
    id: "cyclic",
    title: "Sequential",
    description: "CYCLIC: advance through layers in filename order, then wrap.",
  },
  {
    id: "reverseCyclic",
    title: "Reverse sequential",
    description:
      "REVERSE CYCLIC: move backwards through the layers, then wrap.",
  },
  {
    id: "velocity",
    title: "Velocity",
    description:
      "VELOCITY: select a layer using the step velocity. Alt/Option-click a step to edit.",
  },
];
export const idleTransport: Transport = {
  running: false,
  step: -1,
  peak: 0,
  activeLayers: [-1, -1, -1, -1],
  underruns: 0,
  renderedFrames: 0,
  sampleRate: 0,
  error: null,
};
export function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "No folder";
}
export function sourceSamples(snapshot: Snapshot, channel: number) {
  const v = snapshot.session.voices[channel];
  return (
    snapshot.catalog.kits
      .find((k) => k.id === v.kitID)
      ?.samples.filter((s) => s.voice === channel + 1) ?? []
  );
}
export function displaySample(
  snapshot: Snapshot,
  channel: number,
  active: number,
) {
  const samples = sourceSamples(snapshot, channel);
  const voice = snapshot.session.voices[channel];
  const path =
    voice.layerMode === "manual"
      ? voice.samplePath
      : (snapshot.banks[channel]?.[active] ?? voice.samplePath);
  return samples.find((s) => s.id === path);
}
export function compatibilityWarnings(snapshot: Snapshot) {
  const kits = new Set(snapshot.session.voices.map((v) => v.kitID));
  return [
    ...new Set([
      ...snapshot.catalog.kits
        .filter((k) => kits.has(k.id))
        .flatMap((k) => k.warnings),
      ...snapshot.session.voices.flatMap((v, i) =>
        sourceSamples(snapshot, i)
          .filter(
            (s) =>
              s.id === v.samplePath ||
              (v.layerMode !== "manual" && snapshot.banks[i]?.includes(s.id)),
          )
          .flatMap((s) => s.warnings),
      ),
    ]),
  ];
}

export interface EditSettings {
  start: number;
  end: number;
  gainDb: number;
  pitch: number;
  lowDb: number;
  midDb: number;
  highDb: number;
  echoMix: number;
  echoMs: number;
  echoFeedback: number;
  reverbMix: number;
  reverbSeconds: number;
}
export interface EditorInfo {
  id: string;
  channel: number;
  path: string;
  fingerprint: string;
  duration: number;
  waveform: number[];
  settings: EditSettings;
}
export interface EditorSaveReply {
  snapshot: Snapshot;
  editor: EditorInfo;
  backup: string;
  rendered: {
    frames: number;
    duration: number;
    peak: number;
    clippedFrames: number;
  };
}
