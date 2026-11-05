import { useEffect, useMemo, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  Layers,
  Pencil,
  Plus,
  Search,
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
import type { LibrarySample, Patch, PatchAction } from "./types";

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

/** The Kits of the open Patch: browse, create and re-address them. */
export function KitList({
  patch,
  issues,
  samples,
  onPatch,
}: {
  patch: Patch | undefined;
  /** Validation messages of the current Patch, as from `patchIssues`. */
  issues: string[];
  samples: Map<string, LibrarySample>;
  onPatch: (action: PatchAction) => Promise<boolean>;
}) {
  const [search, setSearch] = useState("");
  const [bank, setBank] = useState("All");
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const kits = useMemo(() => sortedKits(patch), [patch]);
  const used = useMemo(() => [...new Set(kits.map((k) => k.bank))], [kits]);
  useEffect(() => {
    setCreating(false);
    setEditing(null);
    setSearch("");
    setBank("All");
  }, [patch?.id]);
  const term = search.trim().toLowerCase();
  const visible = kits.filter(
    (k) =>
      (bank === "All" || k.bank === bank) &&
      (!term ||
        kitAddress(k).toLowerCase().includes(term) ||
        k.voices.some((v) =>
          v.layers.some((l) =>
            samples.get(l.sampleId)?.name.toLowerCase().includes(term),
          ),
        )),
  );
  const addresses = (except?: string) =>
    new Set(kits.filter((k) => k.id !== except).map(kitAddress));
  const selected = patch?.kits.find((k) => k.id === patch.selectedKit);
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
            setEditing(null);
            setCreating(true);
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
      {used.length > 1 && (
        <label className="bank-filter">
          <Layers size={14} />
          <select
            aria-label="Filter banks"
            value={used.includes(bank) ? bank : "All"}
            onChange={(e) => setBank(e.target.value)}
          >
            <option value="All">All banks</option>
            {used.map((b) => (
              <option key={b} value={b}>
                Bank {b}
              </option>
            ))}
          </select>
          <ChevronDown size={11} />
        </label>
      )}
      <nav aria-label="Sample kits" className="kit-list">
        {creating && patch && (
          <AddressForm
            label="New Kit address"
            submit="Create Kit"
            initial={nextFreeAddress(patch, selected?.bank ?? "A")}
            taken={addresses()}
            onCancel={() => setCreating(false)}
            onSubmit={async (b, number) => {
              const ok = await onPatch({ type: "createKit", bank: b, number });
              if (ok) {
                setCreating(false);
                setSearch("");
                setBank("All");
              }
              return ok;
            }}
          />
        )}
        {visible.map((k) => {
          const address = kitAddress(k);
          const current = patch?.selectedKit === k.id;
          const problems = kitIssues(issues, k);
          if (editing === k.id)
            return (
              <AddressForm
                key={k.id}
                label={`Change address of Kit ${address}`}
                submit="Move Kit"
                initial={k}
                taken={addresses(k.id)}
                onCancel={() => setEditing(null)}
                onSubmit={async (b, number) => {
                  const ok = await onPatch({
                    type: "addressKit",
                    bank: b,
                    number,
                  });
                  if (ok) setEditing(null);
                  return ok;
                }}
              />
            );
          const count = layerCount(k);
          return (
            <div key={k.id} className={`kit ${current ? "selected" : ""}`}>
              <button
                className="kit-select"
                aria-label={`Load kit ${address}`}
                aria-current={current ? "true" : undefined}
                onClick={() =>
                  current || void onPatch({ type: "selectKit", kit: k.id })
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
              {current && (
                <button
                  className="kit-edit"
                  aria-label={`Change address of Kit ${address}`}
                  title="Change Bank or Kit number"
                  onClick={() => {
                    setCreating(false);
                    setEditing(k.id);
                  }}
                >
                  <Pencil size={11} />
                </button>
              )}
            </div>
          );
        })}
        {patch && !kits.length && !creating && (
          <p className="no-matches">
            No Kits yet.
            <br />
            <button className="text-button" onClick={() => setCreating(true)}>
              Create the first Kit
            </button>
          </p>
        )}
        {kits.length > 0 && !visible.length && (
          <p className="no-matches">
            {term ? "No matches" : "No kits in this bank"}
          </p>
        )}
      </nav>
    </>
  );
}
