import { useEffect, useState } from "react";
import * as native from "./native";
import type { MigrationStatus } from "./types";
export function LibraryMigration({ onComplete }: { onComplete: () => void }) {
  const [status, setStatus] = useState<MigrationStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const working = busy || status?.status === "working";
  useEffect(() => {
    let alive = true;
    void native
      .migrationStatus()
      .then((value) => {
        if (alive) setStatus(value);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  useEffect(() => {
    if (!working) return;
    let alive = true;
    const timer = setInterval(() => {
      void native
        .migrationStatus()
        .then((value) => {
          if (alive) {
            setStatus(value);
            if (value.status === "complete") onComplete();
          }
        })
        .catch(() => {});
    }, 250);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [working]);
  async function migrate() {
    setBusy(true);
    try {
      const result = await native.migrateLegacy();
      setStatus(result);
      if (result.status === "complete") onComplete();
    } catch (error) {
      setStatus({
        status: "failed",
        errors: [String(error)],
        processed: 0,
        total: 0,
        current: null,
        backup: null,
      });
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="notice" aria-label="Managed Library migration">
      <div>
        <strong>
          {status?.status === "complete"
            ? "Migration complete"
            : "Keep your sounds in the private Library"}
        </strong>
        <p>
          Copy your current folder and preserve Kits, selected sounds, modes,
          sequencer and mixer. Source files stay untouched. Previously baked
          audio keeps its sound; old unsaved editor controls cannot be
          recovered.
        </p>
        {working && (
          <p role="status">
            Copying and validating {status?.processed ?? 0} of{" "}
            {status?.total ?? 0} files
            {status?.current ? ` · ${status.current}` : "…"}
          </p>
        )}
        {status?.status === "failed" && (
          <>
            <p>
              Your folder session remains available. Successfully copied audio
              is retained for a safe retry.
            </p>
            <p role="alert">{status.errors.join("\n")}</p>
          </>
        )}
        {status?.backup && <p>Session backup: {status.backup}</p>}
        {status?.status !== "complete" && (
          <button disabled={working} onClick={() => void migrate()}>
            {working
              ? "Migrating…"
              : status?.status === "failed"
                ? "Retry migration"
                : "Move current folder into Library"}
          </button>
        )}
      </div>
    </section>
  );
}
