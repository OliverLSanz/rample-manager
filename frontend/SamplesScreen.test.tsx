import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SamplesScreen } from "./SamplesScreen";
import * as native from "./native";
import type { LibrarySample, PatchKit, PatchSnapshot } from "./types";

vi.mock("./native", () => ({
  listSamples: vi.fn(),
  retireSample: vi.fn(),
  restoreSample: vi.fn(),
  updateSampleMetadata: vi.fn(),
  replaceSampleTag: vi.fn(),
  setSampleTags: vi.fn(),
  openSampleAudition: vi.fn(),
  chooseSampleSources: vi.fn(),
  importSamples: vi.fn(),
  chooseSampleExport: vi.fn(),
  exportSample: vi.fn(),
  auditionSample: vi.fn(),
  stopSampleAudition: vi.fn(),
  sampleWaveform: vi.fn(),
}));

function sample(id: string, name: string, extra: Partial<LibrarySample> = {}) {
  return {
    id,
    originalId: `original-${id}`,
    name,
    tags: [] as string[],
    sha256: id,
    duration: 0.1,
    rate: 44100,
    bits: 16,
    channels: 1,
    format: "WAV",
    retired: false,
    readable: true,
    error: null,
    ...extra,
  };
}
function kit(id: string, bank: string, number: number, layers: string[][]) {
  return {
    id,
    bank,
    number,
    voices: [0, 1, 2, 3].map((c) => ({
      layers: (layers[c] ?? []).map((s, i) => ({
        id: `${id}-${c}-${i}`,
        sampleId: s,
      })),
      selectedLayer: "",
      mode: "manual" as const,
    })),
  } satisfies PatchKit;
}
/** "Live" is open on A0; "Studio" is another Patch with one Kit. */
function view(): PatchSnapshot {
  const patch = (id: string, name: string, kits: PatchKit[]) => ({
    id,
    name,
    selectedKit: kits[0]?.id ?? "",
    kits,
    bpm: 120,
    voices: [],
  });
  return {
    graph: {
      selectedPatch: "live",
      patches: [
        patch("live", "Live", [
          kit("a0", "A", 0, [["kick"], ["snare", "kick"]]),
          kit("b2", "B", 2, [[], [], ["kick"]]),
        ]),
        patch("studio", "Studio", [kit("c1", "C", 1, [[], ["snare"]])]),
      ],
    },
    session: null,
    samples: [],
    validation: [],
    playing: false,
  };
}
const onPatch = vi.fn();
const onOpen = vi.fn();
const onChanged = vi.fn();
function show(rows: LibrarySample[], snapshot = view()) {
  vi.mocked(native.listSamples).mockResolvedValue(rows);
  return render(
    <SamplesScreen
      view={snapshot}
      onPatch={onPatch}
      onOpen={onOpen}
      onChanged={onChanged}
    />,
  );
}
const list = () => screen.findByRole("listbox", { name: "Library Samples" });
const names = () =>
  within(screen.getByRole("listbox"))
    .getAllByRole("option")
    .map((o) => o.getAttribute("aria-label"));

