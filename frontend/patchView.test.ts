import { expect, it } from "vitest";
import {
  heardLayer,
  kitIssues,
  nextFreeAddress,
  patchIssues,
  sortedKits,
} from "./patchView";
import type { Patch, PatchKit, PatchSnapshot } from "./types";

function kit(bank: string, number: number): PatchKit {
  return { id: `${bank}${number}`, bank, number, voices: [] };
}
function patch(name: string, kits: PatchKit[]): Patch {
  return { id: name, name, selectedKit: "", kits, bpm: 120, voices: [] };
}

it("sorts Kits by Bank letter, then numerically", () => {
  expect(
    sortedKits(patch("P", [kit("B", 2), kit("A", 10), kit("A", 9)])).map(
      (k) => k.id,
    ),
  ).toEqual(["A9", "A10", "B2"]);
});

it("suggests the first free address, starting in the given Bank and wrapping", () => {
  const full = Array.from({ length: 100 }, (_, n) => kit("Z", n));
  expect(nextFreeAddress(patch("P", [kit("A", 0), kit("A", 2)]))).toEqual({
    bank: "A",
    number: 1,
  });
  expect(nextFreeAddress(patch("P", full), "Z")).toEqual({
    bank: "A",
    number: 0,
  });
  expect(nextFreeAddress(undefined)).toEqual({ bank: "A", number: 0 });
});

it("keeps only the open Patch's issues, without similarly named Patches", () => {
  const view = {
    validation: [
      "Set · A0: Voice 1 needs a Sample.",
      "Set: Layer modes were not read from this SD Import.",
      "Set 2 · A0: Voice 1 needs a Sample.",
      "Set 2: Layer modes were not read from this SD Import.",
      "Voice 3, Layer 1: Original changed.",
    ],
  } as PatchSnapshot;
  const issues = patchIssues(view, patch("Set", []));
  expect(issues).toEqual([
    "A0: Voice 1 needs a Sample.",
    "Layer modes were not read from this SD Import.",
    "Voice 3, Layer 1: Original changed.",
  ]);
  expect(kitIssues(issues, kit("A", 0))).toEqual(["Voice 1 needs a Sample."]);
  expect(kitIssues(issues, kit("A", 1))).toEqual([]);
});

it("hears the selected Layer in Single sample mode and the triggered one otherwise", () => {
  const layers = [
    { id: "one", sampleId: "a" },
    { id: "two", sampleId: "b" },
  ];
  expect(
    heardLayer({ layers, selectedLayer: "two", mode: "manual" }, 0)?.id,
  ).toBe("two");
  expect(
    heardLayer({ layers, selectedLayer: "two", mode: "cyclic" }, 0)?.id,
  ).toBe("one");
  expect(
    heardLayer({ layers, selectedLayer: "two", mode: "random" }, -1)?.id,
  ).toBe("two");
  expect(
    heardLayer({ layers: [], selectedLayer: "", mode: "manual" }, 0),
  ).toBeUndefined();
});
