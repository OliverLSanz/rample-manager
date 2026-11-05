import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { Check, Plus, Search, Volume2, X } from "lucide-react";
import { useSampleAudition } from "./audition";
import { colors } from "./Sequencer";
import type { LibrarySample } from "./types";

/** Drag type for Library Samples dropped on a voice card: IDs, one per line. */
export const sampleDrag = "application/x-rample-sample";
/** The Sample IDs carried by a `sampleDrag` payload, in order. */
export const draggedSamples = (data: string) =>
  data.split("\n").filter((id) => id.length > 0);
const page = 150;

/** Literal, case-insensitive part of a name or tag, like the Library search. */
export function matchesSample(sample: LibrarySample, query: string) {
  const term = query.trim().toLowerCase();
  return (
    !term ||
    [sample.name, ...sample.tags].some((text) =>
      text.toLowerCase().includes(term),
    )
  );
}

/**
 * A side panel beside the Kit for adding Library Samples as Layers. Browsing
 * with the arrow keys auditions each Sample as the Rample will play it; Enter,
 * double-click or + adds it to the target Voice, and the panel stays open for
 * the next one. Shift- and Cmd/Ctrl-click (or Shift+arrows, Cmd/Ctrl+A) select
 * several, which Enter adds and a drag carries together, in list order. Rows
 * can also be dragged onto any voice card.
 */
