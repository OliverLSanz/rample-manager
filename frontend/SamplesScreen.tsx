import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  ArrowUpRight,
  Download,
  FolderInput,
  Pencil,
  Plus,
  RotateCcw,
  Search,
  Square,
  Trash2,
  Volume2,
  X,
} from "lucide-react";
import * as native from "./native";
import { useSampleAudition } from "./audition";
import { matchesSample } from "./SamplePanel";
import { colors } from "./Sequencer";
import { Waveform } from "./VoiceCard";
import {
  currentPatch,
  kitAddress,
  sampleUsage,
  sortedKits,
  type Usage,
} from "./patchView";
import {
  basename,
  type LibraryImportReply,
  type LibrarySample,
  type PatchAction,
  type PatchSnapshot,
} from "./types";

type Filter = "all" | "patch" | "unused" | "trash";
type Sort = "import" | "name" | "duration" | "used";
interface Target {
  patch: string;
  kit: string;
  channel: number;
}
const page = 200;
/** The tag lists a tag change replaced, so Undo can put them back. */
interface TagUndo {
  changes: { id: string; tags: string[] }[];
}

function format(sample: LibrarySample) {
  return [
    sample.format,
    `${sample.duration.toFixed(2)} s`,
    `${sample.rate} Hz`,
    sample.bits > 0 ? `${sample.bits}-bit` : null,
    sample.channels === 2 ? "stereo" : "mono",
  ]
    .filter(Boolean)
    .join(" · ");
}

/** Where added Samples go: the open Patch and Kit unless the user picks another. */
function TargetPicker({
  view,
  target,
  onChange,
}: {
  view: PatchSnapshot;
  target: Target;
  onChange: (target: Target) => void;
}) {
  const patch = view.graph.patches.find((p) => p.id === target.patch);
  const kits = sortedKits(patch);
  return (
    <fieldset className="target-picker" aria-label="Add to a Kit">
      <legend>Add to</legend>
      <select
        aria-label="Patch"
        value={target.patch}
        onChange={(e) => {
          const next = view.graph.patches.find((p) => p.id === e.target.value);
          onChange({
            patch: e.target.value,
            kit: next?.selectedKit || sortedKits(next)[0]?.id || "",
            channel: target.channel,
          });
        }}
      >
        {view.graph.patches.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
            {p.id === view.graph.selectedPatch ? " (open)" : ""}
          </option>
        ))}
      </select>
      <div>
        <select
          aria-label="Kit"
          value={target.kit}
          disabled={!kits.length}
          onChange={(e) => onChange({ ...target, kit: e.target.value })}
        >
          {!kits.length && <option value="">No Kits</option>}
          {kits.map((k) => (
            <option key={k.id} value={k.id}>
              Kit {kitAddress(k)}
            </option>
          ))}
        </select>
        <div role="radiogroup" aria-label="Voice">
          {[0, 1, 2, 3].map((c) => (
            <button
              key={c}
              role="radio"
              aria-checked={c === target.channel}
              aria-label={`Voice ${c + 1}`}
              style={{ "--voice": colors[c] } as CSSProperties}
              onClick={() => onChange({ ...target, channel: c })}
            >
              {c + 1}
            </button>
          ))}
        </div>
      </div>
    </fieldset>
  );
}

function SampleMetadata({
  sample,
  busy,
  onSave,
}: {
  sample: LibrarySample;
  busy: boolean;
  onSave: (name: string, tags: string[]) => Promise<void>;
}) {
  const [name, setName] = useState(sample.name);
  const [tags, setTags] = useState(sample.tags.join("\n"));
  return (
    <form
      className="sample-metadata"
      aria-label="Sample metadata"
      onSubmit={(event) => {
        event.preventDefault();
        void onSave(
          name,
          tags.split(/\r?\n/).filter((tag) => tag.length > 0),
        );
      }}
    >
      <label>
        Sample name
        <input
          value={name}
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
          required
        />
      </label>
      <label>
        Tags (one per line)
        <textarea
          value={tags}
          disabled={busy}
          onChange={(event) => setTags(event.target.value)}
        />
      </label>
      <button disabled={busy || !name.trim()} type="submit">
        Save metadata
      </button>
    </form>
  );
}

