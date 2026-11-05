import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
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
} from "./types";

vi.mock("./native", () => ({
  getTransport: vi.fn(),
  patchAction: vi.fn(),
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
  auditionSample: vi.fn(),
  sampleWaveform: vi.fn(),
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
      selectedKit: "kit",
      bpm: 120,
      voices,
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

beforeEach(() => {
  vi.mocked(native.getTransport).mockResolvedValue(idleTransport);
  vi.mocked(native.openSampleAudition).mockResolvedValue("picker");
  vi.mocked(native.listSamples).mockResolvedValue([sample, snare]);
  vi.mocked(native.querySamples).mockResolvedValue([sample]);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

/** jsdom has no DataTransfer; this keeps data by type like a browser does. */
function dataTransfer() {
  const data: Record<string, string> = {};
  return {
    data,
    get types() {
      return Object.keys(data);
    },
    setData(type: string, value: string) {
      data[type] = value;
    },
    getData(type: string) {
      return data[type] ?? "";
    },
    effectAllowed: "all",
    dropEffect: "none",
  };
}

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

  it("changes a Kit's address from its menu and keeps the form open on failure", async () => {
    await openApp();
    fireEvent.click(screen.getByRole("button", { name: "Kit A0 actions" }));
    fireEvent.click(
      within(screen.getByRole("menu", { name: "Kit A0 actions" })).getByRole(
        "menuitem",
        { name: /Change address/ },
      ),
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
      kit: "kit",
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

  it("groups Kits by Bank and folds a Bank unless it holds the selected Kit", async () => {
    await openApp();
    const kits = screen.getByRole("navigation", { name: "Sample kits" });
    const a = within(kits).getByRole("button", { name: /BANK A/ });
    const b = within(kits).getByRole("button", { name: /BANK B/ });
    fireEvent.click(b);
    expect(b.getAttribute("aria-expanded")).toBe("false");
    expect(
      within(kits).queryByRole("button", { name: "Load kit B12" }),
    ).toBeNull();
    // The selected Kit stays visible.
    fireEvent.click(a);
    expect(
      within(kits).getByRole("button", { name: "Load kit A0" }),
    ).toBeTruthy();
    // Searching shows every match, even in a folded Bank.
    fireEvent.change(
      screen.getByRole("textbox", { name: "Search kits or samples" }),
      { target: { value: "b12" } },
    );
    expect(
      within(kits).getByRole("button", { name: "Load kit B12" }),
    ).toBeTruthy();
  });

  it("duplicates a Kit to the next free address in its Bank", async () => {
    await openApp();
    fireEvent.click(screen.getByRole("button", { name: "Kit B12 actions" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Duplicate/ }));
    const form = screen.getByRole("form", { name: "Duplicate Kit B12" });
    expect(
      (
        within(form).getByRole("combobox", {
          name: "Bank",
        }) as HTMLSelectElement
      ).value,
    ).toBe("B");
    expect(
      (
        within(form).getByRole("textbox", {
          name: "Kit number",
        }) as HTMLInputElement
      ).value,
    ).toBe("0");
    fireEvent.click(
      within(form).getByRole("button", { name: "Duplicate Kit" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "duplicateKit",
        kit: "other-kit",
        bank: "B",
        number: 0,
      }),
    );
  });

  it("deletes a Kit at once and offers Undo, also with the keyboard", async () => {
    const user = userEvent.setup();
    await openApp();
    const deleted = view();
    deleted.graph.patches[0].kits.shift();
    deleted.undo = "Kit B12";
    vi.mocked(native.patchAction).mockResolvedValueOnce(deleted);
    await user.click(screen.getByRole("button", { name: "Kit B12 actions" }));
    await user.click(screen.getByRole("menuitem", { name: /Delete Kit/ }));
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "deleteKit",
      kit: "other-kit",
    });
    const toast = (await screen.findByText("Deleted Kit B12")).closest(
      ".toast",
    ) as HTMLElement;
    expect(toast.getAttribute("role")).toBe("status");
    await user.click(within(toast).getByRole("button", { name: /Undo/ }));
    expect(native.patchAction).toHaveBeenLastCalledWith({ type: "undoDelete" });
    expect(screen.queryByText("Deleted Kit B12")).toBeNull();

    // Arrow keys move between Kits; Delete removes the focused one.
    screen.getByRole("button", { name: "Load kit A0" }).focus();
    await user.keyboard("{ArrowDown}");
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "selectKit",
      kit: "other-kit",
    });
    // Focus follows the selection, so the arrows keep walking the list.
    await vi.waitFor(() =>
      expect(document.activeElement?.getAttribute("aria-label")).toBe(
        "Load kit B12",
      ),
    );
    vi.mocked(native.patchAction).mockResolvedValueOnce({
      ...view(),
      undo: "Kit A0",
    });
    screen.getByRole("button", { name: "Load kit A0" }).focus();
    await user.keyboard("{Delete}");
    expect(native.patchAction).toHaveBeenLastCalledWith({
      type: "deleteKit",
      kit: "kit",
    });
    await screen.findByText("Deleted Kit A0");
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("{Meta>}z{/Meta}");
    expect(native.patchAction).toHaveBeenLastCalledWith({ type: "undoDelete" });
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

  it("shows when a Voice plays from another Kit", async () => {
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
        "Voice 2 has 13 Layers; Rample supports at most 12. All draft Layers are retained.",
      ),
    ).toBeTruthy();
    // Each Kit issue links to its Kit.
    fireEvent.click(screen.getByRole("button", { name: "Show Kit B12" }));
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "selectKit",
        kit: "other-kit",
      }),
    );
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
    fireEvent.click(within(menu).getByRole("button", { name: /^Studio/ }));
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

  it("duplicates and deletes Patches, offering Undo for the deletion", async () => {
    await openApp();
    fireEvent.click(screen.getByRole("button", { name: "Patch Live set" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Duplicate Patch Live set" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "duplicatePatch",
        patch: "patch",
        name: "Live set copy",
      }),
    );
    await vi.waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Patches" })).toBeNull(),
    );
    const deleted = view();
    deleted.graph.patches.pop();
    deleted.undo = "Patch Studio";
    vi.mocked(native.patchAction).mockResolvedValue(deleted);
    fireEvent.click(screen.getByRole("button", { name: "Patch Live set" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Delete Patch Studio" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "deletePatch",
        patch: "studio",
      }),
    );
    expect(await screen.findByText("Deleted Patch Studio")).toBeTruthy();
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
  it("adds Samples from the side panel, auditioning them while browsing", async () => {
    const user = userEvent.setup();
    const many = view();
    many.samples.push(
      { ...sample, id: "retired", name: "Old kick", retired: true },
      { ...sample, id: "clap", name: "Clap", tags: ["hand"] },
    );
    await openApp(many);
    await user.click(
      screen.getByRole("button", { name: "Add Sample to Voice 2" }),
    );
    const panel = screen.getByRole("complementary", { name: "Add Samples" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(
      within(panel)
        .getByRole("radio", { name: "Voice 2" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    const search = within(panel).getByRole("searchbox");
    expect(document.activeElement).toBe(search);
    const options = () =>
      within(panel)
        .getAllByRole("option")
        .map((o) => o.querySelector("strong")?.textContent);
    // Retired Samples are not offered.
    expect(options()).toEqual(["Warm kick", "Dry snare", "Clap"]);
    // Search matches names and tags.
    await user.type(search, "SNARE");
    expect(options()).toEqual(["Dry snare"]);
    await user.clear(search);
    await user.type(search, "hand");
    expect(options()).toEqual(["Clap"]);
    await user.keyboard("{ArrowDown}");
    await vi.waitFor(() =>
      expect(native.auditionSample).toHaveBeenLastCalledWith(
        "clap",
        "picker",
        true,
      ),
    );
    await user.keyboard("{Enter}");
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "addLayer",
        channel: 1,
        sample: "clap",
      }),
    );
    expect(
      await within(panel).findByText("Added Clap to Voice 2"),
    ).toBeTruthy();
    // The panel stays open; the target can change for the next Sample.
    await user.clear(search);
    await user.click(within(panel).getByRole("radio", { name: "Voice 4" }));
    await user.click(
      within(panel).getByRole("button", { name: "Add Warm kick to Voice 4" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "addLayer",
        channel: 3,
        sample: "sample",
      }),
    );
    // Turning off browsing audition keeps the arrows silent.
    vi.mocked(native.auditionSample).mockClear();
    await user.click(within(panel).getByRole("checkbox"));
    search.focus();
    await user.keyboard("{ArrowUp}");
    expect(native.auditionSample).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("complementary", { name: "Add Samples" }),
    ).toBeNull();
    expect(native.stopSampleAudition).toHaveBeenCalledWith("picker", true);
  });

  it("selects several Samples in the side panel to add or drag them together", async () => {
    const user = userEvent.setup();
    const many = view();
    many.samples.push({ ...sample, id: "clap", name: "Clap", tags: [] });
    await openApp(many);
    await user.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
    );
    const panel = screen.getByRole("complementary", { name: "Add Samples" });
    const option = (name: RegExp) =>
      within(panel).getByRole("option", { name });
    const chosen = () =>
      within(panel)
        .getAllByRole("option")
        .filter((o) => o.getAttribute("aria-selected") === "true")
        .map((o) => o.querySelector("strong")?.textContent);
    await user.click(option(/Warm kick/));
    await user.keyboard("{Shift>}");
    await user.click(option(/Clap/));
    await user.keyboard("{/Shift}");
    expect(chosen()).toEqual(["Warm kick", "Dry snare", "Clap"]);
    await user.keyboard("{Meta>}");
    await user.click(option(/Dry snare/));
    await user.keyboard("{/Meta}");
    expect(chosen()).toEqual(["Warm kick", "Clap"]);

    // Enter adds the selection in list order.
    vi.mocked(native.patchAction).mockClear();
    await user.keyboard("{Enter}");
    await vi.waitFor(() =>
      expect(
        vi.mocked(native.patchAction).mock.calls.map(([a]) => a),
      ).toEqual([
        { type: "addLayer", channel: 0, sample: "sample" },
        { type: "addLayer", channel: 0, sample: "clap" },
      ]),
    );
    expect(
      await within(panel).findByText("Added 2 Samples to Voice 1"),
    ).toBeTruthy();

    // So does the selection's Add button, and Cmd+A selects every row.
    vi.mocked(native.patchAction).mockClear();
    within(panel).getByRole("listbox").focus();
    await user.keyboard("{Meta>}a{/Meta}");
    expect(chosen()).toEqual(["Warm kick", "Dry snare", "Clap"]);
    await user.click(
      within(panel).getByRole("button", { name: "Add 3 to Voice 1" }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenCalledTimes(3),
    );

    // Dragging a selected row carries the whole selection to any voice.
    vi.mocked(native.patchAction).mockClear();
    await user.click(option(/Dry snare/));
    await user.keyboard("{Shift>}");
    await user.click(option(/Clap/));
    await user.keyboard("{/Shift}");
    const transfer = dataTransfer();
    fireEvent.dragStart(option(/Clap/), { dataTransfer: transfer });
    fireEvent.drop(screen.getByRole("region", { name: "Voice 4" }), {
      dataTransfer: transfer,
    });
    await vi.waitFor(() =>
      expect(
        vi.mocked(native.patchAction).mock.calls.map(([a]) => a),
      ).toEqual([
        { type: "addLayer", channel: 3, sample: "snare" },
        { type: "addLayer", channel: 3, sample: "clap" },
      ]),
    );
    // An unselected row drags alone.
    vi.mocked(native.patchAction).mockClear();
    const single = dataTransfer();
    fireEvent.dragStart(option(/Warm kick/), { dataTransfer: single });
    fireEvent.drop(screen.getByRole("region", { name: "Voice 2" }), {
      dataTransfer: single,
    });
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenCalledWith({
        type: "addLayer",
        channel: 1,
        sample: "sample",
      }),
    );
    expect(native.patchAction).toHaveBeenCalledTimes(1);
  });

  it("combines tag filters in the side panel", async () => {
    const many = view();
    many.samples = [
      { ...sample, id: "a", name: "Big kick", tags: ["kick", "industrial"] },
      { ...sample, id: "b", name: "Soft kick", tags: ["kick"] },
      { ...sample, id: "c", name: "Clank", tags: ["industrial"] },
    ];
    await openApp(many);
    fireEvent.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
    );
    const panel = screen.getByRole("complementary", { name: "Add Samples" });
    const tags = within(panel).getByRole("group", { name: "Filter by tags" });
    const options = () =>
      within(panel)
        .getAllByRole("option")
        .map((o) => o.querySelector("strong")?.textContent);
    fireEvent.click(within(tags).getByRole("button", { name: "#kick" }));
    fireEvent.click(within(tags).getByRole("button", { name: "#industrial" }));
    expect(options()).toEqual(["Big kick"]);
    expect(
      within(tags)
        .getAllByRole("button")
        .filter((b) => b.getAttribute("aria-pressed") === "true")
        .map((b) => b.textContent),
    ).toEqual(["#kick", "#industrial"]);
    fireEvent.click(within(tags).getByRole("button", { name: "#kick" }));
    expect(options()).toEqual(["Big kick", "Clank"]);
  });

  it("reports a failed add and keeps the panel open", async () => {
    await openApp();
    fireEvent.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
    );
    vi.mocked(native.patchAction).mockRejectedValueOnce(
      new Error("Library disk is full"),
    );
    fireEvent.doubleClick(
      within(
        screen.getByRole("complementary", { name: "Add Samples" }),
      ).getByRole("option", { name: /Warm kick/ }),
    );
    const toast = await screen.findByText("Library disk is full");
    expect(toast.closest('[role="alert"]')).toBeTruthy();
    expect(
      screen.getByRole("complementary", { name: "Add Samples" }),
    ).toBeTruthy();
    expect(screen.queryByText(/^Added /)).toBeNull();
  });

  it("pages long Libraries instead of rendering every Sample", async () => {
    const big = view();
    big.samples = Array.from({ length: 320 }, (_, i) => ({
      ...sample,
      id: `s${i}`,
      name: `Hit ${i}`,
    }));
    await openApp(big);
    fireEvent.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
    );
    const panel = screen.getByRole("complementary", { name: "Add Samples" });
    expect(within(panel).getAllByRole("option")).toHaveLength(150);
    fireEvent.click(
      within(panel).getByRole("button", { name: "Show 150 more of 170" }),
    );
    expect(within(panel).getAllByRole("option")).toHaveLength(300);
    expect(within(panel).getByText("320 Samples")).toBeTruthy();
  });

  it("drops a dragged Sample on any voice and reorders Layers by dragging", async () => {
    await openApp();
    fireEvent.click(
      screen.getByRole("button", { name: "Add Sample to Voice 1" }),
    );
    const transfer = dataTransfer();
    fireEvent.dragStart(
      within(
        screen.getByRole("complementary", { name: "Add Samples" }),
      ).getByRole("option", { name: /Dry snare/ }),
      { dataTransfer: transfer },
    );
    const voice3 = screen.getByRole("region", { name: "Voice 3" });
    fireEvent.dragOver(voice3, { dataTransfer: transfer });
    expect(voice3.className).toContain("dropping");
    fireEvent.drop(voice3, { dataTransfer: transfer });
    expect(voice3.className).not.toContain("dropping");
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "addLayer",
        channel: 2,
        sample: "snare",
      }),
    );

    const voice1 = screen.getByRole("region", { name: "Voice 1" });
    const layers = within(voice1).getAllByRole("listitem");
    const move = dataTransfer();
    fireEvent.dragStart(layers[1], { dataTransfer: move });
    fireEvent.dragOver(layers[0], { dataTransfer: move });
    expect(layers[0].className).toContain("drop-before");
    fireEvent.drop(layers[0], { dataTransfer: move });
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "moveLayer",
        channel: 0,
        layer: "occ-second",
        to: 0,
      }),
    );
    // A Layer never moves into another Voice.
    vi.mocked(native.patchAction).mockClear();
    const other = dataTransfer();
    fireEvent.dragStart(layers[0], { dataTransfer: other });
    fireEvent.drop(voice3, { dataTransfer: other });
    expect(native.patchAction).not.toHaveBeenCalled();
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

  it("never offers the old folder session and keeps backups available", async () => {
    vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([]);
    await openApp();
    expect(screen.queryByText(/Move current folder/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Library backup/ }));
    expect(
      (
        (await screen.findByRole("button", {
          name: "Back up Library…",
        })) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });
});

