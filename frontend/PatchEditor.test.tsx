import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { PatchEditor } from "./PatchEditor";
import * as native from "./native";
import { idleTransport } from "./types";
vi.mock("./native", () => ({
  libraryRecoverySnapshots: vi.fn(),
  chooseFolder: vi.fn(),
  restoreLibrary: vi.fn(),
  patchAction: vi.fn(),
  closePatch: vi.fn(),
  listSamples: vi.fn(),
  querySamples: vi.fn(),
  openSampleAudition: vi.fn(),
  stopSampleAudition: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
const sample = {
  id: "sample",
  originalId: "original",
  name: "Warm kick",
  tags: ["round"],
  sha256: "hash",
  duration: 0.1,
  rate: 44100,
  bits: 16,
  channels: 1,
  format: "WAV",
  retired: false,
  readable: true,
  error: null,
};
function view() {
  const kitVoices = Array.from({ length: 4 }, () => ({
    layers: [] as { id: string; sampleId: string }[],
    selectedLayer: "",
    mode: "manual" as const,
  }));
  kitVoices[0] = {
    layers: [
      { id: "occ-first", sampleId: "sample" },
      { id: "occ-second", sampleId: "sample" },
    ],
    selectedLayer: "occ-second",
    mode: "manual",
  };
  const voices = kitVoices.map((v) => ({
    kitID: "kit",
    samplePath: v.selectedLayer,
    steps: Array(16).fill(false) as boolean[],
    volume: 0.8,
    muted: false,
    solo: false,
    layerMode: v.mode,
    velocities: Array(16).fill(100) as number[],
  }));
  return {
    graph: {
      selectedPatch: "patch",
      patches: [
        {
          id: "patch",
          name: "Live set",
          selectedKit: "kit",
          kits: [{ id: "kit", bank: "A", number: 0, voices: kitVoices }],
          bpm: 120,
          voices,
        },
      ],
    },
    session: {
      schemaVersion: 2,
      rootPath: "",
      selectedKit: "kit",
      bpm: 120,
      voices,
      kitConfigurations: { libraries: {} },
    },
    samples: [sample],
    validation: ["Live set · B12: Voice 1 needs a Sample."],
    playing: false,
  };
}
it("shows distinct repeated Layers, selected identity, modes, validation and ordered native commands", async () => {
  const initial = view();
  const reordered = structuredClone(initial);
  reordered.graph.patches[0].kits[0].voices[0].layers.reverse();
  vi.mocked(native.patchAction)
    .mockResolvedValueOnce(initial)
    .mockResolvedValueOnce(reordered)
    .mockResolvedValue(reordered);
  render(<PatchEditor transport={idleTransport} onClose={() => {}} />);
  const voice = await screen.findByRole("region", { name: "Patch Voice 1" });
  expect(
    within(voice)
      .getByRole("button", { name: "Select Layer 2: Warm kick" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  fireEvent.click(
    within(voice).getByRole("button", { name: "Move Layer 2 up" }),
  );
  await screen.findByRole("button", {
    name: "Select Layer 1: Warm kick",
    pressed: true,
  });
  expect(native.patchAction).toHaveBeenLastCalledWith({
    type: "moveLayer",
    channel: 0,
    layer: "occ-second",
    to: 0,
  });
  expect(
    screen.getByText("Live set · B12: Voice 1 needs a Sample."),
  ).toBeTruthy();
  expect(
    within(voice).getByRole("combobox", { name: "Layer mode for Voice 1" })
      .children.length,
  ).toBe(5);
  fireEvent.change(
    within(voice).getByRole("combobox", { name: "Layer mode for Voice 1" }),
    { target: { value: "cyclic" } },
  );
  await vi.waitFor(() =>
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "musical",
      action: { type: "setMode", channel: 0, mode: "cyclic" },
    }),
  );
  expect(
    screen.getAllByRole("button", { name: /Voice \d, step / }),
  ).toHaveLength(64);
});
it("adds another occurrence from the searchable Library picker and restores the folder workspace on close", async () => {
  const initial = view();
  vi.mocked(native.patchAction).mockResolvedValue(initial);
  vi.mocked(native.openSampleAudition).mockResolvedValue("picker");
  vi.mocked(native.listSamples).mockResolvedValue([sample]);
  vi.mocked(native.querySamples).mockResolvedValue([sample]);
  vi.mocked(native.closePatch).mockResolvedValue(undefined);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  const close = vi.fn();
  render(<PatchEditor transport={idleTransport} onClose={close} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Add Sample to Voice 1" }),
  );
  fireEvent.change(await screen.findByRole("searchbox"), {
    target: { value: "round" },
  });
  fireEvent.click(
    await screen.findByRole("button", { name: "Select Warm kick" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Use Sample" }));
  await screen.findByRole("region", { name: "Patch Voice 1" });
  expect(native.patchAction).toHaveBeenLastCalledWith({
    type: "addLayer",
    channel: 0,
    sample: "sample",
  });
  fireEvent.click(screen.getByRole("button", { name: "Back to folders" }));
  await vi.waitFor(() => expect(close).toHaveBeenCalled());
  expect(native.closePatch).toHaveBeenCalled();
});

it("reports a failed picker add and prevents a second add while publication is pending", async () => {
  vi.mocked(native.patchAction).mockResolvedValueOnce(view());
  let rejectAdd: (error: Error) => void = () => {};
  vi.mocked(native.openSampleAudition).mockResolvedValue("picker");
  vi.mocked(native.listSamples).mockResolvedValue([sample]);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  render(<PatchEditor transport={idleTransport} onClose={() => {}} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Add Sample to Voice 1" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Select Warm kick" }),
  );
  vi.mocked(native.patchAction).mockImplementation(
    () =>
      new Promise((_, reject) => {
        rejectAdd = reject;
      }),
  );
  const use = screen.getByRole("button", { name: "Use Sample" });
  fireEvent.click(use);
  await vi.waitFor(() => expect(use.hasAttribute("disabled")).toBe(true));
  await vi.waitFor(() =>
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "addLayer",
      channel: 0,
      sample: "sample",
    }),
  );
  rejectAdd(new Error("Library disk is full"));
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    "Error: Library disk is full",
  );
});

it("keeps migrated users in the managed workspace and shows each Voice's source Kit", async () => {
  const initial = view();
  const other = structuredClone(initial.graph.patches[0].kits[0]);
  other.id = "other-kit";
  other.bank = "B";
  other.number = 12;
  other.voices[0].selectedLayer = "other-layer";
  other.voices[0].layers = [{ id: "other-layer", sampleId: "sample" }];
  const patch = {
    ...initial.graph.patches[0],
    voiceKits: ["other-kit", "kit", "kit", "kit"],
  };
  patch.kits.push(other);
  initial.graph.patches[0] = patch;
  vi.mocked(native.patchAction).mockResolvedValue(initial);
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  const voice = await screen.findByRole("region", { name: "Patch Voice 1" });
  expect(within(voice).getByText("Source Kit B12")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Back to folders" })).toBeNull();
  expect(
    within(voice)
      .getByRole("button", { name: "Select Layer 1: Warm kick" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
});

it("opens Library restore controls and refreshes Patch state after activation", async () => {
  vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([]);
  const before = view();
  const restored = view();
  restored.graph.patches[0].name = "Restored set";
  vi.mocked(native.patchAction)
    .mockResolvedValueOnce(before)
    .mockResolvedValue(restored);
  vi.mocked(native.chooseFolder).mockResolvedValue("/backup/Library");
  vi.mocked(native.restoreLibrary).mockResolvedValue({
    complete: true,
    activated: true,
    location: "/backup/Library",
    recovery: "/recovery/previous",
    error: null,
    snapshot: null,
  });
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  await screen.findByDisplayValue("Live set");
  fireEvent.click(
    screen.getByRole("button", { name: "Library backup and restore…" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
  await screen.findByRole("button", { name: "Replace Library" });
  fireEvent.click(screen.getByRole("button", { name: "Replace Library" }));
  await screen.findByText("Library restored.");
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  expect(
    await screen.findByRole("option", { name: "Restored set" }),
  ).toBeTruthy();
});
