// Pure view-model helpers for the Patch workspace. They read a PatchSnapshot
// and never talk to the native backend.
import type { Patch, PatchKit, PatchSnapshot, PatchVoice } from "./types";

export const banks = Array.from({ length: 26 }, (_, i) =>
  String.fromCharCode(65 + i),
);

export function currentPatch(view: PatchSnapshot | null): Patch | undefined {
  return view?.graph.patches.find((p) => p.id === view.graph.selectedPatch);
}

export function currentKit(patch: Patch | undefined): PatchKit | undefined {
  return patch?.kits.find((k) => k.id === patch.selectedKit);
}

export function kitAddress(kit: Pick<PatchKit, "bank" | "number">) {
  return `${kit.bank}${kit.number}`;
}

/** Kits in Rample order: Bank letter, then Kit number. */
export function sortedKits(patch: Patch | undefined): PatchKit[] {
  return [...(patch?.kits ?? [])].sort(
    (a, b) => a.bank.localeCompare(b.bank) || a.number - b.number,
  );
}

export function layerCount(kit: PatchKit) {
  return kit.voices.reduce((sum, voice) => sum + voice.layers.length, 0);
}

/** The Kit whose Voice plays on this channel; usually the selected Kit. */
export function voiceSource(
  patch: Patch,
  kit: PatchKit,
  channel: number,
): { kit: PatchKit; voice: PatchVoice } {
  const id = patch.voiceKits?.[channel];
  const source = (id && patch.kits.find((k) => k.id === id)) || kit;
  return { kit: source, voice: source.voices[channel] };
}

/** The first free address in a Bank, then in the following Banks. */
export function nextFreeAddress(
  patch: Patch | undefined,
  bank = "A",
): { bank: string; number: number } {
  const used = new Set(patch?.kits.map(kitAddress));
  const start = Math.max(0, banks.indexOf(bank));
  for (const letter of [...banks.slice(start), ...banks.slice(0, start)])
    for (let number = 0; number < 100; number++)
      if (!used.has(`${letter}${number}`)) return { bank: letter, number };
  return { bank, number: 0 };
}

/**
 * Validation messages for the current Patch. Native issues are prefixed with
 * `<Patch>:`, Kit issues with `<Patch> · <Kit>:` and preview errors name only
 * the Voice and Layer, because they always belong to the playing Patch.
 */
export function patchIssues(view: PatchSnapshot, patch: Patch): string[] {
  return view.validation.flatMap((message) => {
    for (const prefix of [`${patch.name} · `, `${patch.name}: `])
      if (message.startsWith(prefix)) return [message.slice(prefix.length)];
    return /^Voice \d+, Layer \d+:/.test(message) ? [message] : [];
  });
}

export function kitIssues(issues: string[], kit: PatchKit): string[] {
  const prefix = `${kitAddress(kit)}: `;
  return issues
    .filter((issue) => issue.startsWith(prefix))
    .map((issue) => issue.slice(prefix.length));
}

/**
 * The Layer heard on a Voice: the selected Layer in Single sample mode,
 * otherwise the Layer the sequencer last triggered.
 */
export function heardLayer(voice: PatchVoice, active: number) {
  if (voice.mode !== "manual" && voice.layers[active])
    return voice.layers[active];
  return (
    voice.layers.find((l) => l.id === voice.selectedLayer) ?? voice.layers[0]
  );
}

/** One Layer that plays a Sample. */
export interface Usage {
  patch: string;
  patchName: string;
  kit: string;
  address: string;
  channel: number;
  /** One-based position in the Voice. */
  position: number;
}

/** Every Layer of every Patch, by Sample ID, in Patch and Rample Kit order. */
export function sampleUsage(graph: PatchSnapshot["graph"]) {
  const usage = new Map<string, Usage[]>();
  for (const patch of graph.patches)
    for (const kit of sortedKits(patch))
      kit.voices.forEach((voice, channel) =>
        voice.layers.forEach((layer, index) => {
          const list = usage.get(layer.sampleId) ?? [];
          list.push({
            patch: patch.id,
            patchName: patch.name,
            kit: kit.id,
            address: kitAddress(kit),
            channel,
            position: index + 1,
          });
          usage.set(layer.sampleId, list);
        }),
      );
  return usage;
}
