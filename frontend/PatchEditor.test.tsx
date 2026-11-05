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
import userEvent from "@testing-library/user-event";
vi.mock("./native", () => ({
  libraryRecoverySnapshots: vi.fn(),
  chooseFolder: vi.fn(),
  restoreLibrary: vi.fn(),
  patchAction: vi.fn(),
  reviewPatchBuild: vi.fn(),
  applyPatchBuild: vi.fn(),
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
it("sets the Library default Layer mode used for new and never-stored Kits", async () => {
  const initial = view();
  const updated = structuredClone(initial);
  (updated.graph as { defaultLayerMode?: string }).defaultLayerMode = "random";
  vi.mocked(native.patchAction)
    .mockResolvedValueOnce(initial)
    .mockResolvedValue(updated);
  render(<PatchEditor transport={idleTransport} onClose={() => {}} />);
  const select = await screen.findByRole("combobox", {
    name: /Default Layer mode/,
  });
  expect((select as HTMLSelectElement).value).toBe("manual");
  expect(select.children.length).toBe(5);
  fireEvent.change(select, { target: { value: "random" } });
  await vi.waitFor(() =>
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "setDefaultMode",
      mode: "random",
    }),
  );
  await vi.waitFor(() =>
    expect((select as HTMLSelectElement).value).toBe("random"),
  );
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

it("Space activates a focused Patch button instead of hidden playback", async () => {
  vi.mocked(native.patchAction).mockResolvedValue(view());
  const user = userEvent.setup();
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  const add = await screen.findByRole("button", {
    name: "Add Sample to Voice 1",
  });
  vi.mocked(native.openSampleAudition).mockResolvedValue("picker");
  vi.mocked(native.listSamples).mockResolvedValue([sample]);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  add.focus();
  await user.keyboard(" ");
  expect(await screen.findByRole("searchbox")).toBeTruthy();
  expect(native.patchAction).not.toHaveBeenCalledWith({
    type: "musical",
    action: { type: "togglePlayback" },
  });
});

it("keeps Patch shortcuts suspended throughout Library and snapshot child views", async () => {
  vi.mocked(native.patchAction).mockResolvedValue(view());
  vi.mocked(native.openSampleAudition).mockResolvedValue("library");
  vi.mocked(native.listSamples).mockResolvedValue([sample]);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([
    "/recovery/previous",
  ]);
  const user = userEvent.setup();
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  await user.click(
    await screen.findByRole("button", { name: "Library samples…" }),
  );
  await screen.findByRole("searchbox");
  (document.activeElement as HTMLElement).blur();
  await user.keyboard(" 1");
  expect(native.patchAction).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "Close" }));
  await user.click(
    await screen.findByRole("button", { name: "Library backup and restore…" }),
  );
  const recover = await screen.findByRole("button", {
    name: "Restore this recovery snapshot…",
  });
  (document.activeElement as HTMLElement).blur();
  await user.keyboard(" 1");
  expect(native.patchAction).toHaveBeenCalledTimes(2);
  recover.focus();
  await user.keyboard(" ");
  expect(
    await screen.findByRole("button", { name: "Replace Library" }),
  ).toBeTruthy();
  expect(native.patchAction).not.toHaveBeenCalledWith({
    type: "musical",
    action: { type: "togglePlayback" },
  });
});

