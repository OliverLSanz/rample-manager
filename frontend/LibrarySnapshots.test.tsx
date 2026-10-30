import { beforeEach, afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { LibrarySnapshots } from "./LibrarySnapshots";
import * as native from "./native";
vi.mock("./native", () => ({
  libraryRecoverySnapshots: vi.fn(),
  chooseLibraryBackup: vi.fn(),
  chooseFolder: vi.fn(),
  backupLibrary: vi.fn(),
  restoreLibrary: vi.fn(),
}));
beforeEach(() => {
  vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
it("backs up to a new folder and shows its concrete location", async () => {
  vi.mocked(native.chooseLibraryBackup).mockResolvedValue("/backup/My Library");
  vi.mocked(native.backupLibrary).mockResolvedValue({
    complete: true,
    activated: false,
    location: "/backup/My Library",
    recovery: null,
    error: null,
    snapshot: null,
  });
  render(<LibrarySnapshots managed onClose={() => {}} onRestored={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Back up Library…" }));
  expect(await screen.findByText("Library backup complete.")).toBeTruthy();
  expect(screen.getByText("/backup/My Library")).toBeTruthy();
  expect(native.backupLibrary).toHaveBeenCalledWith("/backup/My Library");
});
it("confirms replacement, returns the recovery location and offers the same restore route", async () => {
  vi.mocked(native.chooseFolder).mockResolvedValue(
    "/incoming/Portable Library",
  );
  vi.mocked(native.restoreLibrary).mockResolvedValue({
    complete: true,
    activated: true,
    location: "/incoming/Portable Library",
    recovery: "/support/Library/Recovery/previous",
    error: null,
    snapshot: null,
  });
  const onRestored = vi.fn();
  render(
    <LibrarySnapshots managed onClose={() => {}} onRestored={onRestored} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
  expect(
    await screen.findByText(/replaces the entire active Library/),
  ).toBeTruthy();
  expect(screen.getByText(/verified recovery snapshot/)).toBeTruthy();
  expect(native.restoreLibrary).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Replace Library" }));
  expect(await screen.findByText("Library restored.")).toBeTruthy();
  expect(screen.getByText("/support/Library/Recovery/previous")).toBeTruthy();
  expect(onRestored).toHaveBeenCalledTimes(1);
  fireEvent.click(
    screen.getByRole("button", { name: "Restore recovery snapshot…" }),
  );
  expect(
    await screen.findByText("/support/Library/Recovery/previous"),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Replace Library" }));
  await waitFor(() =>
    expect(native.restoreLibrary).toHaveBeenLastCalledWith(
      "/support/Library/Recovery/previous",
    ),
  );
});
it("keeps cancellation local and gates all controls while a restore is running", async () => {
  vi.mocked(native.chooseLibraryBackup).mockResolvedValue(null);
  vi.mocked(native.chooseFolder)
    .mockResolvedValueOnce(null)
    .mockResolvedValue("/backup/Library");
  const onClose = vi.fn();
  let finish!: (result: import("./types").LibrarySnapshotReply) => void;
  vi.mocked(native.restoreLibrary).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<LibrarySnapshots managed onClose={onClose} onRestored={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Back up Library…" }));
  await waitFor(() =>
    expect(
      screen
        .getByRole("button", { name: "Back up Library…" })
        .hasAttribute("disabled"),
    ).toBe(false),
  );
  expect(native.backupLibrary).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
  await waitFor(() =>
    expect(
      screen
        .getByRole("button", { name: "Restore Library…" })
        .hasAttribute("disabled"),
    ).toBe(false),
  );
  expect(native.restoreLibrary).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
  await screen.findByRole("button", { name: "Cancel replacement" });
  fireEvent.click(screen.getByRole("button", { name: "Cancel replacement" }));
  expect(native.restoreLibrary).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
  await screen.findByRole("button", { name: "Replace Library" });
  fireEvent.click(screen.getByRole("button", { name: "Replace Library" }));
  await waitFor(() => expect(native.restoreLibrary).toHaveBeenCalledTimes(1));
  expect(
    screen
      .getByRole("button", { name: "Back up Library…" })
      .hasAttribute("disabled"),
  ).toBe(true);
  expect(
    screen.getByRole("button", { name: "Done" }).hasAttribute("disabled"),
  ).toBe(true);
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onClose).not.toHaveBeenCalled();
  finish({
    complete: false,
    activated: false,
    location: null,
    recovery: "/recovery/current",
    error: "Could not publish activation",
    snapshot: null,
  });
  expect(await screen.findByText("Library was not replaced.")).toBeTruthy();
  expect(screen.getByText("/recovery/current")).toBeTruthy();
});
it("distinguishes validation failure from an activated Library needing playback refresh", async () => {
  vi.mocked(native.chooseFolder).mockResolvedValue("/backup/Library");
  vi.mocked(native.restoreLibrary)
    .mockRejectedValueOnce("Snapshot Original integrity check failed")
    .mockResolvedValue({
      complete: false,
      activated: true,
      location: "/backup/Library",
      recovery: "/recovery/old",
      error: "Playback refresh failed; reopen the application",
      snapshot: null,
    });
  const refreshed = vi.fn();
  render(
    <LibrarySnapshots managed onClose={() => {}} onRestored={refreshed} />,
  );
  for (const index of [0, 1]) {
    fireEvent.click(screen.getByRole("button", { name: "Restore Library…" }));
    await screen.findByRole("button", { name: "Replace Library" });
    fireEvent.click(screen.getByRole("button", { name: "Replace Library" }));
    if (index === 0) {
      expect(
        await screen.findByText(/Library was not replaced: Snapshot Original/),
      ).toBeTruthy();
      expect(refreshed).not.toHaveBeenCalled();
    } else {
      expect(
        await screen.findByText("Library restored; playback needs attention."),
      ).toBeTruthy();
      expect(screen.getByText("/recovery/old")).toBeTruthy();
      expect(refreshed).toHaveBeenCalledTimes(1);
    }
  }
});
it("explains migration before allowing a portable folder-session backup", () => {
  render(
    <LibrarySnapshots
      managed={false}
      onClose={() => {}}
      onRestored={() => {}}
    />,
  );
  expect(screen.getByText(/Move the folder session/)).toBeTruthy();
  expect(
    screen
      .getByRole("button", { name: "Back up Library…" })
      .hasAttribute("disabled"),
  ).toBe(true);
  expect(
    screen
      .getByRole("button", { name: "Restore Library…" })
      .hasAttribute("disabled"),
  ).toBe(true);
});

it("offers saved recovery snapshots again after reopening the app", async () => {
  vi.mocked(native.libraryRecoverySnapshots).mockResolvedValue([
    "/support/Library/Recovery/older",
  ]);
  render(<LibrarySnapshots managed onClose={() => {}} onRestored={() => {}} />);
  expect(
    await screen.findByText("/support/Library/Recovery/older"),
  ).toBeTruthy();
  fireEvent.click(
    screen.getByRole("button", { name: "Restore this recovery snapshot…" }),
  );
  expect(
    await screen.findByRole("group", { name: "Confirm Library replacement" }),
  ).toBeTruthy();
  expect(native.restoreLibrary).not.toHaveBeenCalled();
});
