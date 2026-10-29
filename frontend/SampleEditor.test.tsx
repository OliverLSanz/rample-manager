import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SampleEditor } from "./SampleEditor";
import * as native from "./native";
import { idleTransport, type EditorInfo, type Snapshot } from "./types";
vi.mock("./native", () => ({
  previewEditor: vi.fn(),
  saveEditor: vi.fn(),
  closeEditor: vi.fn(),
  auditionEditor: vi.fn(),
  showEditBackups: vi.fn(),
}));
const original: EditorInfo = {
  id: "edit-1",
  channel: 0,
  path: "/SD/A0/1_Kick.wav",
  fingerprint: "original",
  duration: 1,
  waveform: [0.1, 0.5, 1, 0.4],
  settings: {
    start: 0,
    end: 1,
    gainDb: 0,
    pitch: 0,
    lowDb: 0,
    midDb: 0,
    highDb: 0,
    echoMix: 0,
    echoMs: 250,
    echoFeedback: 0.35,
    reverbMix: 0,
    reverbSeconds: 1.5,
  },
};
const onClose = vi.fn(),
  onSaved = vi.fn(),
  onToggle = vi.fn();
function boot(invalidated = false) {
  return render(
    <SampleEditor
      initial={structuredClone(original)}
      transport={idleTransport}
      invalidated={invalidated}
      onClose={onClose}
      onSaved={onSaved}
      onToggle={onToggle}
    />,
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(native.closeEditor).mockResolvedValue();
  vi.mocked(native.auditionEditor).mockResolvedValue();
  vi.mocked(native.previewEditor).mockImplementation(
    async (_id, sequence, settings) => ({
      sequence,
      duration: settings.end - settings.start,
    }),
  );
});
afterEach(cleanup);
describe("sample editor", () => {
  it("previews edits without saving and discards them on close", async () => {
    boot();
    fireEvent.change(screen.getByRole("slider", { name: "Gain" }), {
      target: { value: "-6" },
    });
    await waitFor(() =>
      expect(native.previewEditor).toHaveBeenCalledWith(
        "edit-1",
        1,
        expect.objectContaining({ gainDb: -6 }),
        true,
      ),
    );
    expect(native.saveEditor).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Discard edits" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(native.closeEditor).toHaveBeenCalledWith("edit-1");
    expect(native.saveEditor).not.toHaveBeenCalled();
  });
  it("bakes the latest settings only after Save, then resets controls for the baked WAV", async () => {
    const next = {
      ...structuredClone(original),
      id: "edit-2",
      duration: 0.5,
      settings: { ...original.settings, end: 0.5 },
    };
    vi.mocked(native.saveEditor).mockResolvedValue({
      snapshot: { message: null } as Snapshot,
      editor: next,
      backup: "/support/Sample edit backups/original.wav",
      rendered: { duration: 0.5, frames: 22050, peak: 0.5, clippedFrames: 0 },
    });
    boot();
    fireEvent.change(screen.getByRole("slider", { name: "Pitch" }), {
      target: { value: "12" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save to sample" }));
    await screen.findByText("Sample saved. Original backed up.");
    expect(native.saveEditor).toHaveBeenCalledWith(
      "edit-1",
      expect.objectContaining({ pitch: 12 }),
    );
    expect(onSaved).toHaveBeenCalled();
    expect(
      (screen.getByRole("slider", { name: "Pitch" }) as HTMLInputElement).value,
    ).toBe("0");
    expect(
      (
        screen.getByRole("button", {
          name: "Save to sample",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });
  it("supports original/edited comparison and exact sample audition", async () => {
    boot();
    fireEvent.change(screen.getByRole("slider", { name: "Echo amount" }), {
      target: { value: "30" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: "Preview edits" }));
    await waitFor(() =>
      expect(native.previewEditor).toHaveBeenLastCalledWith(
        "edit-1",
        2,
        expect.objectContaining({ echoMix: 0.3 }),
        false,
      ),
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Save to sample",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Audition sample" }));
    expect(native.auditionEditor).toHaveBeenCalledWith("edit-1");
    fireEvent.click(screen.getByRole("button", { name: "Play sequence" }));
    expect(onToggle).toHaveBeenCalled();
  });
  it("blocks saving when the source changes externally", () => {
    boot(true);
    expect(screen.getByRole("alert").textContent).toContain("source changed");
    expect(
      (
        screen.getByRole("slider", { name: "Gain" }) as HTMLInputElement
      ).matches(":disabled"),
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Save to sample" }));
    expect(native.saveEditor).not.toHaveBeenCalled();
  });
  it("preserves edits and shows a save failure for retry", async () => {
    vi.mocked(native.saveEditor).mockRejectedValue(
      "The source changed before replacement.",
    );
    boot();
    fireEvent.change(screen.getByRole("slider", { name: "Gain" }), {
      target: { value: "-3" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save to sample" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("alert").textContent).toContain("source changed");
    expect(
      (screen.getByRole("slider", { name: "Gain" }) as HTMLInputElement).value,
    ).toBe("-3");
    expect(onClose).not.toHaveBeenCalled();
  });
  it("accepts negative gain and decimal trim times without committing partial keystrokes", async () => {
    const user = userEvent.setup();
    boot();
    const start = screen.getByRole("spinbutton", { name: "Start time" });
    await user.clear(start);
    await user.type(start, "0.05");
    expect(native.previewEditor).not.toHaveBeenCalled();
    await user.tab();
    const end = screen.getByRole("spinbutton", { name: "End time" });
    await user.clear(end);
    await user.type(end, "0.3");
    await user.tab();
    const gain = screen.getByRole("spinbutton", { name: "Gain value" });
    await user.clear(gain);
    await user.type(gain, "-6");
    await user.keyboard("{Enter}");
    await waitFor(() =>
      expect(native.previewEditor).toHaveBeenLastCalledWith(
        "edit-1",
        expect.any(Number),
        expect.objectContaining({ start: 0.05, end: 0.3, gainDb: -6 }),
        true,
      ),
    );
    expect((start as HTMLInputElement).value).toBe("0.05");
    expect((end as HTMLInputElement).value).toBe("0.3");
    expect((gain as HTMLInputElement).value).toBe("-6");
  });
});
