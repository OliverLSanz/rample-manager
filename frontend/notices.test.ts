import { afterEach, describe, expect, it, vi } from "vitest";
import {
  compareVersions,
  dismissNotice,
  dismissedNotices,
  loadNotice,
  messageNotices,
  pickNotice,
  releaseNotes,
  updateNotice,
} from "./notices";

const notes = `Rample Preview is now **Rample Manager**.

### New in 3.1

- **Faster Build.** Large Patches build in half the time and use less memory while rendering every Layer of every Kit.
- Search \`tags\` and [names](https://example.com) together.
- Fix a crash.
* Fourth
- Fifth
- Sixth is never shown

### First launch

The macOS app is not notarized.`;

afterEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe("compareVersions", () => {
  it("compares numerically and ignores a v prefix", () => {
    expect(compareVersions("3.10.0", "3.9.1")).toBe(1);
    expect(compareVersions("v3.0.0", "3.0")).toBe(0);
    expect(compareVersions("2.9.9", "3.0.0")).toBe(-1);
    expect(compareVersions("3.0.0-beta", "3.0.0")).toBeNull();
  });
});

describe("updateNotice", () => {
  it("offers only a newer published release, with its first notes", () => {
    const release = { tag_name: "v3.1.0", body: notes };
    const notice = updateNotice(release, "3.0.0");
    expect(notice?.id).toBe("update:3.1.0");
    expect(notice?.title).toBe("Rample Manager 3.1.0 is available");
    expect(notice?.notes).toEqual([
      "Faster Build. Large Patches build in half the time and use less memory while rendering every Layer of every Kit.",
      "Search tags and names together.",
      "Fix a crash.",
      "Fourth",
      "Fifth",
    ]);
    expect(updateNotice(release, "3.1.0")).toBeNull();
    expect(updateNotice(release, "3.2.0")).toBeNull();
    expect(updateNotice({ ...release, prerelease: true }, "3.0.0")).toBeNull();
    expect(updateNotice({ message: "rate limited" }, "3.0.0")).toBeNull();
    expect(updateNotice(null, "3.0.0")).toBeNull();
  });
  it("joins wrapped bullets and shortens long ones at a sentence", () => {
    const wrapped = `- **Library.** Import WAV, AIFF, FLAC and MP3 files or
  whole folders. Originals are kept untouched; name, tag, search, audition and
  move Samples to a recoverable Trash.
- One. Two.

Not a note.`;
    expect(releaseNotes(wrapped)).toEqual([
      "Library. Import WAV, AIFF, FLAC and MP3 files or whole folders.",
      "One. Two.",
    ]);
    expect(releaseNotes(`- ${"word ".repeat(40)}`)[0]).toMatch(/word…$/);
  });
});

describe("messageNotices", () => {
  const today = new Date("2026-11-06T12:00:00Z");
  it("keeps valid messages for this version and date", () => {
    const feed = {
      messages: [
        { id: "welcome", title: "Hello", body: "Thanks", url: "https://x.y" },
        { id: "old", title: "Expired", until: "2026-11-05" },
        { id: "future", title: "Needs 4", minVersion: "4.0.0" },
        { id: "legacy", title: "Only 2.x", maxVersion: "2.9.9" },
        { id: "unsafe", title: "Local link", url: "file:///etc" },
        { title: "No id" },
        "junk",
      ],
    };
    const found = messageNotices(feed, "3.0.0", today);
    expect(found.map((n) => n.id)).toEqual([
      "message:welcome",
      "message:unsafe",
    ]);
    expect(found[0]).toMatchObject({
      url: "https://x.y",
      linkLabel: "Read more",
    });
    expect(found[1].url).toBeUndefined();
  });
  it("ignores feeds that are missing or malformed", () => {
    expect(messageNotices(null, "3.0.0", today)).toEqual([]);
    expect(messageNotices({ messages: "x" }, "3.0.0", today)).toEqual([]);
  });
});

describe("dismissal", () => {
  it("shows the first notice not dismissed, and remembers dismissals", () => {
    const a = updateNotice({ tag_name: "v9.0.0", body: "" }, "3.0.0");
    const b = messageNotices(
      { messages: [{ id: "news", title: "News" }] },
      "3.0.0",
      new Date(),
    )[0];
    expect(pickNotice([a, b], dismissedNotices())?.id).toBe("update:9.0.0");
    dismissNotice("update:9.0.0");
    expect(pickNotice([a, b], dismissedNotices())?.id).toBe("message:news");
    dismissNotice("message:news");
    expect(pickNotice([a, b], dismissedNotices())).toBeNull();
  });
  it("treats unreadable storage as nothing dismissed", () => {
    localStorage.setItem("rample-manager.dismissed-notices", "{not json");
    expect(dismissedNotices().size).toBe(0);
  });
});

describe("loadNotice", () => {
  it("resolves to no notice when the network fails", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("offline")));
    await expect(loadNotice("3.0.0")).resolves.toBeNull();
  });
  it("resolves to no notice on error responses or invalid JSON", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.reject(new SyntaxError("bad")),
      }),
    );
    await expect(loadNotice("3.0.0")).resolves.toBeNull();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false }));
    await expect(loadNotice("3.0.0")).resolves.toBeNull();
  });
  it("still offers an update when only the message feed fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        url.includes("api.github.com")
          ? Promise.resolve({
              ok: true,
              json: () =>
                Promise.resolve({ tag_name: "v3.1.0", body: "- New" }),
            })
          : Promise.reject(new TypeError("offline")),
      ),
    );
    expect((await loadNotice("3.0.0"))?.notes).toEqual(["New"]);
  });
});