/**
 * The Library as a screen: find, hear, describe and retire Samples, see every
 * Layer that plays one, and add them to any Kit of any Patch. The open Patch
 * and Kit are the default destination.
 */
export function SamplesScreen({
  view,
  onPatch,
  onOpen,
  onChanged,
}: {
  view: PatchSnapshot;
  onPatch: (action: PatchAction) => Promise<boolean>;
  /** Shows a Kit of a Patch in the Kit view. */
  onOpen: (usage: Usage) => void;
  /** Library Samples changed: names, tags, Trash or imports. */
  onChanged: () => void;
}) {
  const [samples, setSamples] = useState<LibrarySample[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [tag, setTag] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("import");
  const [selected, setSelected] = useState<string[]>([]);
  // Shift-selection grows from the anchor; the cursor is the row last moved to.
  const [anchor, setAnchor] = useState<string | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [preview, setPreview] = useState(true);
  const [limit, setLimit] = useState(page);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [report, setReport] = useState<LibraryImportReply | null>(null);
  const [newTag, setNewTag] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameTo, setRenameTo] = useState("");
  const [tagUndo, setTagUndo] = useState<TagUndo | null>(null);
  const [waveforms, setWaveforms] = useState<Record<string, number[]>>({});
  const open = currentPatch(view);
  const [target, setTarget] = useState<Target>({
    patch: open?.id ?? "",
    kit: open?.selectedKit ?? "",
    channel: 0,
  });
  const audition = useSampleAudition();
  const alive = useRef(true);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    alive.current = true;
    native
      .listSamples()
      .then((rows) => alive.current && setSamples(rows))
      .catch((e) => alive.current && setError(String(e)));
    return () => {
      alive.current = false;
    };
  }, []);
  useEffect(() => setLimit(page), [query, filter, tag, sort]);
  // Keep the destination valid when Patches or Kits change.
  useEffect(() => {
    const patch = view.graph.patches.find((p) => p.id === target.patch);
    if (!patch) {
      if (open)
        setTarget({ patch: open.id, kit: open.selectedKit, channel: 0 });
    } else if (!patch.kits.some((k) => k.id === target.kit)) {
      const kit = patch.selectedKit || sortedKits(patch)[0]?.id || "";
      if (kit !== target.kit) setTarget({ ...target, kit });
    }
  }, [view.graph, target, open]);

  const usage = useMemo(() => sampleUsage(view.graph), [view.graph]);
  const rows = samples ?? [];
  const active = rows.filter((s) => !s.retired);
  const inPatch = (s: LibrarySample) =>
    usage.get(s.id)?.some((u) => u.patch === open?.id) ?? false;
  const counts = {
    all: active.length,
    patch: active.filter(inPatch).length,
    unused: active.filter((s) => !usage.has(s.id)).length,
    trash: rows.length - active.length,
  };
  const tags = useMemo(() => {
    const all = new Map<string, number>();
    for (const s of rows)
      if (!s.retired) for (const t of s.tags) all.set(t, (all.get(t) ?? 0) + 1);
    return [...all].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  }, [rows]);
  const matches = useMemo(() => {
    const found = rows.filter(
      (s) =>
        (filter === "trash" ? s.retired : !s.retired) &&
        (filter !== "patch" || inPatch(s)) &&
        (filter !== "unused" || !usage.has(s.id)) &&
        (!tag || s.tags.includes(tag)) &&
        matchesSample(s, query),
    );
    if (sort === "name")
      found.sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { numeric: true }),
      );
    if (sort === "duration") found.sort((a, b) => a.duration - b.duration);
    if (sort === "used")
      found.sort(
        (a, b) =>
          (usage.get(b.id)?.length ?? 0) - (usage.get(a.id)?.length ?? 0),
      );
    return found;
  }, [rows, filter, tag, query, sort, usage, open?.id]);
  const shown = matches.slice(0, limit);
  const picked = selected
    .map((id) => rows.find((s) => s.id === id))
    .filter((s): s is LibrarySample => !!s);
  const single = picked.length === 1 ? picked[0] : null;
  const selectionTags = (() => {
    const all = new Map<string, number>();
    for (const s of picked)
      for (const t of new Set(s.tags)) all.set(t, (all.get(t) ?? 0) + 1);
    return [...all].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  })();

  useEffect(() => {
    if (!single?.readable || waveforms[single.id]) return;
    let cancelled = false;
    native
      .sampleWaveform(single.id)
      .then((peaks) => {
        if (!cancelled) setWaveforms((all) => ({ ...all, [single.id]: peaks }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [single?.id, single?.readable, waveforms]);

  function replace(changed: LibrarySample[]) {
    setSamples((all) =>
      (all ?? []).map((row) => changed.find((s) => s.id === row.id) ?? row),
    );
  }
  /**
   * Removes a tag from `ids`, or deletes or renames it everywhere (ids null),
   * then offers Undo with the tag lists it replaced.
   */
  async function changeTag(
    from: string,
    to: string | null,
    ids: string[] | null,
    done: (changed: number) => string,
  ) {
    const before = new Map(rows.map((s) => [s.id, s.tags]));
    const changed = await run(() => native.replaceSampleTag(from, to, ids));
    if (!changed || !alive.current) return false;
    replace(changed);
    // A Library-wide rename or delete carries the tag filter with it.
    if (!ids && tag === from) setTag(to);
    setMessage(done(changed.length));
    if (changed.length)
      setTagUndo({
        changes: changed.map((s) => ({ id: s.id, tags: before.get(s.id)! })),
      });
    onChanged();
    return true;
  }
  async function undoTagChange(undo: TagUndo) {
    const restored = await run(() => native.setSampleTags(undo.changes));
    if (!restored || !alive.current) return;
    replace(restored);
    setMessage("Tag change undone.");
    onChanged();
  }
  async function renameTag(from: string) {
    const to = renameTo.trim();
    setRenaming(null);
    if (!to || to === from) return;
    const merge = rows.some((s) => s.tags.includes(to));
    await changeTag(from, to, null, () =>
      merge ? `Merged #${from} into #${to}.` : `Renamed #${from} to #${to}.`,
    );
  }
  async function run<T>(task: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    setError(null);
    setMessage(null);
    setTagUndo(null);
    try {
      return await task();
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  function select(id: string, how: "single" | "toggle" | "range") {
    setCursor(id);
    if (how === "toggle") {
      setSelected((all) =>
        all.includes(id) ? all.filter((x) => x !== id) : [...all, id],
      );
      setAnchor(id);
    } else if (how === "range" && anchor) {
      const ids = shown.map((s) => s.id);
      const [from, to] = [ids.indexOf(anchor), ids.indexOf(id)].sort(
        (a, b) => a - b,
      );
      if (from >= 0) setSelected(ids.slice(from, to + 1));
    } else {
      setSelected([id]);
      setAnchor(id);
    }
  }
  function focusRow(id: string) {
    requestAnimationFrame(() =>
      list.current
        ?.querySelector(`[data-sample="${id}"]`)
        ?.scrollIntoView?.({ block: "nearest" }),
    );
  }
  async function addTo(items: LibrarySample[]) {
    const usable = items.filter((s) => s.readable && !s.retired);
    if (!usable.length || !target.kit) return;
    let added = 0;
    for (const s of usable) {
      const ok = await onPatch({
        type: "addLayerTo",
        patch: target.patch,
        kit: target.kit,
        channel: target.channel,
        sample: s.id,
      });
      if (!ok) break;
      added++;
    }
    const patch = view.graph.patches.find((p) => p.id === target.patch);
    const kit = patch?.kits.find((k) => k.id === target.kit);
    if (added && alive.current && kit)
      setMessage(
        `Added ${added === 1 ? usable[0].name : `${added} Samples`} to Kit ${kitAddress(kit)}, Voice ${target.channel + 1}${patch && patch.id !== open?.id ? ` of ${patch.name}` : ""}.`,
      );
  }
  async function setRetired(items: LibrarySample[], retired: boolean) {
    const changed = await run(() =>
      Promise.all(
        items.map((s) =>
          retired ? native.retireSample(s.id) : native.restoreSample(s.id),
        ),
      ),
    );
    if (!changed || !alive.current) return;
    replace(changed);
    setReport(
      (current) =>
        current && {
          ...current,
          results: current.results.map((r) => ({
            ...r,
            sample: changed.find((s) => s.id === r.sample?.id) ?? r.sample,
          })),
        },
    );
    setSelected([]);
    onChanged();
  }
  async function importSources(folder: boolean) {
    const reply = await run(async () => {
      const paths = await native.chooseSampleSources(folder);
      return paths.length ? native.importSamples(paths) : undefined;
    });
    if (!reply || !alive.current) return;
    setSamples(reply.samples);
    setReport(reply);
    onChanged();
  }
  function key(e: React.KeyboardEvent) {
    if (e.target !== list.current || e.metaKey || e.ctrlKey || e.altKey) return;
    const current = cursor ? shown.findIndex((s) => s.id === cursor) : -1;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next =
        shown[
          Math.min(
            shown.length - 1,
            Math.max(0, current + (e.key === "ArrowDown" ? 1 : -1)),
          )
        ];
      if (!next) return;
      if (e.shiftKey) {
        select(next.id, "range");
      } else {
        select(next.id, "single");
        if (preview && next.readable) void audition.hear(next.id, true);
      }
      focusRow(next.id);
    } else if (e.key === " " && current >= 0) {
      e.preventDefault();
      e.stopPropagation();
      if (shown[current].readable) void audition.hear(shown[current].id, true);
    } else if (e.key === "Enter" && picked.length) {
      e.preventDefault();
      void addTo(picked);
    } else if (e.key === "Escape") {
      setSelected([]);
    }
  }
  const trash = filter === "trash";
  const where = (u: Usage) =>
    `${u.address} · Voice ${u.channel + 1} · Layer ${u.position}`;
  const canAdd = !!target.kit;
  return (
    <section className="samples-screen" aria-label="Samples">
      <header className="samples-header">
        <div>
          <h1>Samples</h1>
          <small>
            {counts.all} in the Library
            {counts.trash ? ` · ${counts.trash} in Trash` : ""}
          </small>
        </div>
        <label className="search">
          <Search size={14} />
          <input
            type="search"
            aria-label="Search Samples by name or tag"
            placeholder="Search name or tag"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <select
          aria-label="Sort Samples"
          value={sort}
          onChange={(e) => setSort(e.target.value as Sort)}
        >
          <option value="import">Import order</option>
          <option value="name">Name</option>
          <option value="duration">Duration</option>
          <option value="used">Most used</option>
        </select>
        <label className="hear-toggle">
          <input
            type="checkbox"
            checked={preview}
            onChange={(e) => setPreview(e.target.checked)}
          />
          Hear while browsing
        </label>
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void importSources(false)}
        >
          <Download size={13} /> Import audio files…
        </button>
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void importSources(true)}
        >
          <FolderInput size={13} /> Import folder…
        </button>
      </header>
      {report && (
        <details className="compatibility import-report" open>
          <summary role="status">
            {report.imported} imported · {report.recognized} recognized ·{" "}
            {report.failed} failed{!report.complete && " — Import incomplete"}
          </summary>
          <ul aria-label="Import results">
            {report.results.map((result, i) => (
              <li key={i}>
                {basename(result.source)}:{" "}
                {result.error ??
                  (result.status === "recognized"
                    ? result.sample?.retired
                      ? `Already in Trash: ${result.sample.name}`
                      : "Already in Library"
                    : "Imported")}
                {result.sample?.retired && (
                  <button
                    className="text-button"
                    disabled={busy}
                    aria-label={`Restore ${result.sample.name}`}
                    onClick={() => void setRetired([result.sample!], false)}
                  >
                    Restore Sample
                  </button>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
      <div className="samples-body">
        <nav className="samples-filters" aria-label="Sample filters">
          {(
            [
              ["all", "All Samples"],
              ["patch", `In ${open?.name ?? "this Patch"}`],
              ["unused", "Not in any Kit"],
              ["trash", "Trash"],
            ] as [Filter, string][]
          ).map(([id, label]) => (
            <button
              key={id}
              aria-pressed={filter === id}
              onClick={() => {
                setFilter(id);
                setSelected([]);
              }}
            >
              <span>{label}</span>
              <small>{counts[id]}</small>
            </button>
          ))}
          {tags.length > 0 && <h3>Tags</h3>}
          {tags.slice(0, 40).map(([name, count]) =>
            renaming === name ? (
              <form
                key={name}
                className="tag-rename"
                onSubmit={(e) => {
                  e.preventDefault();
                  void renameTag(name);
                }}
              >
                <input
                  aria-label={`New name for #${name}`}
                  value={renameTo}
                  autoFocus
                  onChange={(e) => setRenameTo(e.target.value)}
                  onBlur={() => setRenaming(null)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") {
                      e.stopPropagation();
                      setRenaming(null);
                    }
                  }}
                />
              </form>
            ) : (
              <div key={name} className="tag-row">
                <button
                  className="tag-filter"
                  aria-pressed={tag === name}
                  onClick={() => setTag(tag === name ? null : name)}
                >
                  <span>#{name}</span>
                  <small>{count}</small>
                </button>
                <button
                  className="icon-button"
                  aria-label={`Rename tag #${name}`}
                  title="Rename; an existing name merges both tags"
                  disabled={busy}
                  onClick={() => {
                    setRenameTo(name);
                    setRenaming(name);
                  }}
                >
                  <Pencil size={11} />
                </button>
                <button
                  className="icon-button"
                  aria-label={`Delete tag #${name}`}
                  title="Remove this tag from every Sample"
                  disabled={busy}
                  onClick={() =>
                    void changeTag(
                      name,
                      null,
                      null,
                      (n) =>
                        `Deleted #${name} from ${n} ${n === 1 ? "Sample" : "Samples"}.`,
                    )
                  }
                >
                  <Trash2 size={11} />
                </button>
              </div>
            ),
          )}
        </nav>
        <div
          className="samples-list"
          role="listbox"
          aria-label={trash ? "Trash Samples" : "Library Samples"}
          aria-multiselectable="true"
          tabIndex={0}
          ref={list}
          onKeyDown={key}
        >
          {samples === null && <p className="panel-empty">Loading Samples…</p>}
          {samples !== null && !matches.length && (
            <p className="panel-empty">
              {trash
                ? "Trash is empty."
                : rows.length
                  ? "No Samples match."
                  : "No Samples yet. Import audio files or a folder."}
            </p>
          )}
          {shown.map((s) => {
            const used = usage.get(s.id) ?? [];
            const here = used.filter((u) => u.patch === open?.id);
            const elsewhere = new Set(
              used.filter((u) => u.patch !== open?.id).map((u) => u.patch),
            ).size;
            return (
              <div
                key={s.id}
                data-sample={s.id}
                role="option"
                aria-selected={selected.includes(s.id)}
                aria-label={s.name}
                className="sample-row"
                onClick={(e) => {
                  list.current?.focus();
                  if (e.metaKey || e.ctrlKey) select(s.id, "toggle");
                  else if (e.shiftKey) select(s.id, "range");
                  else {
                    select(s.id, "single");
                    if (preview && s.readable) void audition.hear(s.id, true);
                  }
                }}
              >
                <span className="sample-name">
                  <strong>{s.name}</strong>
                  <small>
                    {s.tags.length ? s.tags.map((t) => `#${t}`).join(" ") : ""}
                    {s.error ? ` ${s.error}` : ""}
                  </small>
                </span>
                <span className="sample-meta">
                  {s.duration.toFixed(2)} s
                  <small>
                    {s.format} · {s.channels === 2 ? "stereo" : "mono"}
                  </small>
                </span>
                <span className="sample-used">
                  {here.slice(0, 3).map((u, i) => (
                    <i
                      key={i}
                      style={{ "--voice": colors[u.channel] } as CSSProperties}
                    >
                      {u.address}·V{u.channel + 1}
                    </i>
                  ))}
                  {here.length > 3 && <small>+{here.length - 3}</small>}
                  {elsewhere > 0 && (
                    <small>
                      {elsewhere} other {elsewhere === 1 ? "Patch" : "Patches"}
                    </small>
                  )}
                  {!used.length && <small>—</small>}
                </span>
                <button
                  className="icon-button"
                  aria-label={`Hear ${s.name}`}
                  title="Hear as the Rample plays it"
                  disabled={!s.readable}
                  onClick={(e) => {
                    e.stopPropagation();
                    void audition.hear(s.id, true);
                  }}
                >
                  <Volume2 size={13} />
                </button>
              </div>
            );
          })}
          {matches.length > shown.length && (
            <button
              className="text-button panel-more"
              onClick={() => setLimit((n) => n + page)}
            >
              Show {Math.min(page, matches.length - shown.length)} more of{" "}
              {matches.length - shown.length}
            </button>
          )}
        </div>
        <aside className="samples-detail" aria-label="Sample details">
          {single ? (
            <>
              <h2>{single.name}</h2>
              <small className="sample-format">{format(single)}</small>
              <Waveform peaks={waveforms[single.id]} color="#f68061" />
              <div className="detail-actions">
                <button
                  disabled={!single.readable}
                  aria-label={`Audition Rample ${single.name}`}
                  onClick={() => void audition.hear(single.id, true)}
                >
                  <Volume2 size={12} /> Rample mono
                </button>
                <button
                  disabled={!single.readable}
                  aria-label={`Audition Original ${single.name}`}
                  onClick={() => void audition.hear(single.id)}
                >
                  <Volume2 size={12} /> Original
                </button>
                <button
                  aria-label="Stop audition"
                  disabled={!audition.playing}
                  onClick={() => void audition.stop()}
                >
                  <Square size={10} />
                </button>
              </div>
              {!trash && (
                <>
                  <TargetPicker
                    view={view}
                    target={target}
                    onChange={setTarget}
                  />
                  <button
                    className="primary add-to"
                    disabled={!canAdd || !single.readable || busy}
                    onClick={() => void addTo([single])}
                  >
                    <Plus size={13} /> Add as Layer
                  </button>
                </>
              )}
              <section className="detail-usage" aria-label="Used in">
                <h3>Used in</h3>
                {(usage.get(single.id) ?? []).length === 0 ? (
                  <p>Not used in any Kit.</p>
                ) : (
                  <ul>
                    {(usage.get(single.id) ?? []).map((u, i) => (
                      <li key={i}>
                        <button
                          aria-label={`Open ${u.patchName} ${where(u)}`}
                          onClick={() => onOpen(u)}
                        >
                          <span>
                            {u.patch === open?.id ? (
                              <b>{u.patchName}</b>
                            ) : (
                              u.patchName
                            )}
                            <small>{where(u)}</small>
                          </span>
                          <ArrowUpRight size={12} />
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
              <SampleMetadata
                key={single.id + single.name + single.tags.join("\n")}
                sample={single}
                busy={busy}
                onSave={async (name, tags) => {
                  const saved = await run(() =>
                    native.updateSampleMetadata(single.id, name, tags),
                  );
                  if (saved && alive.current) {
                    replace([saved]);
                    setMessage("Metadata saved.");
                    onChanged();
                  }
                }}
              />
              <div className="detail-actions">
                <button
                  disabled={busy || !single.readable}
                  aria-label={`Export ${single.name}`}
                  onClick={() =>
                    void run(async () => {
                      const destination = await native.chooseSampleExport(
                        single.name,
                      );
                      if (!destination) return;
                      await native.exportSample(single.id, destination);
                      if (alive.current) setMessage("Sample exported.");
                    })
                  }
                >
                  <Download size={12} /> Export WAV…
                </button>
                <button
                  disabled={busy}
                  className={single.retired ? "" : "danger"}
                  aria-label={`${single.retired ? "Restore" : "Retire"} ${single.name}`}
                  onClick={() => void setRetired([single], !single.retired)}
                >
                  {single.retired ? (
                    <>
                      <RotateCcw size={12} /> Restore
                    </>
                  ) : (
                    <>
                      <Trash2 size={12} /> Move to Trash
                    </>
                  )}
                </button>
              </div>
              {single.retired && (
                <p className="detail-note">
                  Retired Samples stay available to existing Layers. Restore
                  them to add them to another Voice.
                </p>
              )}
            </>
          ) : picked.length > 1 ? (
            <>
              <h2>{picked.length} Samples selected</h2>
              {!trash && (
                <>
                  <TargetPicker
                    view={view}
                    target={target}
                    onChange={setTarget}
                  />
                  <button
                    className="primary add-to"
                    disabled={!canAdd || busy}
                    onClick={() => void addTo(picked)}
                  >
                    <Plus size={13} /> Add {picked.length} as Layers
                  </button>
                  <form
                    className="bulk-tag"
                    onSubmit={(e) => {
                      e.preventDefault();
                      const value = newTag.trim();
                      if (!value) return;
                      void run(async () => {
                        const saved: LibrarySample[] = [];
                        for (const s of picked)
                          if (!s.tags.includes(value))
                            saved.push(
                              await native.updateSampleMetadata(s.id, s.name, [
                                ...s.tags,
                                value,
                              ]),
                            );
                        if (!alive.current) return;
                        replace(saved);
                        setNewTag("");
                        setMessage(
                          `Tagged ${picked.length} Samples #${value}.`,
                        );
                        onChanged();
                      });
                    }}
                  >
                    <input
                      aria-label="Tag to add"
                      placeholder="Add a tag"
                      value={newTag}
                      onChange={(e) => setNewTag(e.target.value)}
                    />
                    <button disabled={busy || !newTag.trim()}>Add tag</button>
                  </form>
                  {selectionTags.length > 0 && (
                    <ul
                      className="selection-tags"
                      aria-label="Tags in selection"
                    >
                      {selectionTags.map(([name, count]) => (
                        <li key={name}>
                          <span>#{name}</span>
                          <small>
                            {count} of {picked.length}
                          </small>
                          <button
                            className="icon-button"
                            disabled={busy}
                            aria-label={`Remove #${name} from ${count} selected Samples`}
                            title="Remove from the selected Samples"
                            onClick={() =>
                              void changeTag(
                                name,
                                null,
                                picked.map((s) => s.id),
                                (n) =>
                                  `Removed #${name} from ${n} ${n === 1 ? "Sample" : "Samples"}.`,
                              )
                            }
                          >
                            <X size={11} />
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </>
              )}
              <div className="detail-actions">
                <button
                  disabled={busy}
                  className={trash ? "" : "danger"}
                  onClick={() => void setRetired(picked, !trash)}
                >
                  {trash ? (
                    <>
                      <RotateCcw size={12} /> Restore {picked.length}
                    </>
                  ) : (
                    <>
                      <Trash2 size={12} /> Move {picked.length} to Trash
                    </>
                  )}
                </button>
              </div>
            </>
          ) : (
            <p className="detail-note">
              Select a Sample to hear it, see where it plays and add it to a
              Kit. ⌘/Ctrl-click or Shift-click selects several.
            </p>
          )}
          {message && (
            <p role="status" className="detail-message">
              {message}
              {tagUndo && (
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => void undoTagChange(tagUndo)}
                >
                  Undo
                </button>
              )}
            </p>
          )}
          {(error || audition.error) && (
            <p role="alert" className="detail-error">
              {error ?? audition.error}
            </p>
          )}
        </aside>
      </div>
    </section>
  );
}
