import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { LibrarySamples } from "./LibrarySamples";
import * as native from "./native";

vi.mock("./native", () => ({
  listSamples: vi.fn(),
  openSampleAudition: vi.fn(),
  chooseSampleSources: vi.fn(),
  importSamples: vi.fn(),
  auditionSample: vi.fn(),
  stopSampleAudition: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("imports files, reports failures, lists private Samples and auditions by identity", async () => {
  const sample = {
    id: "sample-kick",
    originalId: "original-kick",
    name: "My kick",
    sha256: "abc",
    duration: 0.1,
    rate: 44100,
    bits: 16,
    channels: 1,
    readable: true,
    error: null,
  };
  vi.mocked(native.openSampleAudition).mockResolvedValue("owner-test");
  vi.mocked(native.listSamples).mockResolvedValue([]);
  vi.mocked(native.chooseSampleSources).mockResolvedValue([
    "/source/My kick.wav",
    "/source/Broken.wav",
  ]);
  vi.mocked(native.importSamples).mockResolvedValue({
    imported: 1,
    recognized: 0,
    failed: 1,
    complete: false,
    samples: [sample],
    results: [
      {
        source: "/source/My kick.wav",
        status: "imported",
        sample,
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
  vi.mocked(native.auditionSample).mockResolvedValue();
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  render(<LibrarySamples onClose={() => {}} />);
  await screen.findByText("No Samples yet.");
  fireEvent.click(screen.getByRole("button", { name: "Import WAV files…" }));
  const samples = await screen.findByRole("list", { name: "Library Samples" });
  expect(within(samples).getByText("My kick")).toBeTruthy();
  expect(screen.getByRole("status").textContent).toContain(
    "1 imported · 0 recognized · 1 failed",
  );
  expect(screen.getByText(/Broken.wav: Truncated WAV chunk/)).toBeTruthy();
  fireEvent.click(
    within(samples).getByRole("button", { name: "Audition My kick" }),
  );
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith(
      "sample-kick",
      "owner-test",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "Stop audition" }));
  await waitFor(() => expect(native.stopSampleAudition).toHaveBeenCalled());
  vi.mocked(native.chooseSampleSources).mockResolvedValue(["/renamed-folder"]);
  vi.mocked(native.importSamples).mockResolvedValue({
    imported: 0,
    recognized: 1,
    failed: 0,
    complete: true,
    samples: [sample],
    results: [
      {
        source: "/renamed-folder/Renamed.wav",
        status: "recognized",
        sample,
        error: null,
      },
    ],
  });
  fireEvent.click(screen.getByRole("button", { name: "Import folder…" }));
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain(
      "0 imported · 1 recognized · 0 failed",
    ),
  );
  expect(native.chooseSampleSources).toHaveBeenLastCalledWith(true);
  expect(
    within(screen.getByRole("list", { name: "Library Samples" })).getAllByRole(
      "button",
    ),
  ).toHaveLength(1);
});

it("closing an old panel cannot stop audition in a newly opened Library", async () => {
  const sample = {
    id: "sample-kick",
    originalId: "original-kick",
    name: "Kick",
    sha256: "abc",
    duration: 0.1,
    rate: 44100,
    bits: 16,
    channels: 1,
    readable: true,
    error: null,
  };
  vi.mocked(native.openSampleAudition)
    .mockResolvedValueOnce("owner-a")
    .mockResolvedValueOnce("owner-b");
  vi.mocked(native.listSamples).mockResolvedValue([sample]);
  let finish!: () => void;
  vi.mocked(native.auditionSample)
    .mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValue();
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  const old = render(<LibrarySamples onClose={() => {}} />);
  fireEvent.click(await screen.findByRole("button", { name: "Audition Kick" }));
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith(
      "sample-kick",
      "owner-a",
    ),
  );
  old.unmount();
  expect(native.stopSampleAudition).toHaveBeenCalledWith("owner-a", true);
  render(<LibrarySamples onClose={() => {}} />);
  fireEvent.click(await screen.findByRole("button", { name: "Audition Kick" }));
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith(
      "sample-kick",
      "owner-b",
    ),
  );
  finish();
  await Promise.resolve();
  expect(native.stopSampleAudition).toHaveBeenCalledTimes(1);
});
