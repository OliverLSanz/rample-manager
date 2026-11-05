import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  Copy,
  MoreHorizontal,
  Pencil,
  Plus,
  Search,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react";
import {
  banks,
  kitAddress,
  kitIssues,
  layerCount,
  nextFreeAddress,
  sortedKits,
} from "./patchView";
import type { LibrarySample, Patch, PatchAction, PatchKit } from "./types";

/** Bank letter and Kit number, as Rample folders name a Kit (A0–Z99). */
function AddressForm({
  label,
  submit,
  initial,
  taken,
  onSubmit,
  onCancel,
}: {
  label: string;
  submit: string;
  initial: { bank: string; number: number };
  /** Addresses already used by other Kits of the Patch. */
  taken: Set<string>;
  onSubmit: (bank: string, number: number) => Promise<boolean>;
  onCancel: () => void;
}) {
  const [bank, setBank] = useState(initial.bank);
  const [number, setNumber] = useState(String(initial.number));
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLFormElement>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLInputElement>("input")?.select();
  }, []);
  const value = Number(number);
  const valid =
    number.trim() !== "" &&
    Number.isInteger(value) &&
    value >= 0 &&
    value <= 99;
  const conflict = valid && taken.has(`${bank}${value}`);
  return (
    <form
      ref={ref}
      className="address-form"
      aria-label={label}
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid || conflict || busy) return;
        setBusy(true);
        void onSubmit(bank, value).then((ok) => {
          if (!ok) setBusy(false);
        });
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onCancel();
        }
      }}
    >
      <div>
        <label className="address-bank">
          <select
            aria-label="Bank"
            value={bank}
            onChange={(e) => setBank(e.target.value)}
          >
            {banks.map((b) => (
              <option key={b}>{b}</option>
            ))}
          </select>
          <ChevronDown size={9} />
        </label>
        <input
          aria-label="Kit number"
          inputMode="numeric"
          value={number}
          onChange={(e) =>
            setNumber(e.target.value.replace(/\D/g, "").slice(0, 2))
          }
        />
        <button
          type="submit"
          className="address-submit"
          aria-label={submit}
          title={submit}
          disabled={!valid || conflict || busy}
        >
          <Check size={13} />
        </button>
        <button
          type="button"
          aria-label="Cancel"
          title="Cancel"
          onClick={onCancel}
        >
          <X size={13} />
        </button>
      </div>
      <small role={conflict ? "alert" : undefined}>
        {conflict
          ? `${bank}${value} is already used in this Patch`
          : valid
            ? `Folder ${bank}${value} on the SD card`
            : "Kit numbers go from 0 to 99"}
      </small>
    </form>
  );
}

type Editing =
  | { kind: "create" }
  | { kind: "move"; kit: string }
  | { kind: "duplicate"; kit: string };

/** Actions for one Kit, shown from its row's ⋯ button. */
function KitMenu({
  address,
  onMove,
  onDuplicate,
  onDelete,
  onClose,
}: {
  address: string;
  onMove: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button")?.focus();
    function key(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    }
    function pointer(e: MouseEvent) {
      if (!ref.current?.contains(e.target as Node)) onClose();
    }
    window.addEventListener("keydown", key, true);
    window.addEventListener("mousedown", pointer);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("mousedown", pointer);
    };
  }, [onClose]);
  return (
    <div
      className="kit-menu"
      role="menu"
      aria-label={`Kit ${address} actions`}
      ref={ref}
      onKeyDown={(e) => {
        if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
        e.preventDefault();
        e.stopPropagation();
        const items = [
          ...(ref.current?.querySelectorAll<HTMLButtonElement>("button") ?? []),
        ];
        const at = items.indexOf(document.activeElement as HTMLButtonElement);
        const next =
          (at + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
        items[next]?.focus();
      }}
    >
      <button role="menuitem" onClick={onMove}>
        <Pencil size={12} /> Change address…
      </button>
      <button role="menuitem" onClick={onDuplicate}>
        <Copy size={12} /> Duplicate…
      </button>
      <button role="menuitem" className="danger" onClick={onDelete}>
        <Trash2 size={12} /> Delete Kit
      </button>
    </div>
  );
}