describe("Samples view", () => {
  it("switches between the Kit and the Library, and opens a Kit from where a Sample is used", async () => {
    vi.mocked(native.listSamples).mockResolvedValue([sample, snare]);
    vi.mocked(native.sampleWaveform).mockResolvedValue(Array(100).fill(0.4));
    await openApp();
    fireEvent.click(screen.getByRole("tab", { name: /Samples/ }));
    expect(screen.queryByRole("region", { name: "Voice 1" })).toBeNull();
    const library = await screen.findByRole("listbox", {
      name: "Library Samples",
    });
    fireEvent.click(within(library).getByRole("option", { name: "Warm kick" }));
    const used = screen.getByRole("region", { name: "Used in" });
    // Warm kick plays in Live set's A0 (Layer 1) and B12.
    expect(
      within(used)
        .getAllByRole("button")
        .map((b) => b.getAttribute("aria-label")),
    ).toEqual([
      "Open Live set A0 · Voice 1 · Layer 1",
      "Open Live set B12 · Voice 1 · Layer 1",
    ]);
    fireEvent.click(
      within(used).getByRole("button", {
        name: "Open Live set B12 · Voice 1 · Layer 1",
      }),
    );
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "selectKit",
        kit: "other-kit",
      }),
    );
    expect(await screen.findByRole("region", { name: "Voice 1" })).toBeTruthy();
    expect(
      screen.getByRole("tab", { name: /Kit/ }).getAttribute("aria-selected"),
    ).toBe("true");
  });

  it("returns to the Kit view when a Kit is chosen in the sidebar", async () => {
    vi.mocked(native.listSamples).mockResolvedValue([sample]);
    await openApp();
    fireEvent.click(screen.getByRole("tab", { name: /Samples/ }));
    await screen.findByRole("listbox", { name: "Library Samples" });
    fireEvent.click(screen.getByRole("button", { name: "Load kit B12" }));
    expect(await screen.findByRole("region", { name: "Voice 1" })).toBeTruthy();
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

  it("keeps shortcuts suspended while a Library dialog is open", async () => {
    vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([]);
    const user = userEvent.setup();
    await openApp();
    await user.click(screen.getByRole("button", { name: /Library backup/ }));
    await screen.findByRole("button", { name: "Restore Library…" });
    (document.activeElement as HTMLElement).blur();
    const calls = vi.mocked(native.patchAction).mock.calls.length;
    await user.keyboard(" 1");
    expect(native.patchAction).toHaveBeenCalledTimes(calls);
  });

  it("switches views, searches, walks Kits and lists every shortcut", async () => {
    vi.mocked(native.listSamples).mockResolvedValue([sample]);
    const user = userEvent.setup();
    await openApp();
    await user.keyboard("{Meta>}2{/Meta}");
    expect(
      await screen.findByRole("listbox", { name: "Library Samples" }),
    ).toBeTruthy();
    await user.keyboard("{Meta>}f{/Meta}");
    expect(document.activeElement?.getAttribute("aria-label")).toBe(
      "Search Samples by name or tag",
    );
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("{Meta>}1{/Meta}");
    expect(await screen.findByRole("region", { name: "Voice 1" })).toBeTruthy();
    await user.keyboard("{Meta>}f{/Meta}");
    expect(document.activeElement?.getAttribute("aria-label")).toBe(
      "Search kits or samples",
    );
    // Brackets walk the Kits in Rample order, but not while typing.
    await user.keyboard("]");
    expect(native.patchAction).not.toHaveBeenCalledWith({
      type: "selectKit",
      kit: "other-kit",
    });
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("]");
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "selectKit",
        kit: "other-kit",
      }),
    );
    await user.keyboard("?");
    const sheet = screen.getByRole("dialog", { name: "Keyboard shortcuts" });
    expect(within(sheet).getByText("Previous or next Kit")).toBeTruthy();
    expect(within(sheet).getByText("Search the current view")).toBeTruthy();
    // The sheet suspends playback shortcuts until it closes.
    const calls = vi.mocked(native.patchAction).mock.calls.length;
    await user.keyboard("1");
    expect(native.patchAction).toHaveBeenCalledTimes(calls);
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Keyboard shortcuts" }),
    ).toBeNull();
  });

  it("offers Undo after removing a Layer", async () => {
    await openApp();
    const removed = view();
    removed.graph.patches[0].kits[1].voices[0].layers.pop();
    removed.undo = "Layer 2 of A0 Voice 1";
    vi.mocked(native.patchAction).mockResolvedValueOnce(removed);
    fireEvent.click(
      within(screen.getByRole("region", { name: "Voice 1" })).getByRole(
        "button",
        { name: "Remove Layer 2" },
      ),
    );
    expect(
      await screen.findByText("Deleted Layer 2 of A0 Voice 1"),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Undo/ }));
    await vi.waitFor(() =>
      expect(native.patchAction).toHaveBeenLastCalledWith({
        type: "undoDelete",
      }),
    );
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
  fireEvent.click(screen.getByRole("button", { name: /Build to SD/ }));
}
/** The fixture without Kit issues, so Build asks for a destination. */
function buildable() {
  const snapshot = view();
  snapshot.validation = [];
  return snapshot;
}

