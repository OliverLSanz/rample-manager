import { useEffect, useState } from "react";
import { Megaphone, Sparkles, X } from "lucide-react";
import * as native from "./native";
import { dismissNotice, loadNotice, type Notice } from "./notices";

/** How long after startup a notice may appear, so it never greets the user. */
const DELAY_MS = 4000;

/**
 * One quiet notice in the corner: a new version with its notes, or news.
 * It never takes focus, and closing it or following its link dismisses it
 * for good on this computer.
 */
export function NoticeCard({
  version,
  load = loadNotice,
  delay = DELAY_MS,
}: {
  version: string;
  load?: (version: string) => Promise<Notice | null>;
  delay?: number;
}) {
  const [notice, setNotice] = useState<Notice | null>(null);
  useEffect(() => {
    let alive = true;
    const timer = setTimeout(() => {
      void load(version)
        .then((found) => {
          if (alive) setNotice(found);
        })
        .catch(() => {});
    }, delay);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [version, load, delay]);
  if (!notice) return null;
  function dismiss() {
    if (!notice) return;
    dismissNotice(notice.id);
    setNotice(null);
  }
  function follow() {
    const url = notice?.url;
    dismiss();
    if (url) Promise.resolve(native.openLink(url)).catch(() => {});
  }
  return (
    <aside
      className="notice-card"
      role="status"
      aria-label={notice.kind === "update" ? "Update available" : "News"}
    >
      <div className="notice-card-heading">
        {notice.kind === "update" ? (
          <Sparkles size={15} />
        ) : (
          <Megaphone size={15} />
        )}
        <span>{notice.kind === "update" ? "Update available" : "News"}</span>
        <button aria-label="Dismiss" title="Dismiss" onClick={dismiss}>
          <X size={14} />
        </button>
      </div>
      <strong>{notice.title}</strong>
      {notice.body && <p>{notice.body}</p>}
      {notice.notes.length > 0 && (
        <ul aria-label="What's new">
          {notice.notes.map((note, i) => (
            <li key={i}>{note}</li>
          ))}
        </ul>
      )}
      <div className="notice-card-actions">
        {notice.url && (
          <button className="primary" onClick={follow}>
            {notice.linkLabel}
          </button>
        )}
        <button className="text-button" onClick={dismiss}>
          {notice.kind === "update" ? "Skip this version" : "Dismiss"}
        </button>
      </div>
    </aside>
  );
}
