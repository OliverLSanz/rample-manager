import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { LibraryMigration } from "./LibraryMigration";
import * as native from "./native";
vi.mock("./native", () => ({
  migrateLegacy: vi.fn(),
  migrationStatus: vi.fn(),
}));
afterEach(cleanup);
it("reports recoverable partial failure then opens the managed workspace after retry", async () => {
  const complete = vi.fn();
  vi.mocked(native.migrationStatus).mockResolvedValue({
    status: "idle",
    processed: 0,
    total: 0,
    errors: [],
    backup: null,
    current: null,
  });
  vi.mocked(native.migrateLegacy)
    .mockResolvedValueOnce({
      status: "failed",
      processed: 2,
      total: 2,
      errors: ["2_Broken.wav: truncated audio"],
      backup: "/owned/session.backup.json",
      current: null,
    })
    .mockResolvedValueOnce({
      status: "complete",
      processed: 2,
      total: 2,
      errors: [],
      backup: "/owned/session.backup.json",
      current: null,
    });
  render(<LibraryMigration onComplete={complete} />);
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Move current folder into Library",
    }),
  );
  expect((await screen.findByRole("alert")).textContent).toContain(
    "2_Broken.wav: truncated audio",
  );
  expect(
    screen.getByText(/Your folder session remains available/),
  ).toBeTruthy();
  expect(complete).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Retry migration" }));
  expect(await screen.findByText(/Migration complete/)).toBeTruthy();
  expect(complete).toHaveBeenCalled();
});
it("resumes visible progress after the view reopens during migration", async () => {
  const complete = vi.fn();
  vi.mocked(native.migrationStatus)
    .mockResolvedValueOnce({
      status: "working",
      processed: 1,
      total: 2,
      current: "2_Snare.wav",
      errors: [],
      backup: "/owned/backup.json",
    })
    .mockResolvedValue({
      status: "complete",
      processed: 2,
      total: 2,
      current: null,
      errors: [],
      backup: "/owned/backup.json",
    });
  render(<LibraryMigration onComplete={complete} />);
  expect((await screen.findByRole("status")).textContent).toContain(
    "2_Snare.wav",
  );
  expect(await screen.findByText("Migration complete")).toBeTruthy();
  expect(complete).toHaveBeenCalled();
});