describe("Build to SD", () => {
  it("shows what blocks a Build instead of asking for a folder", async () => {
    const blocked = view();
    blocked.validation.push(
      "Live set: Layer modes were not read from this SD Import.",
    );
    await openApp(blocked);
    const button = screen.getByRole("button", { name: /Build to SD/ });
    expect(
      within(button).getByLabelText("1 issue blocks Build").textContent,
    ).toBe("1");
    fireEvent.click(button);
    await vi.waitFor(() =>
      expect(
        screen.getByText(/1 issue to fix before Build/).closest("details")
          ?.open,
      ).toBe(true),
    );
    expect(native.chooseFolder).not.toHaveBeenCalled();
    // Native settings notes are listed apart and do not block a Build.
    expect(screen.getByText("1 note on this Patch")).toBeTruthy();
    expect(
      screen.getByText("Layer modes were not read from this SD Import."),
    ).toBeTruthy();
  });

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
    await openApp(buildable());
    build();
    const panel = await screen.findByRole("region", {
      name: "Build Patch review",
    });
    expect(native.reviewPatchBuild).toHaveBeenCalledWith(
      "patch",
      "/Volumes/RAMPLE",
      expect.any(Function),
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
      expect(native.applyPatchBuild).toHaveBeenCalledWith(
        review,
        expect.any(Function),
      ),
    );
    expect(await screen.findByText("Building Patch…")).toBeTruthy();
    // Each file the Build handles is shown with its stage's progress.
    const report = vi.mocked(native.applyPatchBuild).mock.calls[0][1]!;
    act(() =>
      report({
        stage: "Writing to the destination",
        file: "A0/2_01_Snare.wav",
        done: 2,
        total: 4,
      }),
    );
    expect(
      screen.getByText("Writing to the destination: 3 of 4"),
    ).toBeTruthy();
    expect(screen.getByText("A0/2_01_Snare.wav")).toBeTruthy();
    const bar = screen.getByRole("progressbar", {
      name: "Writing to the destination",
    }) as HTMLProgressElement;
    expect([bar.value, bar.max]).toEqual([2, 4]);
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
    await openApp(buildable());
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
    await openApp(buildable());
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
    await openApp(buildable());
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
    await openApp(buildable());
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
    await openApp(buildable());
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
