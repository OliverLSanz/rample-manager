import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import * as native from "./native";
import {
  idleTransport,
  type BuildReply,
  type BuildReview,
  type Patch,
  type PatchKit,
  type PatchSnapshot,
  type Snapshot,
} from "./types";

vi.mock("./native", () => ({
  getSnapshot: vi.fn(),
  getTransport: vi.fn(),
  patchAction: vi.fn(),
  migrationStatus: vi.fn(),
  migrateLegacy: vi.fn(),
  chooseFolder: vi.fn(),
  reviewPatchBuild: vi.fn(),
  applyPatchBuild: vi.fn(),
  patchBuildBackups: vi.fn(),
  restorePatchBuild: vi.fn(),
  showPatchBuildBackups: vi.fn(),
  libraryRecoverySnapshots: vi.fn(),
  restoreLibrary: vi.fn(),
  listSamples: vi.fn(),
  querySamples: vi.fn(),
  openSampleAudition: vi.fn(),
  stopSampleAudition: vi.fn(),
  openManagedEditor: vi.fn(),
  prepareSdImport: vi.fn(),
  finishSdImport: vi.fn(),
}));

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
const snare = { ...sample, id: "snare", name: "Dry snare", sha256: "snare" };

function emptyVoices() {
  return Array.from({ length: 4 }, () => ({
    layers: [] as { id: string; sampleId: string }[],
    selectedLayer: "",
    mode: "manual" as const,
  }));
}
function kit(id: string, bank: string, number: number): PatchKit {
  return { id, bank, number, voices: emptyVoices() };
}
function sessionVoices(voices: PatchKit["voices"]) {
  return voices.map((v) => ({
    kitID: "kit",
    samplePath: v.selectedLayer,
    steps: Array(16).fill(false) as boolean[],
    volume: 0.8,
    muted: false,
    solo: false,
    layerMode: v.mode,
    velocities: Array(16).fill(100) as number[],
  }));
}
/** A Library with two Patches; "Live set" is open on Kit A0. */
function view(): PatchSnapshot {
  const a0 = kit("kit", "A", 0);
  a0.voices[0] = {
    layers: [
      { id: "occ-first", sampleId: "sample" },
      { id: "occ-second", sampleId: "snare" },
    ],
    selectedLayer: "occ-second",
    mode: "manual",
  };
  const b12 = kit("other-kit", "B", 12);
  b12.voices[0] = {
    layers: [{ id: "other-layer", sampleId: "sample" }],
    selectedLayer: "other-layer",
    mode: "manual",
  };
  const voices = sessionVoices(a0.voices);
  const live: Patch = {
    id: "patch",
    name: "Live set",
    selectedKit: "kit",
    kits: [b12, a0],
    bpm: 120,
    voices,
  };
  const studio: Patch = {
    id: "studio",
    name: "Studio",
    selectedKit: "",
    kits: [],
    bpm: 120,
    voices,
  };
  return {
    graph: { selectedPatch: "patch", patches: [live, studio] },
    session: {
      schemaVersion: 2,
      rootPath: "",
      selectedKit: "kit",
      bpm: 120,
      voices,
      kitConfigurations: { libraries: {} },
    },
    samples: [sample, snare],
    waveforms: { "occ-second": Array(100).fill(0.5) },
    validation: [
      "Live set · B12: Voice 2 has 13 Layers; Rample supports at most 12. All draft Layers are retained.",
      "Studio · A0: Voice 1 needs a Sample.",
    ],
    playing: false,
  };
}
function legacy(managed: boolean, isDemo = false) {
  return { managed, isDemo } as unknown as Snapshot;
}

