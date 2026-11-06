import { afterEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { NoticeCard } from "./NoticeCard";
import * as native from "./native";
import { dismissedNotices, type Notice } from "./notices";

vi.mock("./native", () => ({ openLink: vi.fn() }));

const update: Notice = {
  id: "update:3.1.0",
  kind: "update",
  title: "Rample Manager 3.1.0 is available",
  notes: ["Faster Build."],
  url: "https://oliverlsanz.com/RampleManager/#download",
  linkLabel: "Download",
};

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.useRealTimers();
});

it("waits before appearing and never takes focus", async () => {
  vi.useFakeTimers();
  render(<NoticeCard version="3.0.0" load={async () => update} />);
  expect(screen.queryByRole("status")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(4000));
  expect(screen.getByText("Rample Manager 3.1.0 is available")).toBeTruthy();
  expect(screen.getByText("Faster Build.")).toBeTruthy();
  expect(document.activeElement).toBe(document.body);
});

it("skipping a version dismisses it for good", async () => {
  render(<NoticeCard version="3.0.0" load={async () => update} delay={0} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Skip this version" }),
  );
  expect(screen.queryByText(update.title)).toBeNull();
  expect(dismissedNotices().has("update:3.1.0")).toBe(true);
});

it("following the link opens the browser and dismisses the notice", async () => {
  render(<NoticeCard version="3.0.0" load={async () => update} delay={0} />);
  fireEvent.click(await screen.findByRole("button", { name: "Download" }));
  expect(native.openLink).toHaveBeenCalledWith(update.url);
  expect(dismissedNotices().has("update:3.1.0")).toBe(true);
});

it("shows nothing when loading fails", async () => {
  const load = vi.fn().mockRejectedValue(new Error("offline"));
  const { container } = render(
    <NoticeCard version="3.0.0" load={load} delay={0} />,
  );
  await waitFor(() => expect(load).toHaveBeenCalled());
  await act(async () => {});
  expect(container.innerHTML).toBe("");
});
