import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SdImport } from "./SdImport";
import * as native from "./native";
vi.mock("./native", () => ({
  chooseFolder: vi.fn(),
  prepareSdImport: vi.fn(),
  finishSdImport: vi.fn(),
  restoreSample: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
it("keeps copied audio and draft errors visible while requiring an explicit changed-arrangement choice", async () => {
  vi.mocked(native.chooseFolder).mockResolvedValue("/SD");
  vi.mocked(native.prepareSdImport).mockResolvedValue({
    id: "intake",
    status: "changed",
    recognizedPatch: null,
    nativeVerified: false,
    candidates: [{ id: "existing", name: "Live set" }],
    audio: {
      imported: 1,
      recognized: 2,
      failed: 1,
      complete: false,
      samples: [],
      results: [
        {
          source: "/SD/A0/1_bad.wav",
          status: "failed",
          sample: null,
          error: "Truncated WAV",
        },
      ],
    },
    draft: {
      id: "draft",
      name: "SD",
      selectedKit: "",
      kits: [],
      bpm: 120,
      voices: [],
      importedSd: {
        intakeId: "intake",
        copyComplete: false,
        fingerprint: "hash",
        files: [
          {
            path: "_save/A0.rpl",
            sha256: "native",
            kind: "native",
            sampleId: null,
          },
        ],
        issues: ["Odd folder/1_Kick.wav: Invalid Kit folder"],
        unassigned: [],
        native: {
          verifiedFirmware: "2.00",
          defaultLayerMode: "velocity",
          kits: [
            {
              bank: "A",
              number: 0,
              source: "store",
              voices: [],
              issues: [
                "A0 Voice 1: Stored Layer mode value 7 is not a verified mode. The app shows MANUAL.",
              ],
            },
            {
              bank: "A",
              number: 1,
              source: "appDefault",
              voices: [],
              issues: [],
            },
          ],
        },
      },
    },
  });
  vi.mocked(native.finishSdImport).mockRejectedValueOnce(
    "Storage is unavailable",
  );
  render(<SdImport onClose={() => {}} onImported={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Choose SD folder…" }));
  await screen.findByText(/1 new Sample.*2 recognized.*1 failed/);
  expect(screen.getByText(/Invalid Kit folder/)).toBeTruthy();
  expect(screen.getByText(/Truncated WAV/)).toBeTruthy();
  expect(
    screen.getByText(
      /restored for 1 Kit\(s\) saved with STORE\. 1 Kit\(s\) without a stored Kit file use your default Layer mode \(Velocity\)/,
    ),
  ).toBeTruthy();
  expect(
    screen.getByText(
      "These mappings were verified on Rample firmware 2.00. The firmware of this card is not detected.",
    ),
  ).toBeTruthy();
  expect(screen.getByText(/1 native file\(s\) preserved exactly/)).toBeTruthy();
  expect(screen.getByText(/Stored Layer mode value 7/)).toBeTruthy();
  expect(screen.getByText(/never writes native settings/)).toBeTruthy();
  expect(native.finishSdImport).not.toHaveBeenCalled();
  fireEvent.click(
    screen.getByRole("button", { name: "Update existing Patch" }),
  );
  await screen.findByRole("alert");
  expect(native.finishSdImport).toHaveBeenCalledWith("intake", {
    type: "update",
    patch: "existing",
  });
  expect(screen.getByRole("button", { name: "Create new Patch" })).toBeTruthy();
});
it("shows folder-copy failures without losing the retry route", async () => {
  vi.mocked(native.chooseFolder).mockResolvedValue("/SD");
  vi.mocked(native.prepareSdImport).mockRejectedValue(
    "SD is no longer mounted",
  );
  render(<SdImport onClose={() => {}} onImported={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Choose SD folder…" }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "no longer mounted",
  );
  expect(
    screen
      .getByRole("button", { name: "Choose SD folder…" })
      .hasAttribute("disabled"),
  ).toBe(false);
});
it("reuses an unchanged Patch and restores a recognized Trash Sample by its original identity", async () => {
  const sample = {
    id: "retired",
    originalId: "original",
    format: "WAV",
    name: "User kick",
    tags: ["kept"],
    sha256: "hash",
    duration: 0.1,
    rate: 44100,
    channels: 1,
    bits: 16,
    readable: true,
    error: null,
    retired: true,
  };
  vi.mocked(native.chooseFolder).mockResolvedValue("/other mount");
  vi.mocked(native.prepareSdImport).mockResolvedValue({
    id: "known",
    status: "recognized",
    recognizedPatch: "existing",
    nativeVerified: false,
    candidates: [{ id: "existing", name: "Live set" }],
    audio: {
      imported: 0,
      recognized: 1,
      failed: 0,
      complete: true,
      samples: [sample],
      results: [
        {
          source: "/other mount/A0/1_Kick.wav",
          status: "recognized",
          sample,
          error: null,
        },
      ],
    },
    draft: {
      id: "draft",
      name: "Other mount",
      selectedKit: "",
      kits: [],
      bpm: 120,
      voices: [],
    },
  });
  vi.mocked(native.restoreSample).mockResolvedValue({
    ...sample,
    retired: false,
  });
  render(<SdImport onClose={() => {}} onImported={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Choose SD folder…" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Restore Sample: User kick" }),
  );
  await screen.findByText("Restored User kick from Trash.");
  expect(native.restoreSample).toHaveBeenCalledWith("retired");
  expect(screen.queryByRole("button", { name: "Create new Patch" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Open existing Patch" }));
  expect(native.finishSdImport).toHaveBeenCalledWith("known", {
    type: "reuse",
  });
});
it("reports a source without _save instead of presenting its Kits as never saved", async () => {
  vi.mocked(native.chooseFolder).mockResolvedValue("/Copy");
  vi.mocked(native.prepareSdImport).mockResolvedValue({
    id: "intake",
    status: "new",
    recognizedPatch: null,
    nativeVerified: false,
    candidates: [],
    audio: {
      imported: 1,
      recognized: 0,
      failed: 0,
      complete: true,
      samples: [],
      results: [],
    },
    draft: {
      id: "draft",
      name: "Copy",
      selectedKit: "",
      kits: [],
      bpm: 120,
      voices: [],
      importedSd: {
        intakeId: "intake",
        copyComplete: true,
        fingerprint: "hash",
        files: [],
        issues: [],
        unassigned: [],
        native: {
          verifiedFirmware: "2.00",
          defaultLayerMode: "manual",
          issues: [
            "No _save folder was found, so no native settings were read. Layer modes use your Default Layer mode.",
          ],
          kits: [
            {
              bank: "B",
              number: 3,
              source: "appDefault",
              voices: [],
              issues: [],
            },
          ],
        },
      },
    },
  });
  render(<SdImport onClose={() => {}} onImported={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Choose SD folder…" }));
  expect(await screen.findByText(/No _save folder was found/)).toBeTruthy();
  expect(screen.queryByText(/never saved with STORE/)).toBeNull();
  expect(
    screen.getByText(
      /1 Kit\(s\) without a stored Kit file use your default Layer mode/,
    ),
  ).toBeTruthy();
});
