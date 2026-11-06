import { Easing, interpolate, useCurrentFrame } from "remotion";
import analysis from "./generated/analysis.json";
import { frameToStep } from "./score";

/** The current position in 1/16 steps from the start of the video. */
export function useStep() {
  return frameToStep(useCurrentFrame());
}

export function useLevels() {
  const frame = useCurrentFrame();
  const at = (list: number[]) => list[Math.min(list.length - 1, Math.max(0, frame))] ?? 0;
  return {
    master: at(analysis.master),
    voice: (i: number) => at(analysis.voices[i]),
  };
}

export const waveforms = analysis.waveforms as Record<string, number[]>;

/** 0 → 1 between two step positions, eased. */
export function ramp(step: number, from: number, to: number, easing = Easing.out(Easing.cubic)) {
  return interpolate(step, [from, to], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing,
  });
}

/** The snare's waveform after 0…5 sample-editor edits, on one scale. */
export const editorWaveforms = analysis.editor as number[][];