const builtFiles = [
  {
    path: "A0/1_01_Warm_kick.wav",
    kit: "A0",
    voice: 1,
    layer: 1,
    sampleId: "sample",
    sampleName: "Warm kick",
    revisionId: "rev",
  },
  {
    path: "A0/1_02_Warm_kick.wav",
    kit: "A0",
    voice: 1,
    layer: 2,
    sampleId: "sample",
    sampleName: "Warm kick",
    revisionId: "rev",
  },
];
function buildReview(
  overrides: Partial<import("./types").BuildReview> = {},
): import("./types").BuildReview {
  return {
    patch: "patch",
    patchName: "Live set",
    destination: "/Volumes/RAMPLE",
    kits: ["A0"],
    files: builtFiles,
    additions: ["A0/1_02_Warm_kick.wav"],
    replacements: ["A0/1_01_Warm_kick.wav"],
    removals: ["A0/1_Old.wav", "C3/1_Old.wav"],
    unchanged: [],
    removedKits: ["C3"],
    conflicts: [],
    notices: ["1 unassigned SD Import occurrence(s) are not part of any Kit."],
    state: [],
    folders: ["A0", "C3"],
    ...overrides,
  };
}
const backup = {
  id: "backup-1",
  created: 1_800_000_000,
  patchName: "Live set",
  destination: "/Volumes/RAMPLE",
  status: "complete" as const,
  error: null,
  restored: null,
  folder: "/Support/Patch build backups/backup-1",
  added: ["A0/1_02_Warm_kick.wav"],
  replaced: ["A0/1_01_Warm_kick.wav"],
  removed: ["A0/1_Old.wav", "C3/1_Old.wav"],
};

it("reviews destination changes before applying a Build and reports its backup", async () => {
  vi.mocked(native.patchAction).mockResolvedValue(view());
  vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
  const review = buildReview();
  vi.mocked(native.reviewPatchBuild).mockResolvedValue(review);
  let finish: (reply: import("./types").BuildReply) => void = () => {};
  vi.mocked(native.applyPatchBuild).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  await screen.findByDisplayValue("Live set");
  fireEvent.click(screen.getByRole("button", { name: "Build Patch…" }));
  const panel = await screen.findByRole("region", {
    name: "Build Patch review",
  });
  expect(native.reviewPatchBuild).toHaveBeenCalledWith(
    "patch",
    "/Volumes/RAMPLE",
  );
  expect(native.applyPatchBuild).not.toHaveBeenCalled();
  expect(
    within(panel).getByText(
      "1 to add, 1 to replace, 2 to remove, 0 already up to date.",
    ),
  ).toBeTruthy();
  expect(
    within(within(panel).getByRole("list", { name: "Files to add" })).getByText(
      "A0/1_02_Warm_kick.wav",
    ),
  ).toBeTruthy();
  expect(
    within(
      within(panel).getByRole("list", { name: "Files to replace" }),
    ).getByText("A0/1_01_Warm_kick.wav"),
  ).toBeTruthy();
  expect(
    within(
      within(panel).getByRole("list", { name: "Files to remove" }),
    ).getByText("C3/1_Old.wav"),
  ).toBeTruthy();
  expect(
    within(panel).getByText("Kits removed from the destination: C3."),
  ).toBeTruthy();
  expect(within(panel).getByText(/backed up on this computer/)).toBeTruthy();
  expect(within(panel).getByText(/_save/)).toBeTruthy();

  fireEvent.click(within(panel).getByRole("button", { name: "Apply Build" }));
  await vi.waitFor(() =>
    expect(native.applyPatchBuild).toHaveBeenCalledWith(review),
  );
  expect(await screen.findByText("Building Patch…")).toBeTruthy();
  finish({
    destination: "/Volumes/RAMPLE",
    kits: ["A0"],
    files: builtFiles,
    notices: ["1 unassigned SD Import occurrence(s) are not part of any Kit."],
    added: 1,
    replaced: 1,
    removed: 2,
    unchanged: 0,
    backup,
    error: null,
  });
  const result = await screen.findByRole("region", {
    name: "Build Patch result",
  });
  expect(
    screen.queryByRole("region", { name: "Build Patch review" }),
  ).toBeNull();
  expect(
    within(result).getByText(
      "Built 1 Kit and 2 audio files into /Volumes/RAMPLE.",
    ),
  ).toBeTruthy();
  expect(
    within(result).getByText(
      "Added 1, replaced 1 and removed 2 files; 0 were already up to date.",
    ),
  ).toBeTruthy();
  expect(
    within(result).getByText(
      "Replaced and removed files are backed up in /Support/Patch build backups/backup-1.",
    ),
  ).toBeTruthy();
  expect(
    within(result).getByText(
      "A0/1_02_Warm_kick.wav · Voice 1, Layer 2: Warm kick",
    ),
  ).toBeTruthy();
  expect(
    within(result).getByText(/set them on the Rample and STORE/),
  ).toBeTruthy();
});