export function SamplePanel({
  samples,
  channel,
  kit,
  onChannel,
  onAdd,
  onClose,
}: {
  samples: LibrarySample[];
  /** The Voice that receives added Samples. */
  channel: number;
  /** Address of the Kit being edited, such as `A0`. */
  kit: string;
  onChannel: (channel: number) => void;
  /** Adds the Samples, in order, as new Layers of Voice `channel`. */
  onAdd: (samples: LibrarySample[], channel: number) => Promise<boolean>;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  // The cursor row; the selection grows from the anchor with Shift.
  const [active, setActive] = useState<string | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [anchor, setAnchor] = useState<string | null>(null);
  const [preview, setPreview] = useState(true);
  const [limit, setLimit] = useState(page);
  const [added, setAdded] = useState<string | null>(null);
  const audition = useSampleAudition();
  const list = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const matches = useMemo(
    () => samples.filter((s) => !s.retired && matchesSample(s, query)),
    [samples, query],
  );
  const shown = matches.slice(0, limit);
  /** Selected Samples that can be added, in list order. */
  const picked = shown.filter((s) => s.readable && selected.includes(s.id));
  useEffect(() => search.current?.focus(), []);
  useEffect(() => setLimit(page), [query]);
  useEffect(() => {
    if (!added) return;
    const timer = setTimeout(() => setAdded(null), 2500);
    return () => clearTimeout(timer);
  }, [added]);

  /** Plays the Sample as the Rample will. */
  function hear(sample: LibrarySample) {
    if (sample.readable) void audition.hear(sample.id, true);
  }
  /**
   * Moves the cursor to `sample`: `only` selects just it, `toggle` adds or
   * removes it, `range` selects from the anchor to it.
   */
  function select(
    sample: LibrarySample,
    play: boolean,
    how: "only" | "toggle" | "range" = "only",
  ) {
    setActive(sample.id);
    if (how === "range") {
      const from = shown.findIndex((s) => s.id === (anchor ?? active));
      const to = shown.findIndex((s) => s.id === sample.id);
      const [a, b] = from < 0 ? [to, to] : [from, to].sort((x, y) => x - y);
      setSelected(shown.slice(a, b + 1).map((s) => s.id));
    } else {
      setAnchor(sample.id);
      setSelected((all) =>
        how === "only"
          ? [sample.id]
          : all.includes(sample.id)
            ? all.filter((id) => id !== sample.id)
            : [...all, sample.id],
      );
    }
    if (play) void hear(sample);
    requestAnimationFrame(() =>
      list.current
        ?.querySelector(`[data-sample="${sample.id}"]`)
        ?.scrollIntoView?.({ block: "nearest" }),
    );
  }
  async function add(items: LibrarySample[]) {
    const usable = items.filter((s) => s.readable);
    if (!usable.length) return;
    if (await onAdd(usable, channel))
      setAdded(
        usable.length === 1 ? usable[0].name : `${usable.length} Samples`,
      );
  }
  /** The Samples a row acts on: the whole selection when the row is in it. */
  function group(sample: LibrarySample) {
    return selected.includes(sample.id) && picked.length > 1
      ? picked
      : [sample];
  }
  function key(e: React.KeyboardEvent) {
    const typing = e.target === search.current;
    if (
      (e.metaKey || e.ctrlKey) &&
      e.key.toLowerCase() === "a" &&
      !typing &&
      shown.length
    ) {
      e.preventDefault();
      setSelected(shown.map((s) => s.id));
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const at = shown.findIndex((s) => s.id === active);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next =
        shown[
          Math.min(
            shown.length - 1,
            Math.max(0, at + (e.key === "ArrowDown" ? 1 : -1)),
          )
        ];
      if (next) select(next, preview, e.shiftKey ? "range" : "only");
    } else if (e.key === "Enter" && at >= 0) {
      e.preventDefault();
      void add(picked.length ? picked : [shown[at]]);
    } else if (e.key === " " && at >= 0 && e.target !== search.current) {
      e.preventDefault();
      e.stopPropagation();
      void hear(shown[at]);
    } else if (/^[1-4]$/.test(e.key) && e.target !== search.current) {
      e.preventDefault();
      e.stopPropagation();
      onChannel(Number(e.key) - 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
  return (
    <aside
      className="sample-panel"
      aria-label="Add Samples"
      style={{ "--voice": colors[channel] } as CSSProperties}
      onKeyDown={key}
    >
      <header>
        <h2>Add Samples</h2>
        <button
          className="icon-button"
          aria-label="Close Add Samples"
          onClick={onClose}
        >
          <X size={15} />
        </button>
      </header>
      <div className="panel-target">
        <span>
          To Kit <b>{kit}</b>
        </span>
        <div role="radiogroup" aria-label="Target voice">
          {[0, 1, 2, 3].map((c) => (
            <button
              key={c}
              role="radio"
              aria-checked={c === channel}
              aria-label={`Voice ${c + 1}`}
              style={{ "--voice": colors[c] } as CSSProperties}
              onClick={() => onChannel(c)}
            >
              {c + 1}
            </button>
          ))}
        </div>
      </div>
      <label className="search panel-search">
        <Search size={14} />
        <input
          ref={search}
          type="search"
          aria-label="Search Samples by name or tag"
          placeholder="Search name or tag"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <div className="panel-options">
        <label>
          <input
            type="checkbox"
            checked={preview}
            onChange={(e) => setPreview(e.target.checked)}
          />
          Hear while browsing
        </label>
        <small>
          {matches.length} {matches.length === 1 ? "Sample" : "Samples"}
        </small>
      </div>
      <div
        className="panel-list"
        role="listbox"
        aria-label="Library Samples"
        aria-multiselectable="true"
        aria-activedescendant={active ? `panel-${active}` : undefined}
        tabIndex={0}
        ref={list}
        onFocus={() => {
          if (!active && shown[0]) select(shown[0], false);
        }}
      >
        {shown.map((sample) => (
          <div
            key={sample.id}
            id={`panel-${sample.id}`}
            data-sample={sample.id}
            role="option"
            aria-selected={selected.includes(sample.id)}
            aria-disabled={!sample.readable || undefined}
            className={`panel-row ${sample.id === active ? "cursor" : ""}`}
            draggable={sample.readable}
            onDragStart={(e) => {
              const dragged = group(sample);
              e.dataTransfer.setData(
                sampleDrag,
                dragged.map((s) => s.id).join("\n"),
              );
              e.dataTransfer.effectAllowed = "copy";
            }}
            onClick={(e) =>
              select(
                sample,
                !e.shiftKey && !e.metaKey && !e.ctrlKey,
                e.shiftKey
                  ? "range"
                  : e.metaKey || e.ctrlKey
                    ? "toggle"
                    : "only",
              )
            }
            onDoubleClick={() => void add([sample])}
          >
            <span>
              <strong>{sample.name}</strong>
              <small>
                {sample.duration.toFixed(2)} s
                {sample.channels === 2 ? " · stereo" : ""}
                {sample.tags.length > 0 && ` · ${sample.tags.join(" · ")}`}
                {sample.error && ` · ${sample.error}`}
              </small>
            </span>
            <button
              tabIndex={-1}
              aria-label={`Hear ${sample.name}`}
              title="Hear as the Rample plays it"
              disabled={!sample.readable}
              onClick={(e) => {
                e.stopPropagation();
                setActive(sample.id);
                void hear(sample);
              }}
            >
              <Volume2 size={13} />
            </button>
            <button
              tabIndex={-1}
              aria-label={`Add ${sample.name} to Voice ${channel + 1}`}
              title={`Add to Voice ${channel + 1}`}
              disabled={!sample.readable}
              onClick={(e) => {
                e.stopPropagation();
                setActive(sample.id);
                void add([sample]);
              }}
            >
              <Plus size={14} />
            </button>
          </div>
        ))}
        {matches.length > shown.length && (
          <button
            className="text-button panel-more"
            onClick={() => setLimit((n) => n + page)}
          >
            Show {Math.min(page, matches.length - shown.length)} more of{" "}
            {matches.length - shown.length}
          </button>
        )}
        {!matches.length && (
          <p className="panel-empty">
            {samples.length
              ? "No Samples match."
              : "No Samples yet. Import audio in the Samples view."}
          </p>
        )}
      </div>
      {picked.length > 1 && (
        <div className="panel-selection" role="group" aria-label="Selection">
          <span>{picked.length} selected</span>
          <button className="primary" onClick={() => void add(picked)}>
            <Plus size={13} /> Add {picked.length} to Voice {channel + 1}
          </button>
          <button className="text-button" onClick={() => setSelected([])}>
            Clear
          </button>
        </div>
      )}
      <footer>
        {audition.error ? (
          <span role="alert">{audition.error}</span>
        ) : added ? (
          <span role="status" className="panel-added">
            <Check size={12} /> Added {added} to Voice {channel + 1}
          </span>
        ) : (
          <span>
            ↑↓ browse · ⇧/⌘-click selects several · Enter adds to Voice{" "}
            {channel + 1} · drag to any voice
          </span>
        )}
      </footer>
    </aside>
  );
}
