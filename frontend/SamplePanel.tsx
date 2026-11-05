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

/** Drag type for a Library Sample dropped on a voice card. */
export const sampleDrag = "application/x-rample-sample";
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
 * the next one. Rows can also be dragged onto any voice card.
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
  onAdd: (sample: LibrarySample, channel: number) => Promise<boolean>;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<string | null>(null);
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
  function select(sample: LibrarySample, play: boolean) {
    setActive(sample.id);
    if (play) void hear(sample);
    requestAnimationFrame(() =>
      list.current
        ?.querySelector(`[data-sample="${sample.id}"]`)
        ?.scrollIntoView?.({ block: "nearest" }),
    );
  }
  async function add(sample: LibrarySample) {
    if (!sample.readable) return;
    setActive(sample.id);
    if (await onAdd(sample, channel)) setAdded(sample.name);
  }
  function key(e: React.KeyboardEvent) {
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
      if (next) select(next, preview);
    } else if (e.key === "Enter" && at >= 0) {
      e.preventDefault();
      void add(shown[at]);
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
            aria-selected={sample.id === active}
            aria-disabled={!sample.readable || undefined}
            className="panel-row"
            draggable={sample.readable}
            onDragStart={(e) => {
              e.dataTransfer.setData(sampleDrag, sample.id);
              e.dataTransfer.effectAllowed = "copy";
            }}
            onClick={() => select(sample, true)}
            onDoubleClick={() => void add(sample)}
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
                void add(sample);
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
      <footer>
        {audition.error ? (
          <span role="alert">{audition.error}</span>
        ) : added ? (
          <span role="status" className="panel-added">
            <Check size={12} /> Added {added} to Voice {channel + 1}
          </span>
        ) : (
          <span>
            ↑↓ browse · Enter adds to Voice {channel + 1} · drag to any voice
          </span>
        )}
      </footer>
    </aside>
  );
}