it("blocks a review with conflicts and can cancel without applying", async () => {
  vi.mocked(native.patchAction).mockResolvedValue(view());
  vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
  vi.mocked(native.reviewPatchBuild).mockResolvedValue(
    buildReview({
      conflicts: [
        "a0 has the same name as Kit folder A0 on cards that ignore letter case.",
      ],
    }),
  );
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  await screen.findByDisplayValue("Live set");
  fireEvent.click(screen.getByRole("button", { name: "Build Patch…" }));
  const panel = await screen.findByRole("region", {
    name: "Build Patch review",
  });
  expect(
    within(panel).getByText(
      "a0 has the same name as Kit folder A0 on cards that ignore letter case.",
    ),
  ).toBeTruthy();
  expect(
    within(panel)
      .getByRole("button", { name: "Apply Build" })
      .hasAttribute("disabled"),
  ).toBe(true);
  fireEvent.click(within(panel).getByRole("button", { name: "Cancel" }));
  expect(
    screen.queryByRole("region", { name: "Build Patch review" }),
  ).toBeNull();
  expect(native.applyPatchBuild).not.toHaveBeenCalled();
});

it("reports an incomplete Build with its backup instead of claiming output", async () => {
  vi.mocked(native.patchAction).mockResolvedValue(view());
  vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
  vi.mocked(native.reviewPatchBuild).mockResolvedValue(buildReview());
  const error =
    "Build Patch did not complete: Could not write A0/1_02_Warm_kick.wav. 3 of 4 file changes were applied, so the destination is only partly updated.";
  vi.mocked(native.applyPatchBuild).mockResolvedValue({
    destination: "/Volumes/RAMPLE",
    kits: ["A0"],
    files: builtFiles,
    notices: [],
    added: 1,
    replaced: 1,
    removed: 2,
    unchanged: 0,
    backup: { ...backup, status: "incomplete", error },
    error,
  });
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  await screen.findByDisplayValue("Live set");
  fireEvent.click(screen.getByRole("button", { name: "Build Patch…" }));
  const panel = await screen.findByRole("region", {
    name: "Build Patch review",
  });
  fireEvent.click(within(panel).getByRole("button", { name: "Apply Build" }));
  const result = await screen.findByRole("region", {
    name: "Build Patch result",
  });
  expect(within(result).getByText(error)).toBeTruthy();
  expect(within(result).queryByText(/^Built /)).toBeNull();
  expect(
    within(result).getByText(
      "Replaced and removed files are backed up in /Support/Patch build backups/backup-1.",
    ),
  ).toBeTruthy();
});

it("reports a blocked review without claiming output and ignores a cancelled folder choice", async () => {
  vi.mocked(native.patchAction).mockResolvedValue(view());
  vi.mocked(native.chooseFolder).mockResolvedValueOnce(null);
  render(<PatchEditor managed transport={idleTransport} onClose={() => {}} />);
  await screen.findByDisplayValue("Live set");
  fireEvent.click(screen.getByRole("button", { name: "Build Patch…" }));
  await vi.waitFor(() => expect(native.chooseFolder).toHaveBeenCalled());
  expect(native.reviewPatchBuild).not.toHaveBeenCalled();
  vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
  vi.mocked(native.reviewPatchBuild).mockRejectedValue(
    "Build Patch is blocked until the Patch is valid. Nothing was written.\nB12: Voice 1 needs a Sample.",
  );
  fireEvent.click(screen.getByRole("button", { name: "Build Patch…" }));
  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText("B12: Voice 1 needs a Sample.")).toBeTruthy();
  expect(
    within(alert).getByText(
      "Build Patch is blocked until the Patch is valid. Nothing was written.",
    ),
  ).toBeTruthy();
  expect(
    screen.queryByRole("region", { name: "Build Patch result" }),
  ).toBeNull();
  expect(
    screen.queryByRole("region", { name: "Build Patch review" }),
  ).toBeNull();
});