beforeEach(() => {
  vi.mocked(native.getSnapshot).mockResolvedValue(legacy(true));
  vi.mocked(native.getTransport).mockResolvedValue(idleTransport);
  vi.mocked(native.migrationStatus).mockResolvedValue({
    status: "idle",
    processed: 0,
    total: 0,
    current: null,
    errors: [],
    backup: null,
  });
  vi.mocked(native.openSampleAudition).mockResolvedValue("picker");
  vi.mocked(native.listSamples).mockResolvedValue([sample, snare]);
  vi.mocked(native.querySamples).mockResolvedValue([sample]);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

async function openApp(initial: PatchSnapshot = view()) {
  vi.mocked(native.patchAction).mockResolvedValue(initial);
  render(<App />);
  return screen.findByRole("region", { name: "Voice 1" });
}

describe("Patch-first main screen", () => {
  it("opens the current Patch and Kit directly, without folder controls", async () => {
    await openApp();
    expect(native.patchAction).toHaveBeenCalledWith({ type: "open" });
    expect(screen.getByRole("button", { name: "Patch Live set" })).toBeTruthy();
    expect(document.querySelector(".kit-code")?.textContent).toBe("A0");
    expect(screen.getAllByRole("region", { name: /^Voice \d$/ })).toHaveLength(
      4,
    );
    expect(
      screen.getAllByRole("button", { name: /Voice \d, step / }),
    ).toHaveLength(64);
    for (const name of [
      /Open folder/,
      /^Patches/,
      /Export kit/,
      /Check sample formats/,
      /Back to folders/,
    ])
      expect(screen.queryByRole("button", { name })).toBeNull();
  });

  it("lists the Patch's Kits in Rample order and switches Kit", async () => {
    await openApp();
    const kits = screen.getByRole("navigation", { name: "Sample kits" });
    const rows = within(kits).getAllByRole("button", { name: /^Load kit/ });
    expect(rows.map((row) => row.getAttribute("aria-label"))).toEqual([
      "Load kit A0",
      "Load kit B12",
    ]);
    expect(rows[0].getAttribute("aria-current")).toBe("true");
    expect(within(rows[0]).getByText("2 layers")).toBeTruthy();
    // Only issues of the open Patch are shown, on the Kit they belong to.
    expect(within(rows[1]).getByRole("img", { name: "1 issue" })).toBeTruthy();
    expect(within(rows[0]).queryByRole("img")).toBeNull();
    fireEvent.click(rows[1]);
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "selectKit",
        kit: "other-kit",
      }),
    );
  });

  it("searches Kits by address or by the names of their Samples", async () => {
    await openApp();
    const kits = screen.getByRole("navigation", { name: "Sample kits" });
    const search = screen.getByRole("textbox", {
      name: "Search kits or samples",
    });
    fireEvent.change(search, { target: { value: "snare" } });
    expect(
      within(kits)
        .getAllByRole("button", { name: /^Load kit/ })
        .map((b) => b.getAttribute("aria-label")),
    ).toEqual(["Load kit A0"]);
    fireEvent.change(search, { target: { value: "b1" } });
    expect(
      within(kits)
        .getAllByRole("button", { name: /^Load kit/ })
        .map((b) => b.getAttribute("aria-label")),
    ).toEqual(["Load kit B12"]);
    fireEvent.change(search, { target: { value: "nothing" } });
    expect(within(kits).getByText("No matches")).toBeTruthy();
  });

  it("creates a Kit at the next free address and refuses a used one", async () => {
    await openApp();
    fireEvent.click(screen.getByRole("button", { name: "New Kit" }));
    const form = screen.getByRole("form", { name: "New Kit address" });
    const bank = within(form).getByRole("combobox", { name: "Bank" });
    const number = within(form).getByRole("textbox", { name: "Kit number" });
    expect((bank as HTMLSelectElement).value).toBe("A");
    expect((number as HTMLInputElement).value).toBe("1");
    fireEvent.change(number, { target: { value: "0" } });
    expect(within(form).getByRole("alert").textContent).toBe(
      "A0 is already used in this Patch",
    );
    expect(
      within(form)
        .getByRole("button", { name: "Create Kit" })
        .hasAttribute("disabled"),
    ).toBe(true);
    fireEvent.change(bank, { target: { value: "C" } });
    fireEvent.change(number, { target: { value: "7" } });
    fireEvent.click(within(form).getByRole("button", { name: "Create Kit" }));
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "createKit",
        bank: "C",
        number: 7,
      }),
    );
    await vi.waitFor(() =>
      expect(
        screen.queryByRole("form", { name: "New Kit address" }),
      ).toBeNull(),
    );
  });

  it("changes the selected Kit's address and keeps the form open on failure", async () => {
    await openApp();
    fireEvent.click(
      screen.getByRole("button", { name: "Change address of Kit A0" }),
    );
    const form = screen.getByRole("form", {
      name: "Change address of Kit A0",
    });
    // The Kit's own address is not a conflict.
    expect(within(form).queryByRole("alert")).toBeNull();
    vi.mocked(native.patchAction).mockRejectedValueOnce(
      "That Bank/Kit address is already used in this Patch.",
    );
    fireEvent.change(
      within(form).getByRole("textbox", { name: "Kit number" }),
      {
        target: { value: "4" },
      },
    );
    fireEvent.click(within(form).getByRole("button", { name: "Move Kit" }));
    expect(
      (await screen.findByRole("alert", { name: "" })).textContent,
    ).toContain("already used");
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "addressKit",
      bank: "A",
      number: 4,
    });
    expect(
      screen.getByRole("form", { name: "Change address of Kit A0" }),
    ).toBeTruthy();
    fireEvent.keyDown(
      within(form).getByRole("textbox", { name: "Kit number" }),
      { key: "Escape" },
    );
    expect(
      screen.queryByRole("form", { name: "Change address of Kit A0" }),
    ).toBeNull();
  });

  it("manages each Voice's ordered Layers from its card", async () => {
    const initial = view();
    const reordered = structuredClone(initial);
    reordered.graph.patches[0].kits[1].voices[0].layers.reverse();
    await openApp(initial);
    vi.mocked(native.patchAction).mockResolvedValue(reordered);
    const voice = screen.getByRole("region", { name: "Voice 1" });
    expect(
      within(voice)
        .getByRole("button", { name: "Select Layer 2: Dry snare" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(within(voice).getByText("2 LAYERS")).toBeTruthy();
    // The waveform and details describe the heard (selected) Layer.
    expect(
      within(voice).getByRole("img", { name: "Waveform" }).innerHTML,
    ).toContain("M1.5,");
    expect(within(voice).getAllByText("Dry snare").length).toBeGreaterThan(1);
    fireEvent.click(
      within(voice).getByRole("button", { name: "Move Layer 2 up" }),
    );
    await screen.findByRole("button", {
      name: "Select Layer 1: Dry snare",
      pressed: true,
    });
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "moveLayer",
      channel: 0,
      layer: "occ-second",
      to: 0,
    });
    fireEvent.click(
      within(voice).getByRole("button", { name: "Repeat Layer 1" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "addLayer",
        channel: 0,
        sample: "snare",
      }),
    );
    fireEvent.click(
      within(voice).getByRole("button", { name: "Remove Layer 2" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "removeLayer",
        channel: 0,
        layer: "occ-first",
      }),
    );
    fireEvent.click(
      within(voice).getByRole("button", { name: "Select Layer 2: Warm kick" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "selectLayer",
        channel: 0,
        layer: "occ-first",
      }),
    );
    const mode = within(voice).getByRole("combobox", {
      name: "Layer mode for voice 1",
    });
    expect(mode.children.length).toBe(5);
    fireEvent.change(mode, { target: { value: "cyclic" } });
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "musical",
        action: { type: "setMode", channel: 0, mode: "cyclic" },
      }),
    );
    expect(
      within(screen.getByRole("region", { name: "Voice 2" })).getByText(
        "No Layers yet",
      ),
    ).toBeTruthy();
  });

  it("shows when a Voice plays from another Kit after a folder migration", async () => {
    const initial = view();
    initial.graph.patches[0].voiceKits = ["other-kit", "kit", "kit", "kit"];
    const voice = await openApp(initial);
    expect(within(voice).getByText("FROM B12")).toBeTruthy();
    expect(
      within(voice)
        .getByRole("button", { name: "Select Layer 1: Warm kick" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
  });

  it("shows the open Patch's issues without its name and the SD Import report", async () => {
    const initial = view();
    initial.validation.push("Voice 1, Layer 2: Original changed.");
    initial.graph.patches[0].importedSd = {
      intakeId: "intake",
      copyComplete: true,
      fingerprint: "fp",
      files: [],
      issues: ["Z9 is not a Kit folder."],
      unassigned: [
        { path: "A0/loose.wav", sha256: "x", kind: "audio", sampleId: "s" },
      ],
    };
    await openApp(initial);
    expect(screen.getByText("2 issues to fix before Build")).toBeTruthy();
    expect(
      screen.getByText(
        "B12: Voice 2 has 13 Layers; Rample supports at most 12. All draft Layers are retained.",
      ),
    ).toBeTruthy();
    expect(
      screen.getByText("Voice 1, Layer 2: Original changed."),
    ).toBeTruthy();
    expect(screen.queryByText(/Voice 1 needs a Sample/)).toBeNull();
    expect(screen.getByText("Z9 is not a Kit folder.")).toBeTruthy();
    expect(
      screen.getByText("Unassigned source occurrence: A0/loose.wav"),
    ).toBeTruthy();
  });

  it("offers the first Kit of an empty Patch and the first Patch of an empty Library", async () => {
    const empty = view();
    empty.graph.selectedPatch = "studio";
    vi.mocked(native.patchAction).mockResolvedValue(empty);
    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Create Kit A0" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "createKit",
        bank: "A",
        number: 0,
      }),
    );
    cleanup();
    const none = view();
    none.graph = { selectedPatch: "", patches: [] };
    none.session = null;
    vi.mocked(native.patchAction).mockResolvedValue(none);
    render(<App />);
    const start = await screen.findByRole("region", { name: "Start a Patch" });
    fireEvent.click(
      within(start).getByRole("button", { name: "Create Patch" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "createPatch",
        name: "My Patch",
      }),
    );
    expect(
      within(start).getByRole("button", { name: /Import Rample SD/ }),
    ).toBeTruthy();
  });
});