/**
 * The Kits of the open Patch, grouped by Bank: browse, create, re-address,
 * duplicate and delete them. Arrow keys move between Kits; Delete removes the
 * focused Kit (it can be undone). Render it with `key` set to the Patch ID so
 * search, folding and open forms start fresh for each Patch.
 */
export function KitList({
  patch,
  issues,
  samples,
  onPatch,
  onDelete,
}: {
  patch: Patch | undefined;
  /** Validation messages of the current Patch, as from `patchIssues`. */
  issues: string[];
  samples: Map<string, LibrarySample>;
  onPatch: (action: PatchAction) => Promise<boolean>;
  /** Deletes a Kit and offers to undo it. */
  onDelete: (kit: string) => void;
}) {
  const [search, setSearch] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [editing, setEditing] = useState<Editing | null>(null);
  const [menu, setMenu] = useState<string | null>(null);
  const nav = useRef<HTMLElement>(null);
  const kits = useMemo(() => sortedKits(patch), [patch]);
  const term = search.trim().toLowerCase();
  const matching = kits.filter(
    (k) =>
      !term ||
      kitAddress(k).toLowerCase().includes(term) ||
      k.voices.some((v) =>
        v.layers.some((l) =>
          samples.get(l.sampleId)?.name.toLowerCase().includes(term),
        ),
      ),
  );
  const groups = [...new Set(matching.map((k) => k.bank))].map((bank) => ({
    bank,
    kits: matching.filter((k) => k.bank === bank),
    // Searching shows every match; the selected Kit's Bank never hides it.
    open:
      !!term ||
      !collapsed.has(bank) ||
      matching.some((k) => k.bank === bank && k.id === patch?.selectedKit),
  }));
  const visible = groups.flatMap((g) => (g.open ? g.kits : []));
  const addresses = (except?: string) =>
    new Set(kits.filter((k) => k.id !== except).map(kitAddress));
  const selected = patch?.kits.find((k) => k.id === patch.selectedKit);
  const closeMenu = useCallback(() => setMenu(null), []);
  function focusKit(id: string) {
    requestAnimationFrame(() =>
      nav.current
        ?.querySelector<HTMLButtonElement>(`[data-kit="${id}"]`)
        ?.focus(),
    );
  }
  function form(kit: PatchKit | undefined) {
    if (!editing || !patch) return null;
    if (editing.kind === "create" && !kit)
      return (
        <AddressForm
          label="New Kit address"
          submit="Create Kit"
          initial={nextFreeAddress(patch, selected?.bank ?? "A")}
          taken={addresses()}
          onCancel={() => setEditing(null)}
          onSubmit={async (bank, number) => {
            const ok = await onPatch({ type: "createKit", bank, number });
            if (ok) {
              setEditing(null);
              setSearch("");
            }
            return ok;
          }}
        />
      );
    if (!kit || editing.kind === "create" || editing.kit !== kit.id)
      return null;
    const address = kitAddress(kit);
    if (editing.kind === "move")
      return (
        <AddressForm
          label={`Change address of Kit ${address}`}
          submit="Move Kit"
          initial={kit}
          taken={addresses(kit.id)}
          onCancel={() => setEditing(null)}
          onSubmit={async (bank, number) => {
            const ok = await onPatch({
              type: "addressKit",
              bank,
              number,
              kit: kit.id,
            });
            if (ok) setEditing(null);
            return ok;
          }}
        />
      );
    return (
      <AddressForm
        label={`Duplicate Kit ${address}`}
        submit="Duplicate Kit"
        initial={nextFreeAddress(patch, kit.bank)}
        taken={addresses()}
        onCancel={() => setEditing(null)}
        onSubmit={async (bank, number) => {
          const ok = await onPatch({
            type: "duplicateKit",
            kit: kit.id,
            bank,
            number,
          });
          if (ok) setEditing(null);
          return ok;
        }}
      />
    );
  }
  return (
    <>
      <div className="library-heading">
        <span>KITS</span>
        <span>{kits.length}</span>
        <button
          className="new-kit"
          aria-label="New Kit"
          title="New Kit"
          disabled={!patch}
          onClick={() => {
            setMenu(null);
            setEditing({ kind: "create" });
          }}
        >
          <Plus size={13} />
        </button>
      </div>
      <label className="search">
        <Search size={14} />
        <input
          aria-label="Search kits or samples"
          placeholder="Search kits or samples"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {search && (
          <button aria-label="Clear search" onClick={() => setSearch("")}>
            <X size={12} />
          </button>
        )}
      </label>
      <nav
        aria-label="Sample kits"
        className="kit-list"
        ref={nav}
        onKeyDown={(e) => {
          const target = e.target as HTMLElement;
          const id = target.dataset.kit;
          if (!id || e.metaKey || e.ctrlKey || e.altKey) return;
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const at = visible.findIndex((k) => k.id === id);
            const next = visible[at + (e.key === "ArrowDown" ? 1 : -1)];
            if (next) {
              void onPatch({ type: "selectKit", kit: next.id });
              focusKit(next.id);
            }
          } else if (e.key === "Delete" || e.key === "Backspace") {
            e.preventDefault();
            const at = visible.findIndex((k) => k.id === id);
            const next = visible[at + 1] ?? visible[at - 1];
            onDelete(id);
            if (next) focusKit(next.id);
          }
        }}
      >
        {form(undefined)}
        {groups.map((group) => (
          <section
            key={group.bank}
            className="bank-group"
            aria-label={`Bank ${group.bank}`}
          >
            <button
              className="bank-heading"
              aria-expanded={group.open}
              disabled={!!term}
              onClick={() =>
                setCollapsed((all) => {
                  const next = new Set(all);
                  if (next.has(group.bank)) next.delete(group.bank);
                  else next.add(group.bank);
                  return next;
                })
              }
            >
              <ChevronDown size={10} />
              BANK {group.bank}
              <small>{group.kits.length}</small>
            </button>
            {group.open &&
              group.kits.map((k) => {
                const editor = form(k);
                if (editing?.kind === "move" && editor)
                  return <div key={k.id}>{editor}</div>;
                const address = kitAddress(k);
                const current = patch?.selectedKit === k.id;
                const problems = kitIssues(issues, k);
                const count = layerCount(k);
                return (
                  <div key={k.id}>
                    <div
                      className={`kit ${current ? "selected" : ""} ${menu === k.id ? "menu-open" : ""}`}
                    >
                      <button
                        className="kit-select"
                        data-kit={k.id}
                        aria-label={`Load kit ${address}`}
                        aria-current={current ? "true" : undefined}
                        onClick={() =>
                          current ||
                          void onPatch({ type: "selectKit", kit: k.id })
                        }
                      >
                        <b>{address}</b>
                        <span>
                          <strong>Kit {address}</strong>
                          <small>
                            {count} {count === 1 ? "layer" : "layers"}
                          </small>
                        </span>
                        {problems.length > 0 && (
                          <span
                            className="kit-warning"
                            role="img"
                            aria-label={`${problems.length} ${problems.length === 1 ? "issue" : "issues"}`}
                            title={problems.join("\n")}
                          >
                            <TriangleAlert size={11} />
                          </span>
                        )}
                      </button>
                      <button
                        className="kit-more"
                        aria-label={`Kit ${address} actions`}
                        aria-haspopup="menu"
                        aria-expanded={menu === k.id}
                        title="Kit actions"
                        onClick={() => setMenu(menu === k.id ? null : k.id)}
                      >
                        <MoreHorizontal size={13} />
                      </button>
                      {menu === k.id && (
                        <KitMenu
                          address={address}
                          onClose={closeMenu}
                          onMove={() => {
                            setMenu(null);
                            setEditing({ kind: "move", kit: k.id });
                          }}
                          onDuplicate={() => {
                            setMenu(null);
                            setEditing({ kind: "duplicate", kit: k.id });
                          }}
                          onDelete={() => {
                            setMenu(null);
                            onDelete(k.id);
                          }}
                        />
                      )}
                    </div>
                    {editor}
                  </div>
                );
              })}
          </section>
        ))}
        {patch && !kits.length && editing?.kind !== "create" && (
          <p className="no-matches">
            No Kits yet.
            <br />
            <button
              className="text-button"
              onClick={() => setEditing({ kind: "create" })}
            >
              Create the first Kit
            </button>
          </p>
        )}
        {kits.length > 0 && !matching.length && (
          <p className="no-matches">No matches</p>
        )}
      </nav>
    </>
  );
}