beforeEach(() => {
  vi.mocked(native.openSampleAudition).mockResolvedValue("owner");
  vi.mocked(native.auditionSample).mockResolvedValue();
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  vi.mocked(native.sampleWaveform).mockResolvedValue(Array(100).fill(0.5));
  onPatch.mockResolvedValue(true);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("imports files, reports failures and auditions by identity", async () => {
  const kick = sample("sample-kick", "My kick");
  show([]);
  await screen.findByText(/No Samples yet/);
  vi.mocked(native.chooseSampleSources).mockResolvedValue([
    "/source/My kick.wav",
    "/source/Broken.wav",
  ]);
  vi.mocked(native.importSamples).mockResolvedValue({
    imported: 1,
    recognized: 0,
    failed: 1,
    complete: false,
    samples: [kick],
    results: [
      {
        source: "/source/My kick.wav",
        status: "imported",
        sample: kick,
        error: null,
      },
      {
        source: "/source/Broken.wav",
        status: "failed",
        sample: null,
        error: "Truncated WAV chunk",
      },
    ],
  });
  fireEvent.click(screen.getByRole("button", { name: /Import audio files/ }));
  fireEvent.click(await screen.findByRole("option", { name: "My kick" }));
  expect(screen.getByText(/1 imported · 0 recognized · 1 failed/)).toBeTruthy();
  expect(screen.getByText(/Import incomplete/)).toBeTruthy();
  expect(screen.getByText(/Broken.wav: Truncated WAV chunk/)).toBeTruthy();
  expect(onChanged).toHaveBeenCalled();
  fireEvent.click(
    screen.getByRole("button", { name: "Audition Original My kick" }),
  );
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenLastCalledWith(
      "sample-kick",
      "owner",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "Stop audition" }));
  await waitFor(() =>
    expect(native.stopSampleAudition).toHaveBeenCalledWith("owner"),
  );
  vi.mocked(native.chooseSampleSources).mockResolvedValue(["/renamed-folder"]);
  vi.mocked(native.importSamples).mockResolvedValue({
    imported: 0,
    recognized: 1,
    failed: 0,
    complete: true,
    samples: [kick],
    results: [
      {
        source: "/renamed-folder/Renamed.wav",
        status: "recognized",
        sample: kick,
        error: null,
      },
    ],
  });
  fireEvent.click(screen.getByRole("button", { name: /Import folder/ }));
  await screen.findByText(/0 imported · 1 recognized · 0 failed/);
  expect(native.chooseSampleSources).toHaveBeenLastCalledWith(true);
  expect(names()).toEqual(["My kick"]);
});

it("closing an old screen cannot stop audition in a newly opened one", async () => {
  vi.mocked(native.openSampleAudition)
    .mockResolvedValueOnce("owner-a")
    .mockResolvedValueOnce("owner-b");
  let finish!: () => void;
  vi.mocked(native.auditionSample)
    .mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValue();
  const old = show([sample("kick", "Kick")]);
  fireEvent.click(await screen.findByRole("button", { name: "Hear Kick" }));
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith("kick", "owner-a", true),
  );
  old.unmount();
  expect(native.stopSampleAudition).toHaveBeenCalledWith("owner-a", true);
  show([sample("kick", "Kick")]);
  fireEvent.click(await screen.findByRole("button", { name: "Hear Kick" }));
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith("kick", "owner-b", true),
  );
  finish();
  await Promise.resolve();
  expect(native.stopSampleAudition).toHaveBeenCalledTimes(1);
});

it("edits a Sample's name and tags, searches them and keeps duplicates apart by ID", async () => {
  const first = sample("first", "Same name", { tags: ["dry"] });
  const second = sample("second", "Same name", { tags: ["warm"] });
  const saved = {
    ...second,
    name: "Warm kick",
    tags: ["café / warm, soft", "低音"],
  };
  vi.mocked(native.updateSampleMetadata).mockResolvedValue(saved);
  show([first, second]);
  const rows = within(await list()).getAllByRole("option");
  fireEvent.click(rows[1]);
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith("second", "owner", true),
  );
  const details = screen.getByRole("complementary", { name: "Sample details" });
  expect(
    (
      within(details).getByRole("textbox", {
        name: "Sample name",
      }) as HTMLInputElement
    ).value,
  ).toBe("Same name");
  fireEvent.change(
    within(details).getByRole("textbox", { name: "Sample name" }),
    {
      target: { value: "Warm kick" },
    },
  );
  fireEvent.change(
    within(details).getByRole("textbox", { name: "Tags (one per line)" }),
    { target: { value: "café / warm, soft\n低音" } },
  );
  fireEvent.click(
    within(details).getByRole("button", { name: "Save metadata" }),
  );
  await screen.findByText("Metadata saved.");
  expect(native.updateSampleMetadata).toHaveBeenCalledWith(
    "second",
    "Warm kick",
    ["café / warm, soft", "低音"],
  );
  expect(onChanged).toHaveBeenCalled();
  expect(
    screen
      .getByRole("option", { name: "Warm kick" })
      .getAttribute("aria-selected"),
  ).toBe("true");
  fireEvent.change(
    screen.getByRole("searchbox", { name: "Search Samples by name or tag" }),
    { target: { value: "低音" } },
  );
  expect(names()).toEqual(["Warm kick"]);
  // Tags filter too.
  fireEvent.change(
    screen.getByRole("searchbox", { name: "Search Samples by name or tag" }),
    { target: { value: "" } },
  );
  fireEvent.click(screen.getByRole("button", { name: /^#dry/ }));
  expect(names()).toEqual(["Same name"]);
});

it("shows source format and plays the Original or the mono Rample sound", async () => {
  show([
    sample("stereo", "Stereo room", {
      duration: 1.2,
      rate: 48000,
      channels: 2,
      bits: 24,
      format: "FLAC",
    }),
  ]);
  fireEvent.click(await screen.findByRole("option", { name: "Stereo room" }));
  expect(screen.getByText(/FLAC.*48000 Hz.*24-bit.*stereo/)).toBeTruthy();
  await waitFor(() =>
    expect(native.sampleWaveform).toHaveBeenCalledWith("stereo"),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Audition Original Stereo room" }),
  );
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenLastCalledWith("stereo", "owner"),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Audition Rample Stereo room" }),
  );
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenLastCalledWith(
      "stereo",
      "owner",
      true,
    ),
  );
});