describe("Patch switcher", () => {
  it("switches, creates and renames Patches and returns to the Kit view", async () => {
    await openApp();
    const trigger = screen.getByRole("button", { name: "Patch Live set" });
    fireEvent.click(trigger);
    let menu = screen.getByRole("dialog", { name: "Patches" });
    expect(within(menu).getByText("2 Kits")).toBeTruthy();
    expect(
      within(menu)
        .getByRole("button", { name: /Live set/, pressed: true })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    fireEvent.click(within(menu).getByRole("button", { name: /Studio/ }));
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "selectPatch",
        patch: "studio",
      }),
    );
    await vi.waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Patches" })).toBeNull(),
    );

    fireEvent.click(trigger);
    menu = screen.getByRole("dialog", { name: "Patches" });
    fireEvent.change(
      within(menu).getByRole("textbox", { name: "New Patch name" }),
      { target: { value: "Live 2027" } },
    );
    fireEvent.click(within(menu).getByRole("button", { name: "Create Patch" }));
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "createPatch",
        name: "Live 2027",
      }),
    );

    fireEvent.click(trigger);
    menu = screen.getByRole("dialog", { name: "Patches" });
    fireEvent.click(
      within(menu).getByRole("button", { name: "Rename Patch Live set" }),
    );
    const name = within(menu).getByRole("textbox", { name: "Patch name" });
    fireEvent.change(name, { target: { value: "Tour set" } });
    fireEvent.click(within(menu).getByRole("button", { name: "Rename" }));
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "renamePatch",
        name: "Tour set",
      }),
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Patches" })).toBeNull();
  });

  it("sets the Library default Layer mode used for new and never-stored Kits", async () => {
    const initial = view();
    const updated = structuredClone(initial);
    updated.graph.defaultLayerMode = "random";
    await openApp(initial);
    vi.mocked(native.patchAction).mockResolvedValue(updated);
    fireEvent.click(screen.getByRole("button", { name: "Patch Live set" }));
    const select = screen.getByRole("combobox", { name: "Default Layer mode" });
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

  it("imports a Rample SD from the switcher", async () => {
    await openApp();
    vi.mocked(native.chooseFolder).mockResolvedValue(null);
    fireEvent.click(screen.getByRole("button", { name: "Patch Live set" }));
    fireEvent.click(screen.getByRole("button", { name: /Import Rample SD/ }));
    expect(
      await screen.findByRole("dialog", { name: "Import Rample SD" }),
    ).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Patches" })).toBeNull();
  });
});

