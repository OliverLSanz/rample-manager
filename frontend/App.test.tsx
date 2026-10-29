import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import App from "./App";
import * as native from "./native";
import {
  basename,
  compatibilityWarnings,
  displaySample,
  idleTransport,
  type Snapshot,
  type Voice,
} from "./types";
vi.mock("./native", () => ({
  getSnapshot: vi.fn(),
  getTransport: vi.fn(),
  onLibraryChange: vi.fn(),
  act: vi.fn(),
  chooseFolder: vi.fn(),
  exportKit: vi.fn(),
  reveal: vi.fn(),
  convertImports: vi.fn(),
  showBackups: vi.fn(),
}));
let snapshot: Snapshot;
let update: (s: Snapshot) => void;
function fixture(): Snapshot {
  const voices: Voice[] = Array.from({ length: 4 }, (_, i) => ({
    kitID: "A0",
    samplePath: `/samples/A0/${i + 1}_sound.wav`,
    steps: Array.from({ length: 16 }, (_, s) => s % 4 === 0),
    volume: 0.8,
    muted: false,
    solo: false,
    layerMode: "manual",
    velocities: Array(16).fill(100),
  }));
  return {
    revision: 1,
    session: {
      schemaVersion: 2,
      rootPath: "/samples",
      selectedKit: "A0",
      bpm: 137,
      voices,
      kitConfigurations: { libraries: {} },
    },
    catalog: {
      unassigned: [],
      kits: [
        {
          id: "A0",
          bank: "A",
          path: "/samples/A0",
          warnings: [],
          samples: voices.map((v, i) => ({
            id: v.samplePath,
            name: `${i + 1}_sound`,
            voice: i + 1,
            revision: "1",
            duration: 0.5,
            rate: 44100,
            channels: 1,
            bits: 16,
            warnings: [],
            readable: true,
            needsConversion: false,
          })),
        },
      ],
      error: null,
    },
    waveforms: {},
    banks: voices.map((v) => [v.samplePath]),
    isDemo: false,
    watching: true,
    supportPath: "/support",
    message: null,
    playing: false,
    imports: [],
  };
}
beforeEach(() => {
  snapshot = fixture();
  vi.mocked(native.getSnapshot).mockImplementation(async () =>
    structuredClone(snapshot),
  );
  vi.mocked(native.getTransport).mockResolvedValue({
    ...idleTransport,
    sampleRate: 44100,
  });
  vi.mocked(native.onLibraryChange).mockImplementation(async (callback) => {
    update = callback;
    return () => {};
  });
  vi.mocked(native.act).mockImplementation(async (action) => {
    if (action.type === "setMode")
      snapshot.session.voices[action.channel].layerMode = action.mode;
    if (action.type === "dismissImports")
      snapshot.imports = snapshot.imports.filter(
        (s) =>
          !action.files.some(
            (f) => f.id === s.id && f.fingerprint === s.fingerprint,
          ),
      );
    if (action.type === "toggleStep")
      snapshot.session.voices[action.channel].steps[action.step] =
        !snapshot.session.voices[action.channel].steps[action.step];
    return structuredClone(snapshot);
  });
  vi.mocked(native.reveal).mockResolvedValue(undefined);
  vi.mocked(native.showBackups).mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
async function boot() {
  render(<App />);
  await screen.findByRole("heading", { name: "Your kit. In context." });
}
describe("desktop UI parity", () => {
  it("restores the session and renders four voices, single sample and 64 steps", async () => {
    await boot();
    expect(
      screen
        .getByRole("spinbutton", { name: "Tempo in BPM" })
        .getAttribute("value"),
    ).toBe("137");
    for (let i = 1; i <= 4; i++) {
      expect(
        (
          screen.getByLabelText(
            `Layer mode for voice ${i}`,
          ) as HTMLSelectElement
        ).value,
      ).toBe("manual");
    }
    expect(
      screen.getAllByRole("button", { name: /Voice \d, step \d/ }),
    ).toHaveLength(64);
    expect(screen.getByText("Live updates")).toBeTruthy();
  });
  it("sends step edits and all layer modes to the native backend", async () => {
    await boot();
    fireEvent.click(screen.getByLabelText("Voice 1, step 2"));
    await waitFor(() =>
      expect(native.act).toHaveBeenCalledWith({
        type: "toggleStep",
        channel: 0,
        step: 1,
      }),
    );
    for (const mode of [
      "random",
      "cyclic",
      "reverseCyclic",
      "velocity",
      "manual",
    ]) {
      fireEvent.change(screen.getByLabelText("Layer mode for voice 1"), {
        target: { value: mode },
      });
      await waitFor(() =>
        expect(native.act).toHaveBeenCalledWith({
          type: "setMode",
          channel: 0,
          mode,
        }),
      );
    }
  });
  it("normalizes the tempo field even when the saved tempo stays at its limit", async () => {
    snapshot.session.bpm = 300;
    await boot();
    const tempo = screen.getByRole("spinbutton", { name: "Tempo in BPM" });
    fireEvent.focus(tempo);
    fireEvent.change(tempo, { target: { value: "999" } });
    fireEvent.blur(tempo);
    expect((tempo as HTMLInputElement).value).toBe("300");
    await waitFor(() =>
      expect(native.act).toHaveBeenCalledWith({ type: "setBpm", value: 300 }),
    );
  });
  it("sends relative tempo adjustments for rapid button presses", async () => {
    await boot();
    fireEvent.click(screen.getByLabelText("Increase tempo"));
    fireEvent.click(screen.getByLabelText("Increase tempo"));
    await waitFor(() => expect(native.act).toHaveBeenCalledTimes(2));
    expect(vi.mocked(native.act).mock.calls.map(([action]) => action)).toEqual([
      { type: "adjustBpm", delta: 1 },
      { type: "adjustBpm", delta: 1 },
    ]);
  });
  it("handles transport keyboard shortcuts while leaving text fields alone", async () => {
    await boot();
    fireEvent.keyDown(window, { code: "Space", key: " " });
    await waitFor(() =>
      expect(native.act).toHaveBeenCalledWith({ type: "togglePlayback" }),
    );
    vi.mocked(native.act).mockClear();
    fireEvent.keyDown(screen.getByLabelText("Search kits or samples"), {
      code: "Space",
      key: " ",
    });
    expect(native.act).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: "3" });
    await waitFor(() =>
      expect(native.act).toHaveBeenCalledWith({ type: "audition", channel: 2 }),
    );
  });
  it("opens a velocity editor using the step context menu", async () => {
    await boot();
    fireEvent.contextMenu(screen.getByLabelText("Voice 2, step 5"));
    expect(
      screen.getByRole("dialog", { name: "Edit step velocity" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "32" }));
    await waitFor(() =>
      expect(native.act).toHaveBeenCalledWith({
        type: "setVelocity",
        channel: 1,
        step: 4,
        value: 32,
      }),
    );
    fireEvent.click(screen.getByText("Done"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
  it("updates selected-file removal from native filesystem events", async () => {
    await boot();
    const next = structuredClone(snapshot);
    next.catalog.kits[0].samples = next.catalog.kits[0].samples.filter(
      (s) => s.voice !== 1,
    );
    next.banks[0] = [];
    update(next);
    await screen.findByRole("option", { name: "Sample deleted" });
    expect(
      screen.getByLabelText("Audition voice 1").hasAttribute("disabled"),
    ).toBe(true);
  });
  it("exports through a native folder picker and never claims native SD mode sync", async () => {
    await boot();
    fireEvent.click(screen.getByRole("button", { name: "Export kit" }));
    expect(screen.getByRole("dialog", { name: "Export kit" })).toBeTruthy();
    expect(screen.getByText(/Layer modes stay on this computer/)).toBeTruthy();
    vi.mocked(native.chooseFolder).mockResolvedValue("/destination");
    vi.mocked(native.exportKit).mockResolvedValue("/destination/B0");
    fireEvent.click(
      screen.getByRole("button", { name: "Choose destination…" }),
    );
    await waitFor(() =>
      expect(native.exportKit).toHaveBeenCalledWith("B0", "/destination"),
    );
  });
});
describe("new sample review", () => {
  function candidate(convertible = true) {
    return {
      id: "/samples/A0/1_New.flac",
      fingerprint: "checked-version",
      name: "1_New.flac",
      kit: "A0",
      issues: ["Rample requires a .wav file."],
      convertible,
      target: "1_New.wav",
    };
  }
  it("opens automatically after a live file event and keeps originals without conversion", async () => {
    await boot();
    const next = structuredClone(snapshot);
    next.imports = [candidate()];
    snapshot = next;
    update(next);
    await screen.findByRole("dialog", { name: "New samples detected" });
    expect(native.convertImports).not.toHaveBeenCalled();
    expect(screen.getByText(/backed up before replacement/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep originals" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(native.act).toHaveBeenCalledWith({
      type: "dismissImports",
      files: [{ id: candidate().id, fingerprint: "checked-version" }],
    });
    expect(native.convertImports).not.toHaveBeenCalled();
  });
  it("converts only selected reviewed files after explicit consent and shows the result", async () => {
    snapshot.imports = [
      candidate(),
      {
        ...candidate(),
        id: "/samples/A0/2_Other.wav",
        name: "2_Other.wav",
        target: "2_Other.wav",
      },
    ];
    vi.mocked(native.convertImports).mockImplementation(async () => {
      const next = structuredClone(snapshot);
      next.revision = 2;
      next.imports = [snapshot.imports[1]];
      return {
        snapshot: next,
        backupPath: "/support/Conversion backups",
        results: [
          {
            name: "1_New.flac",
            target: "/samples/A0/1_New.wav",
            backup: "/backup/1_New.flac",
            error: null,
          },
        ],
      };
    });
    await boot();
    await screen.findByRole("dialog", { name: "New samples detected" });
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Convert A0/2_Other.wav" }),
    );
    expect(native.convertImports).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Convert and replace (1)" }),
    );
    await screen.findByText("Conversion complete");
    expect(native.convertImports).toHaveBeenCalledWith([
      { id: candidate().id, fingerprint: "checked-version" },
    ]);
    expect(screen.getByText("Saved as 1_New.wav")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show backups" }));
    expect(native.showBackups).toHaveBeenCalled();
  });
  it("shows unfixable files and blocks accidental transport shortcuts", async () => {
    snapshot.imports = [candidate(false)];
    await boot();
    await screen.findByRole("dialog", { name: "New samples detected" });
    expect(
      (
        screen.getByRole("button", {
          name: "Convert and replace (0)",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText(/Manual fix required/)).toBeTruthy();
    fireEvent.keyDown(window, { code: "Space", key: " " });
    expect(native.act).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Close sample review" }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
  it("keeps focus inside the modal while every control is disabled during conversion", async () => {
    snapshot.imports = [candidate()];
    let finish!: (reply: import("./types").ConversionReply) => void;
    vi.mocked(native.convertImports).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    await boot();
    await screen.findByRole("dialog", { name: "New samples detected" });
    fireEvent.click(
      screen.getByRole("button", { name: "Convert and replace (1)" }),
    );
    await screen.findByRole("button", { name: "Converting…" });
    expect(fireEvent.keyDown(window, { key: "Tab", cancelable: true })).toBe(
      false,
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeTruthy();
    const next = structuredClone(snapshot);
    next.imports = [];
    finish({ snapshot: next, results: [], backupPath: "/backups" });
    await screen.findByText("Conversion complete");
  });
  it("keeps conversion failures visible instead of dismissing the review", async () => {
    snapshot.imports = [candidate()];
    vi.mocked(native.convertImports).mockRejectedValue(
      "The source changed during conversion.",
    );
    await boot();
    await screen.findByRole("dialog", { name: "New samples detected" });
    fireEvent.click(
      screen.getByRole("button", { name: "Convert and replace (1)" }),
    );
    await screen.findByRole("alert");
    expect(
      screen.getByText("The source changed during conversion."),
    ).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
describe("cross-platform view models", () => {
  it("handles macOS and Windows paths", () => {
    expect(basename("/Volumes/Rample/A0")).toBe("A0");
    expect(basename("C:\\Users\\Oliver\\Samples")).toBe("Samples");
  });
  it("shows the actually triggered layer without changing the saved selection", () => {
    const s = fixture();
    s.session.voices[0].layerMode = "cyclic";
    s.banks[0] = ["/samples/A0/1_other.wav"];
    s.catalog.kits[0].samples.push({
      ...s.catalog.kits[0].samples[0],
      id: s.banks[0][0],
      name: "1_other",
    });
    expect(displaySample(s, 0, 0)?.name).toBe("1_other");
    expect(s.session.voices[0].samplePath).toBe("/samples/A0/1_sound.wav");
  });
  it("collects warnings from mixed source kits and active layers", () => {
    const s = fixture();
    s.catalog.kits[0].warnings = ["Kit warning"];
    s.catalog.kits[0].samples[0].warnings = ["Bad rate"];
    expect(compatibilityWarnings(s)).toEqual(["Kit warning", "Bad rate"]);
  });
});
