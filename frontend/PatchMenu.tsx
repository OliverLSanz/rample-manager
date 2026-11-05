import { useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  Copy,
  Disc3,
  HardDriveDownload,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import {
  modes,
  type LayerMode,
  type Patch,
  type PatchAction,
  type PatchSnapshot,
} from "./types";

/**
 * The open Patch and the way to another one. Choosing, creating, duplicating
 * or importing a Patch returns to the Kit view with that Patch loaded.
 */
export function PatchMenu({
  view,
  patch,
  onPatch,
  onImport,
  onDelete,
}: {
  view: PatchSnapshot;
  patch: Patch | undefined;
  onPatch: (action: PatchAction) => Promise<boolean>;
  onImport: () => void;
  /** Deletes a Patch and offers to undo it. */
  onDelete: (patch: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState("");
  const [creating, setCreating] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  function close() {
    setOpen(false);
    setRenaming(false);
    setCreating("");
  }
  useEffect(() => {
    if (!open) return;
    function key(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
        trigger.current?.focus();
      }
    }
    function pointer(e: MouseEvent) {
      if (!ref.current?.contains(e.target as Node)) close();
    }
    window.addEventListener("keydown", key);
    window.addEventListener("mousedown", pointer);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("mousedown", pointer);
    };
  }, [open]);
  const patches = view.graph.patches;
  return (
    <div className="patch-menu" ref={ref}>
      <button
        ref={trigger}
        className="patch-trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={patch ? `Patch ${patch.name}` : "Choose a Patch"}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <Disc3 size={16} />
        <span>
          <small>PATCH</small>
          <strong>{patch?.name ?? "No Patch"}</strong>
        </span>
        <ChevronDown size={12} />
      </button>
      {open && (
        <div className="patch-popover" role="dialog" aria-label="Patches">
          <ul aria-label="Patches in the Library">
            {patches.map((p) => {
              const current = p.id === patch?.id;
              return (
                <li key={p.id}>
                  {current && renaming ? (
                    <form
                      className="patch-name-form"
                      onSubmit={(e) => {
                        e.preventDefault();
                        if (!name.trim()) return;
                        void onPatch({ type: "renamePatch", name }).then(
                          (ok) => ok && setRenaming(false),
                        );
                      }}
                    >
                      <input
                        aria-label="Patch name"
                        value={name}
                        autoFocus
                        onChange={(e) => setName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Escape") {
                            e.stopPropagation();
                            setRenaming(false);
                          }
                        }}
                      />
                      <button disabled={!name.trim()}>Rename</button>
                    </form>
                  ) : (
                    <>
                      <button
                        className="patch-option"
                        aria-pressed={current}
                        onClick={() => {
                          if (current) close();
                          else
                            void onPatch({
                              type: "selectPatch",
                              patch: p.id,
                            }).then((ok) => ok && close());
                        }}
                      >
                        <span>
                          <strong>{p.name}</strong>
                          <small>
                            {p.kits.length}{" "}
                            {p.kits.length === 1 ? "Kit" : "Kits"}
                            {p.importedSd ? " · from SD" : ""}
                          </small>
                        </span>
                        {current && <Check size={13} />}
                      </button>
                      <span className="patch-actions">
                        {current && (
                          <button
                            className="icon-button"
                            aria-label={`Rename Patch ${p.name}`}
                            title="Rename"
                            onClick={() => {
                              setName(p.name);
                              setRenaming(true);
                            }}
                          >
                            <Pencil size={12} />
                          </button>
                        )}
                        <button
                          className="icon-button"
                          aria-label={`Duplicate Patch ${p.name}`}
                          title="Duplicate"
                          onClick={() =>
                            void onPatch({
                              type: "duplicatePatch",
                              patch: p.id,
                              name: `${p.name} copy`,
                            }).then((ok) => ok && close())
                          }
                        >
                          <Copy size={12} />
                        </button>
                        <button
                          className="icon-button danger"
                          aria-label={`Delete Patch ${p.name}`}
                          title="Delete (can be undone)"
                          onClick={() => onDelete(p.id)}
                        >
                          <Trash2 size={12} />
                        </button>
                      </span>
                    </>
                  )}
                </li>
              );
            })}
            {!patches.length && (
              <li className="patch-empty">No Patches in the Library yet.</li>
            )}
          </ul>
          <form
            className="patch-name-form"
            onSubmit={(e) => {
              e.preventDefault();
              if (!creating.trim()) return;
              void onPatch({ type: "createPatch", name: creating }).then(
                (ok) => ok && close(),
              );
            }}
          >
            <Plus size={13} />
            <input
              aria-label="New Patch name"
              placeholder="New Patch name"
              value={creating}
              onChange={(e) => setCreating(e.target.value)}
            />
            <button disabled={!creating.trim()}>Create Patch</button>
          </form>
          <button
            className="patch-import"
            onClick={() => {
              close();
              onImport();
            }}
          >
            <HardDriveDownload size={14} /> Import Rample SD…
          </button>
          <label className="patch-default-mode">
            <span>
              New Kits start in
              <small>
                Also used for imported Kits never saved with STORE on the
                Rample.
              </small>
            </span>
            <select
              aria-label="Default Layer mode"
              value={view.graph.defaultLayerMode ?? "manual"}
              onChange={(e) =>
                void onPatch({
                  type: "setDefaultMode",
                  mode: e.target.value as LayerMode,
                })
              }
            >
              {modes.map((mode) => (
                <option key={mode.id} value={mode.id}>
                  {mode.title}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}
    </div>
  );
}
