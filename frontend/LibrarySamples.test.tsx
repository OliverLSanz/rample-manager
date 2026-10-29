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
  querySamples: vi.fn(),
  updateSampleMetadata: vi.fn(),
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
    tags: [],
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
      "listitem",
    ),
  ).toHaveLength(1);
});

it("closing an old panel cannot stop audition in a newly opened Library", async () => {
  const sample = {
    id: "sample-kick",
    originalId: "original-kick",
    name: "Kick",
    tags: [],
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

it("edits the selected Sample and searches tags while duplicate names audition by ID", async () => {
  const first = {
    id: "first",
    originalId: "original-first",
    name: "Same name",
    tags: ["dry"],
    sha256: "a",
    duration: 0.1,
    rate: 44100,
    bits: 16,
    channels: 1,
    readable: true,
    error: null,
  };
  const second = {
    ...first,
    id: "second",
    originalId: "original-second",
    tags: ["warm"],
  };
  const saved = {
    ...second,
    name: "Warm kick",
    tags: ["café / warm, soft", "低音"],
  };
  vi.mocked(native.openSampleAudition).mockResolvedValue("owner");
  vi.mocked(native.listSamples).mockResolvedValue([first, second]);
  vi.mocked(native.querySamples).mockResolvedValue([saved]);
  vi.mocked(native.updateSampleMetadata).mockResolvedValue(saved);
  vi.mocked(native.stopSampleAudition).mockResolvedValue(true);
  vi.mocked(native.auditionSample).mockResolvedValue();
  const choose = vi.fn();
  render(<LibrarySamples onClose={() => {}} onSelect={choose} />);
  const rows = within(
    await screen.findByRole("list", { name: "Library Samples" }),
  ).getAllByRole("listitem");
  fireEvent.click(
    within(rows[1]).getByRole("button", { name: "Audition Same name" }),
  );
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith("second", "owner"),
  );
  fireEvent.click(
    within(rows[1]).getByRole("button", { name: "Select Same name" }),
  );
  expect(
    (screen.getByRole("textbox", { name: "Sample name" }) as HTMLInputElement)
      .value,
  ).toBe("Same name");
  fireEvent.change(screen.getByRole("textbox", { name: "Sample name" }), {
    target: { value: "Warm kick" },
  });
  fireEvent.change(
    screen.getByRole("textbox", { name: "Tags (one per line)" }),
    { target: { value: "café / warm, soft\n低音" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "Save metadata" }));
  await screen.findByText("Metadata saved.");
  expect(native.updateSampleMetadata).toHaveBeenCalledWith(
    "second",
    "Warm kick",
    ["café / warm, soft", "低音"],
  );
  expect(
    screen
      .getByRole("button", { name: "Select Warm kick" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  fireEvent.change(
    screen.getByRole("searchbox", { name: "Search Samples by name or tag" }),
    { target: { value: "低音" } },
  );
  await waitFor(() =>
    expect(
      screen.queryByRole("button", { name: "Select Same name" }),
    ).toBeNull(),
  );
  vi.mocked(native.auditionSample).mockClear();
  fireEvent.click(screen.getByRole("button", { name: "Audition Warm kick" }));
  await waitFor(() =>
    expect(native.auditionSample).toHaveBeenCalledWith("second", "owner"),
  );
  expect(native.querySamples).toHaveBeenCalledWith("低音");
  fireEvent.click(screen.getByRole("button", { name: "Use Sample" }));
  expect(choose).toHaveBeenCalledWith("second");
});