describe("Library", () => {
  it("adds another occurrence from the searchable Library picker", async () => {
    await openApp();
    fireEvent.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
    );
    fireEvent.change(await screen.findByRole("searchbox"), {
      target: { value: "round" },
    });
    fireEvent.click(
      await screen.findByRole("button", { name: "Select Warm kick" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Use Sample" }));
    await vi.waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Library samples" }),
      ).toBeNull(),
    );
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "addLayer",
      channel: 0,
      sample: "sample",
    });
  });

  it("reports a failed picker add and prevents a second add while it is pending", async () => {
    await openApp();
    let rejectAdd: (error: Error) => void = () => {};
    fireEvent.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
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
    const toast = await screen.findByText("Library disk is full");
    expect(toast.closest('[role="alert"]')).toBeTruthy();
    expect(
      screen.getByRole("dialog", { name: "Library samples" }),
    ).toBeTruthy();
  });

  it("refreshes Patch state after a Library restore", async () => {
    vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([]);
    const restored = view();
    restored.graph.patches[0].name = "Restored set";
    await openApp();
    vi.mocked(native.patchAction).mockResolvedValue(restored);
    vi.mocked(native.chooseFolder).mockResolvedValue("/backup/Library");
    vi.mocked(native.restoreLibrary).mockResolvedValue({
      complete: true,
      activated: true,
      location: "/backup/Library",
      recovery: "/recovery/previous",
      error: null,
      snapshot: null,
    });
    fireEvent.click(screen.getByRole("button", { name: /Library backup/ }));
    fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Replace Library" }),
    );
    await screen.findByText("Library restored.");
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(
      await screen.findByRole("button", { name: "Patch Restored set" }),
    ).toBeTruthy();
  });

  it("offers moving an old folder session into the Library until hidden", async () => {
    vi.mocked(native.getSnapshot).mockResolvedValue(legacy(false));
    await openApp();
    const migration = await screen.findByRole("region", {
      name: "Managed Library migration",
    });
    expect(
      within(migration).getByRole("button", {
        name: "Move current folder into Library",
      }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Hide folder migration" }),
    );
    expect(
      screen.queryByRole("region", { name: "Managed Library migration" }),
    ).toBeNull();
  });

  it("does not offer migration once migrated or for the bundled demo", async () => {
    vi.mocked(native.getSnapshot).mockResolvedValue(legacy(false, true));
    await openApp();
    await vi.waitFor(() => expect(native.getSnapshot).toHaveBeenCalled());
    expect(
      screen.queryByRole("region", { name: "Managed Library migration" }),
    ).toBeNull();
  });
});

