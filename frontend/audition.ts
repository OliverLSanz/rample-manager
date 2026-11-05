import { useCallback, useEffect, useRef, useState } from "react";
import * as native from "./native";

/**
 * A Library audition owned by one view. Requests play in order; a newer
 * request or a stop wins over a slow preparation that finishes later. The
 * session closes with the view, and is reopened if another view took it.
 */
export function useSampleAudition() {
  const owner = useRef<string | null>(null);
  const alive = useRef(true);
  const queue = useRef(Promise.resolve());
  const request = useRef(0);
  const [playing, setPlaying] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      request.current++;
      const session = owner.current;
      owner.current = null;
      if (session)
        void native.stopSampleAudition(session, true).catch(() => {});
    };
  }, []);
  /** Plays the Original, or the Rample preparation with `rample`. */
  const hear = useCallback(async (id: string, rample = false) => {
    const current = ++request.current;
    setPlaying(id);
    setError(null);
    const task = queue.current
      .catch(() => {})
      .then(async () => {
        for (let attempt = 0; attempt < 2; attempt++) {
          if (!alive.current || current !== request.current) return;
          try {
            if (!owner.current)
              owner.current = await native.openSampleAudition();
            if (!alive.current || current !== request.current) return;
            if (rample) await native.auditionSample(id, owner.current, true);
            else await native.auditionSample(id, owner.current);
            return;
          } catch (e) {
            // Another view may have taken the audition: reopen once.
            owner.current = null;
            if (attempt === 1) throw e;
          }
        }
      });
    queue.current = task;
    try {
      await task;
    } catch (e) {
      if (alive.current && current === request.current) {
        setPlaying(null);
        setError(String(e));
      }
    }
  }, []);
  const stop = useCallback(async () => {
    request.current++;
    setPlaying(null);
    if (owner.current)
      await native.stopSampleAudition(owner.current).catch((e) => {
        if (alive.current) setError(String(e));
      });
  }, []);
  return { hear, stop, playing, error };
}
