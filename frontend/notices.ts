/**
 * Startup notices: "a new version is available" from the latest GitHub
 * Release, and optional news from the website's messages.json. Both are
 * best effort: any failure (offline, timeout, bad data) means no notice.
 * The user sees at most one notice per launch, and never again once dismissed.
 */

export const RELEASES_API =
  "https://api.github.com/repos/OliverLSanz/rample-manager/releases/latest";
export const MESSAGES_URL =
  "https://oliverlsanz.com/RampleManager/messages.json";
export const DOWNLOAD_URL = "https://oliverlsanz.com/RampleManager/#download";
const DISMISSED_KEY = "rample-manager.dismissed-notices";
const TIMEOUT_MS = 5000;
const MAX_NOTES = 5;

export type Notice = {
  id: string;
  kind: "update" | "message";
  title: string;
  body?: string;
  notes: string[];
  url?: string;
  linkLabel?: string;
};

/** A message published in messages.json. Only id and title are required. */
type RemoteMessage = {
  id?: unknown;
  title?: unknown;
  body?: unknown;
  url?: unknown;
  linkLabel?: unknown;
  minVersion?: unknown;
  maxVersion?: unknown;
  until?: unknown;
};

/** Compares dotted versions numerically ("3.10.0" > "3.9.1"); null if unparsable. */
export function compareVersions(a: string, b: string): number | null {
  const parse = (v: string) => {
    const parts = v.trim().replace(/^v/, "").split(".");
    if (parts.length < 1 || parts.length > 3) return null;
    const numbers = parts.map((p) => (/^\d+$/.test(p) ? Number(p) : NaN));
    if (numbers.some(Number.isNaN)) return null;
    while (numbers.length < 3) numbers.push(0);
    return numbers;
  };
  const x = parse(a),
    y = parse(b);
  if (!x || !y) return null;
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
  return 0;
}

/** Plain text of a Markdown line: no emphasis, code marks or link targets. */
function plain(line: string): string {
  return line
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/[*_`]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** Shortens a long note at its last full sentence, or at a word. */
function short(note: string): string {
  if (note.length <= 140) return note;
  const end = note.lastIndexOf(". ", 139);
  if (end >= 40) return note.slice(0, end + 1);
  return note.slice(0, 137).replace(/\s+\S*$/, "") + "…";
}

/** The first bullet points of release notes, as short plain text. A bullet
 * continues on the following indented lines. */
export function releaseNotes(markdown: string): string[] {
  const bullets: string[] = [];
  let open = false;
  for (const line of markdown.split(/\r?\n/)) {
    if (/^\s*[-*] /.test(line)) {
      bullets.push(line.replace(/^\s*[-*] /, ""));
      open = true;
    } else if (open && /^\s+\S/.test(line)) {
      bullets[bullets.length - 1] += " " + line.trim();
    } else {
      open = false;
    }
  }
  return bullets
    .map((bullet) => short(plain(bullet)))
    .filter(Boolean)
    .slice(0, MAX_NOTES);
}

/** An update notice when the latest Release is newer than this app. */
export function updateNotice(release: unknown, current: string): Notice | null {
  if (!release || typeof release !== "object") return null;
  const { tag_name, body, draft, prerelease } = release as Record<
    string,
    unknown
  >;
  if (typeof tag_name !== "string" || draft || prerelease) return null;
  const version = tag_name.replace(/^v/, "");
  if ((compareVersions(version, current) ?? 0) <= 0) return null;
  return {
    id: `update:${version}`,
    kind: "update",
    title: `Rample Manager ${version} is available`,
    notes: typeof body === "string" ? releaseNotes(body) : [],
    url: DOWNLOAD_URL,
    linkLabel: "Download",
  };
}

/** Messages from messages.json that apply to this version today. */
export function messageNotices(
  feed: unknown,
  current: string,
  today: Date,
): Notice[] {
  const list = (feed as { messages?: unknown } | null)?.messages;
  if (!Array.isArray(list)) return [];
  const day = today.toISOString().slice(0, 10);
  return list.flatMap((raw: RemoteMessage) => {
    if (!raw || typeof raw !== "object") return [];
    const { id, title, body, url, linkLabel, minVersion, maxVersion, until } =
      raw;
    if (typeof id !== "string" || !id || typeof title !== "string" || !title)
      return [];
    if (typeof minVersion === "string") {
      const c = compareVersions(current, minVersion);
      if (c === null || c < 0) return [];
    }
    if (typeof maxVersion === "string") {
      const c = compareVersions(current, maxVersion);
      if (c === null || c > 0) return [];
    }
    if (typeof until === "string" && until < day) return [];
    const link =
      typeof url === "string" && url.startsWith("https://") ? url : undefined;
    return [
      {
        id: `message:${id}`,
        kind: "message" as const,
        title,
        body: typeof body === "string" ? body : undefined,
        notes: [],
        url: link,
        linkLabel:
          link && typeof linkLabel === "string" && linkLabel
            ? linkLabel
            : link
              ? "Read more"
              : undefined,
      },
    ];
  });
}

/** The one notice to show: a newer version first, then the oldest message. */
export function pickNotice(
  candidates: (Notice | null)[],
  dismissed: Set<string>,
): Notice | null {
  return candidates.find((n) => n && !dismissed.has(n.id)) ?? null;
}

export function dismissedNotices(): Set<string> {
  try {
    const saved = JSON.parse(localStorage.getItem(DISMISSED_KEY) ?? "[]");
    return new Set(Array.isArray(saved) ? saved.map(String) : []);
  } catch {
    return new Set();
  }
}

export function dismissNotice(id: string) {
  try {
    const ids = dismissedNotices();
    ids.add(id);
    localStorage.setItem(DISMISSED_KEY, JSON.stringify([...ids]));
  } catch {
    // Without storage the notice may return on the next launch; nothing breaks.
  }
}

async function getJson(url: string): Promise<unknown> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const response = await fetch(url, {
      signal: controller.signal,
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
    return response.ok ? await response.json() : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/** Fetches both sources; resolves to null rather than ever rejecting. */
export async function loadNotice(
  current: string,
  today = new Date(),
): Promise<Notice | null> {
  try {
    const [release, feed] = await Promise.all([
      getJson(RELEASES_API),
      getJson(MESSAGES_URL),
    ]);
    return pickNotice(
      [updateNotice(release, current), ...messageNotices(feed, current, today)],
      dismissedNotices(),
    );
  } catch {
    return null;
  }
}