it("stops while audio is preparing and ignores its later completion", async () => {
  let finish!: () => void;
  vi.mocked(native.auditionSample).mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  show([sample("long", "Long stereo", { duration: 600 })]);
  fireEvent.click(await screen.findByRole("option", { name: "Long stereo" }));
  await waitFor(() => expect(native.auditionSample).toHaveBeenCalled());
  const stop = screen.getByRole("button", {
    name: "Stop audition",
  }) as HTMLButtonElement;
  expect(stop.disabled).toBe(false);
  fireEvent.click(stop);
  await waitFor(() =>
    expect(native.stopSampleAudition).toHaveBeenCalledWith("owner"),
  );
  finish();
  await Promise.resolve();
  await waitFor(() => expect(stop.disabled).toBe(true));
});

it("retires a Sample into Trash and restores it with the same identity", async () => {
  const kick = sample("keep-identity", "Warm kick", { tags: ["warm"] });
  vi.mocked(native.retireSample).mockResolvedValue({ ...kick, retired: true });
  vi.mocked(native.restoreSample).mockResolvedValue(kick);
  show([kick]);
  fireEvent.click(await screen.findByRole("option", { name: "Warm kick" }));
  fireEvent.click(screen.getByRole("button", { name: "Retire Warm kick" }));
  await screen.findByText(/No Samples match/);
  expect(screen.getByRole("button", { name: /Trash 1/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Trash/ }));
  expect(
    within(
      await screen.findByRole("listbox", { name: "Trash Samples" }),
    ).getByRole("option", { name: "Warm kick" }),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("option", { name: "Warm kick" }));
  // A retired Sample cannot be added to a Kit.
  expect(screen.queryByRole("button", { name: /Add as Layer/ })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Restore Warm kick" }));
  await screen.findByText("Trash is empty.");
  expect(native.restoreSample).toHaveBeenCalledWith("keep-identity");
  expect(onChanged).toHaveBeenCalledTimes(2);
});

it("reports recognized retired audio and offers restoring it instead of a duplicate", async () => {
  const known = sample("retired-known", "My retired kick", { retired: true });
  vi.mocked(native.chooseSampleSources).mockResolvedValue([
    "/renamed/Again.wav",
  ]);
  vi.mocked(native.importSamples).mockResolvedValue({
    imported: 0,
    recognized: 1,
    failed: 0,
    complete: true,
    samples: [known],
    results: [
      {
        source: "/renamed/Again.wav",
        status: "recognized",
        sample: known,
        error: null,
      },
    ],
  });
  vi.mocked(native.restoreSample).mockResolvedValue({
    ...known,
    retired: false,
  });
  show([known]);
  await screen.findByText(/No Samples match/);
  fireEvent.click(screen.getByRole("button", { name: /Import audio files/ }));
  const results = await screen.findByRole("list", { name: "Import results" });
  expect(
    within(results).getByText(/Already in Trash: My retired kick/),
  ).toBeTruthy();
  fireEvent.click(
    within(results).getByRole("button", { name: "Restore My retired kick" }),
  );
  await screen.findByRole("option", { name: "My retired kick" });
  expect(screen.getByRole("button", { name: /Trash 0/ })).toBeTruthy();
});

it("exports the selected Sample through the destination picker", async () => {
  vi.mocked(native.chooseSampleExport).mockResolvedValue("/tmp/Kick.wav");
  vi.mocked(native.exportSample).mockResolvedValue();
  show([sample("kick", "Kick")]);
  fireEvent.click(await screen.findByRole("option", { name: "Kick" }));
  fireEvent.click(screen.getByRole("button", { name: "Export Kick" }));
  await waitFor(() =>
    expect(native.exportSample).toHaveBeenCalledWith("kick", "/tmp/Kick.wav"),
  );
  expect(await screen.findByText("Sample exported.")).toBeTruthy();
});

it("filters by the open Patch, unused Samples and Trash, and shows where each one plays", async () => {
  show([
    sample("kick", "Kick"),
    sample("snare", "Snare"),
    sample("hat", "Hat"),
    sample("old", "Old", { retired: true }),
  ]);
  await list();
  const filters = screen.getByRole("navigation", { name: "Sample filters" });
  const count = (name: RegExp) =>
    within(filters).getByRole("button", { name }).textContent;
  expect(count(/All Samples/)).toBe("All Samples3");
  expect(count(/In Live/)).toBe("In Live2");
  expect(count(/Not in any Kit/)).toBe("Not in any Kit1");
  expect(count(/Trash/)).toBe("Trash1");
  const kick = screen.getByRole("option", { name: "Kick" });
  expect(within(kick).getByText("A0·V1")).toBeTruthy();
  expect(within(kick).getByText("A0·V2")).toBeTruthy();
  expect(within(kick).getByText("B2·V3")).toBeTruthy();
  expect(
    within(screen.getByRole("option", { name: "Snare" })).getByText(
      "1 other Patch",
    ),
  ).toBeTruthy();
  fireEvent.click(
    within(filters).getByRole("button", { name: /Not in any Kit/ }),
  );
  expect(names()).toEqual(["Hat"]);
  fireEvent.click(within(filters).getByRole("button", { name: /In Live/ }));
  expect(names()).toEqual(["Kick", "Snare"]);
  fireEvent.click(screen.getByRole("option", { name: "Snare" }));
  const used = screen.getByRole("region", { name: "Used in" });
  fireEvent.click(
    within(used).getByRole("button", {
      name: "Open Studio C1 · Voice 2 · Layer 1",
    }),
  );
  expect(onOpen).toHaveBeenCalledWith(
    expect.objectContaining({ patch: "studio", kit: "c1", channel: 1 }),
  );
});

it("adds to the open Kit by default, or to any Kit of any Patch", async () => {
  show([sample("hat", "Hat")]);
  fireEvent.click(await screen.findByRole("option", { name: "Hat" }));
  const target = screen.getByRole("group", { name: "Add to a Kit" });
  expect(
    (
      within(target).getByRole("combobox", {
        name: "Patch",
      }) as HTMLSelectElement
    ).value,
  ).toBe("live");
  expect(
    (within(target).getByRole("combobox", { name: "Kit" }) as HTMLSelectElement)
      .value,
  ).toBe("a0");
  fireEvent.click(within(target).getByRole("radio", { name: "Voice 3" }));
  fireEvent.click(screen.getByRole("button", { name: /Add as Layer/ }));
  await waitFor(() =>
    expect(onPatch).toHaveBeenLastCalledWith({
      type: "addLayerTo",
      patch: "live",
      kit: "a0",
      channel: 2,
      sample: "hat",
    }),
  );
  expect(await screen.findByText("Added Hat to Kit A0, Voice 3.")).toBeTruthy();
  fireEvent.change(within(target).getByRole("combobox", { name: "Patch" }), {
    target: { value: "studio" },
  });
  expect(
    (within(target).getByRole("combobox", { name: "Kit" }) as HTMLSelectElement)
      .value,
  ).toBe("c1");
  fireEvent.click(screen.getByRole("button", { name: /Add as Layer/ }));
  await waitFor(() =>
    expect(onPatch).toHaveBeenLastCalledWith({
      type: "addLayerTo",
      patch: "studio",
      kit: "c1",
      channel: 2,
      sample: "hat",
    }),
  );
  expect(
    await screen.findByText("Added Hat to Kit C1, Voice 3 of Studio."),
  ).toBeTruthy();
});

it("selects several Samples to add, tag or retire them together", async () => {
  const rows = [
    sample("a", "A"),
    sample("b", "B", { tags: ["kept"] }),
    sample("c", "C"),
  ];
  vi.mocked(native.updateSampleMetadata).mockImplementation(
    async (id, name, tags) => ({
      ...rows.find((r) => r.id === id)!,
      name,
      tags,
    }),
  );
  vi.mocked(native.retireSample).mockImplementation(async (id) => ({
    ...rows.find((r) => r.id === id)!,
    retired: true,
  }));
  show(rows);
  fireEvent.click(await screen.findByRole("option", { name: "A" }));
  fireEvent.click(screen.getByRole("option", { name: "C" }), {
    shiftKey: true,
  });
  expect(screen.getByText("3 Samples selected")).toBeTruthy();
  fireEvent.click(screen.getByRole("option", { name: "B" }), { metaKey: true });
  expect(screen.getByText("2 Samples selected")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Add 2 as Layers" }));
  await waitFor(() => expect(onPatch).toHaveBeenCalledTimes(2));
  expect(onPatch.mock.calls.map(([a]) => a.sample)).toEqual(["a", "c"]);
  expect(
    await screen.findByText("Added 2 Samples to Kit A0, Voice 1."),
  ).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox", { name: "Tag to add" }), {
    target: { value: "kept" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add tag" }));
  await screen.findByText("Tagged 2 Samples #kept.");
  expect(native.updateSampleMetadata).toHaveBeenCalledWith("a", "A", ["kept"]);
  expect(native.updateSampleMetadata).toHaveBeenCalledWith("c", "C", ["kept"]);
  fireEvent.click(screen.getByRole("button", { name: "Move 2 to Trash" }));
  await waitFor(() => expect(names()).toEqual(["B"]));
  expect(native.retireSample).toHaveBeenCalledTimes(2);
});

it("removes a tag from selected Samples and offers Undo", async () => {
  const rows = [
    sample("a", "A", { tags: ["kick", "dry"] }),
    sample("b", "B", { tags: ["kick"] }),
    sample("c", "C", { tags: ["kick"] }),
  ];
  vi.mocked(native.replaceSampleTag).mockResolvedValue([
    { ...rows[0], tags: ["dry"] },
    { ...rows[1], tags: [] },
  ]);
  vi.mocked(native.setSampleTags).mockResolvedValue(rows.slice(0, 2));
  show(rows);
  fireEvent.click(await screen.findByRole("option", { name: "A" }));
  fireEvent.click(screen.getByRole("option", { name: "B" }), {
    shiftKey: true,
  });
  const inSelection = screen.getByRole("list", { name: "Tags in selection" });
  expect(within(inSelection).getByText("2 of 2")).toBeTruthy();
  expect(within(inSelection).getByText("1 of 2")).toBeTruthy();
  fireEvent.click(
    screen.getByRole("button", {
      name: "Remove #kick from 2 selected Samples",
    }),
  );
  await screen.findByText("Removed #kick from 2 Samples.");
  expect(native.replaceSampleTag).toHaveBeenCalledWith("kick", null, [
    "a",
    "b",
  ]);
  // C keeps the tag, so the filter still counts it.
  const filters = screen.getByRole("navigation", { name: "Sample filters" });
  expect(within(filters).getByRole("button", { name: "#kick 1" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  await screen.findByText("Tag change undone.");
  expect(native.setSampleTags).toHaveBeenCalledWith([
    { id: "a", tags: ["kick", "dry"] },
    { id: "b", tags: ["kick"] },
  ]);
  expect(within(filters).getByRole("button", { name: "#kick 3" })).toBeTruthy();
});

it("renames, merges and deletes a tag across the Library from its filter", async () => {
  const user = userEvent.setup();
  const rows = [
    sample("a", "A", { tags: ["kick"] }),
    sample("b", "B", { tags: ["Kick", "kick"] }),
    sample("c", "C", { tags: ["snare"] }),
  ];
  show(rows);
  const filters = await screen.findByRole("navigation", {
    name: "Sample filters",
  });
  fireEvent.click(within(filters).getByRole("button", { name: "#kick 2" }));
  expect(names()).toEqual(["A", "B"]);

  vi.mocked(native.replaceSampleTag).mockResolvedValueOnce([
    { ...rows[0], tags: ["Kick"] },
    { ...rows[1], tags: ["Kick"] },
  ]);
  await user.click(screen.getByRole("button", { name: "Rename tag #kick" }));
  const input = screen.getByRole("textbox", { name: "New name for #kick" });
  await user.clear(input);
  await user.type(input, "Kick{Enter}");
  await screen.findByText("Merged #kick into #Kick.");
  expect(native.replaceSampleTag).toHaveBeenLastCalledWith(
    "kick",
    "Kick",
    null,
  );
  // The filter follows the merged tag.
  expect(
    within(filters)
      .getByRole("button", { name: "#Kick 2" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  expect(names()).toEqual(["A", "B"]);

  // Escape cancels a rename without calling the Library.
  await user.click(screen.getByRole("button", { name: "Rename tag #snare" }));
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("textbox", { name: "New name for #snare" })).toBe(
    null,
  );
  expect(native.replaceSampleTag).toHaveBeenCalledTimes(1);

  vi.mocked(native.replaceSampleTag).mockResolvedValueOnce([
    { ...rows[2], tags: ["hat"] },
  ]);
  await user.click(screen.getByRole("button", { name: "Rename tag #snare" }));
  await user.type(
    screen.getByRole("textbox", { name: "New name for #snare" }),
    "{Control>}a{/Control}{Backspace}hat{Enter}",
  );
  await screen.findByText("Renamed #snare to #hat.");

  vi.mocked(native.replaceSampleTag).mockResolvedValueOnce([
    { ...rows[0], tags: [] },
    { ...rows[1], tags: [] },
  ]);
  await user.click(screen.getByRole("button", { name: "Delete tag #Kick" }));
  await screen.findByText("Deleted #Kick from 2 Samples.");
  expect(native.replaceSampleTag).toHaveBeenLastCalledWith("Kick", null, null);
  expect(within(filters).queryByRole("button", { name: /#Kick/ })).toBe(null);
  expect(names()).toEqual(["A", "B", "C"]);
  expect(onChanged).toHaveBeenCalledTimes(3);
});

it("browses with the arrow keys, hearing each Sample, and adds with Enter", async () => {
  const user = userEvent.setup();
  show([
    sample("a", "A"),
    sample("b", "B"),
    sample("c", "C", { readable: false }),
  ]);
  const box = await list();
  box.focus();
  await user.keyboard("{ArrowDown}");
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenLastCalledWith("a", "owner", true),
  );
  await user.keyboard("{ArrowDown}");
  expect(
    screen.getByRole("option", { name: "B" }).getAttribute("aria-selected"),
  ).toBe("true");
  await user.keyboard("{Enter}");
  await waitFor(() =>
    expect(onPatch).toHaveBeenLastCalledWith(
      expect.objectContaining({ type: "addLayerTo", sample: "b" }),
    ),
  );
  // Unreadable Samples are selectable but never auditioned or added.
  vi.mocked(native.auditionSample).mockClear();
  await user.keyboard("{ArrowDown}{Enter}");
  expect(native.auditionSample).not.toHaveBeenCalled();
  expect(onPatch).toHaveBeenCalledTimes(1);
  await user.keyboard("{Shift>}{ArrowUp}{/Shift}");
  expect(screen.getByText("2 Samples selected")).toBeTruthy();
});