describe("keyboard", () => {
  it("plays with Space and auditions with 1–4, but not from focused controls", async () => {
    const user = userEvent.setup();
    await openApp();
    await user.keyboard(" ");
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "musical",
        action: { type: "togglePlayback" },
      }),
    );
    await user.keyboard("3");
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "musical",
        action: { type: "audition", channel: 2 },
      }),
    );
    vi.mocked(native.patchAction).mockClear();
    screen.getByRole("textbox", { name: "Search kits or samples" }).focus();
    await user.keyboard(" 1");
    screen.getByRole("button", { name: "Add Sample to Voice 1" }).focus();
    await user.keyboard(" ");
    expect(await screen.findByRole("searchbox")).toBeTruthy();
    expect(native.patchAction).not.toHaveBeenCalledWith({
      type: "musical",
      action: { type: "togglePlayback" },
    });
  });

  it("keeps shortcuts suspended while a Library view is open", async () => {
    const user = userEvent.setup();
    await openApp();
    await user.click(screen.getByRole("button", { name: "Library samples…" }));
    await screen.findByRole("searchbox");
    (document.activeElement as HTMLElement).blur();
    const calls = vi.mocked(native.patchAction).mock.calls.length;
    await user.keyboard(" 1");
    expect(native.patchAction).toHaveBeenCalledTimes(calls);
  });

  it("opens a velocity editor from a step's context menu", async () => {
    await openApp();
    fireEvent.contextMenu(
      screen.getByRole("button", { name: "Voice 2, step 5" }),
    );
    const slider = await screen.findByRole("slider", { name: "Step velocity" });
    fireEvent.change(slider, { target: { value: "64" } });
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "musical",
        action: { type: "setVelocity", channel: 1, step: 4, value: 64 },
      }),
    );
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
function buildReview(overrides: Partial<BuildReview> = {}): BuildReview {
  return {
    patchId: "patch",
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
    reviewedAudio: [],
    reviewedKitFolders: ["A0", "C3"],
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
function build() {
  fireEvent.click(screen.getByRole("button", { name: "Build to SD…" }));
}

describe("Build to SD", () => {
  it("reviews destination changes before applying a Build and reports its backup", async () => {
    vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
    const review = buildReview();
    vi.mocked(native.reviewPatchBuild).mockResolvedValue(review);
    let finish: (reply: BuildReply) => void = () => {};
    vi.mocked(native.applyPatchBuild).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await openApp();
    build();
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
      within(
        within(panel).getByRole("list", { name: "Files to remove" }),
      ).getByText("C3/1_Old.wav"),
    ).toBeTruthy();
    expect(within(panel).getByText(/backed up on this computer/)).toBeTruthy();
    fireEvent.click(within(panel).getByRole("button", { name: "Apply Build" }));
    await vi.waitFor(() =>
      expect(native.applyPatchBuild).toHaveBeenCalledWith(review),
    );
    expect(await screen.findByText("Building Patch…")).toBeTruthy();
    finish({
      destination: "/Volumes/RAMPLE",
      kits: ["A0"],
      files: builtFiles,
      notices: [],
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
        "Replaced and removed files are backed up in /Support/Patch build backups/backup-1.",
      ),
    ).toBeTruthy();
  });

  it("blocks a review with conflicts and can cancel without applying", async () => {
    vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
    vi.mocked(native.reviewPatchBuild).mockResolvedValue(
      buildReview({
        conflicts: [
          "a0 has the same name as Kit folder A0 on cards that ignore letter case.",
        ],
      }),
    );
    await openApp();
    build();
    const panel = await screen.findByRole("region", {
      name: "Build Patch review",
    });
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
    await openApp();
    build();
    fireEvent.click(
      within(
        await screen.findByRole("region", { name: "Build Patch review" }),
      ).getByRole("button", { name: "Apply Build" }),
    );
    const result = await screen.findByRole("region", {
      name: "Build Patch result",
    });
    expect(within(result).getByText(error)).toBeTruthy();
    expect(within(result).queryByText(/^Built /)).toBeNull();
  });

  it("reports a blocked review without claiming output and ignores a cancelled folder choice", async () => {
    await openApp();
    vi.mocked(native.chooseFolder).mockResolvedValueOnce(null);
    build();
    await vi.waitFor(() => expect(native.chooseFolder).toHaveBeenCalled());
    expect(native.reviewPatchBuild).not.toHaveBeenCalled();
    vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
    vi.mocked(native.reviewPatchBuild).mockRejectedValue(
      "Build Patch is blocked until the Patch is valid. Nothing was written.\nB12: Voice 1 needs a Sample.",
    );
    build();
    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText("B12: Voice 1 needs a Sample."),
    ).toBeTruthy();
    expect(
      within(alert).getByText(
        "Build Patch is blocked until the Patch is valid. Nothing was written.",
      ),
    ).toBeTruthy();
    expect(
      screen.queryByRole("region", { name: "Build Patch result" }),
    ).toBeNull();
  });

  it("lists destination backups and restores one after confirmation", async () => {
    vi.mocked(native.patchBuildBackups).mockResolvedValue([backup]);
    vi.mocked(native.restorePatchBuild).mockResolvedValue({
      destination: "/Volumes/RAMPLE",
      restored: 3,
      removed: 1,
      backup: { ...backup, restored: 1_800_000_100_000 },
      notices: [],
    });
    await openApp();
    fireEvent.click(
      screen.getByRole("button", { name: "Destination backups" }),
    );
    const panel = await screen.findByRole("region", {
      name: "Destination backups",
    });
    const item = within(panel).getByRole("listitem", {
      name: "Live set → /Volumes/RAMPLE",
    });
    fireEvent.click(within(item).getByRole("button", { name: "Restore…" }));
    expect(native.restorePatchBuild).not.toHaveBeenCalled();
    fireEvent.click(
      within(item).getByRole("button", { name: "Restore destination" }),
    );
    await vi.waitFor(() =>
      expect(native.restorePatchBuild).toHaveBeenCalledWith(
        "backup-1",
        "/Volumes/RAMPLE",
      ),
    );
    expect(
      await within(panel).findByText(
        "Restored /Volumes/RAMPLE: 3 files put back and 1 removed.",
      ),
    ).toBeTruthy();
  });

  it("offers restoring the destination of an incomplete Build", async () => {
    vi.mocked(native.chooseFolder).mockResolvedValue("/Volumes/RAMPLE");
    vi.mocked(native.reviewPatchBuild).mockResolvedValue(buildReview());
    const error = "Build Patch did not complete: Could not write B1.";
    const incomplete = { ...backup, status: "incomplete" as const, error };
    vi.mocked(native.applyPatchBuild).mockResolvedValue({
      destination: "/Volumes/RAMPLE",
      kits: ["A0"],
      files: builtFiles,
      notices: [],
      added: 1,
      replaced: 1,
      removed: 2,
      unchanged: 0,
      backup: incomplete,
      error,
    });
    vi.mocked(native.patchBuildBackups).mockResolvedValue([incomplete]);
    await openApp();
    build();
    fireEvent.click(
      within(
        await screen.findByRole("region", { name: "Build Patch review" }),
      ).getByRole("button", { name: "Apply Build" }),
    );
    const result = await screen.findByRole("region", {
      name: "Build Patch result",
    });
    fireEvent.click(
      within(result).getByRole("button", { name: "Restore destination…" }),
    );
    const panel = await screen.findByRole("region", {
      name: "Destination backups",
    });
    expect(within(panel).getByText(`Incomplete: ${error}`)).toBeTruthy();
  });
});
