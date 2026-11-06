// Build to SD: review the difference, back up what will be replaced, write, done.
import { Check, ShieldCheck } from "lucide-react";
import { Easing } from "remotion";
import { BUILD_APPLY, BUILD_DONE, SCENES } from "../score";
import { theme } from "../theme";
import { ramp, useStep } from "../timing";
import { Caption, Path, panel, sceneStyle } from "../ui";
import { Glow } from "./Title";

const { from, to } = SCENES.build;
const PANEL = { left: 120, top: 270, width: 1060, height: 750 };
const GREEN = theme.voices[1];
const YELLOW = theme.voices[3];
const BACKUP = { from: BUILD_APPLY + 1, to: BUILD_APPLY + 6 };
const WRITE = { from: BUILD_APPLY + 6, to: BUILD_DONE };

const groups = [
  { title: "Files to add", sign: "+", color: GREEN, files: ["D0/1_02_Kick_02_punch.wav", "D0/4_01_Bass_01_finger_C2.wav"] },
  { title: "Files to replace", sign: "~", color: YELLOW, files: ["D0/2_01_Snare_03_crack.wav"], note: "edited" },
];
const writing = ["D0/1_02_Kick_02_punch.wav", "D0/2_01_Snare_03_crack.wav", "D0/4_01_Bass_01_finger_C2.wav"];
const folders = ["A0", "A1", "B0", "B1", "C0", "C1", "C2", "D0"];

export function Build() {
  const step = useStep();
  const applyX = PANEL.left + 30 + 85;
  const applyY = PANEL.top + 556;
  const stops = [
    { at: BUILD_APPLY - 4, x: applyX + 260, y: applyY - 120 },
    { at: BUILD_APPLY, x: applyX, y: applyY, click: true },
  ];
  const applied = step >= BUILD_APPLY;
  const backup = ramp(step, BACKUP.from, BACKUP.to, Easing.inOut(Easing.quad));
  const write = ramp(step, WRITE.from, WRITE.to, Easing.inOut(Easing.quad));
  const done = step >= BUILD_DONE;
  const doneIn = ramp(step, BUILD_DONE, BUILD_DONE + 1.5, Easing.out(Easing.back(1.6)));
  let row = 0;
  const rowIn = () => ramp(step, from + 3 + row * 1.2, from + 5 + row++ * 1.2);

  return (
    <div style={sceneStyle(step, from, to)}>
      <Glow />
      <Caption
        eyebrow="BUILD TO SD"
        step={step}
        lines={[
          { at: from, text: "Build to the SD card." },
          { at: from + 14, text: "See every change before it happens." },
          { at: BACKUP.from + 6, text: "Backed up and verified first." },
          { at: BUILD_DONE, text: "Exactly what you heard. On the card." },
        ]}
      />
      <div style={{ ...panel, position: "absolute", ...PANEL, padding: "30px 34px", borderColor: done ? `${GREEN}60` : theme.line, opacity: ramp(step, from, from + 2) }}>
        <div style={{ font: `600 30px ${theme.display}`, color: theme.text }}>
          Review changes to <span style={{ fontFamily: theme.mono, fontSize: 26 }}>/Volumes/RAMPLE</span>
        </div>
        <div style={{ font: `500 19px ${theme.body}`, color: theme.soft, marginTop: 10, opacity: rowIn() }}>
          <b style={{ color: GREEN }}>2</b> to add · <b style={{ color: YELLOW }}>1</b> to replace · <b>0</b> to remove ·{" "}
          <b>289</b> already up to date
        </div>
        {groups.map((group) => (
          <div key={group.title} style={{ marginTop: 22 }}>
            <div style={{ font: `600 18px ${theme.body}`, color: theme.text, opacity: rowIn() }}>
              ▾ {group.title} ({group.files.length})
            </div>
            {group.files.map((file) => {
              const t = rowIn();
              return (
                <div key={file} style={{ display: "flex", alignItems: "center", gap: 14, marginTop: 10, opacity: t, transform: `translateX(${(1 - t) * 30}px)` }}>
                  <span style={{ width: 28, height: 28, borderRadius: 7, display: "grid", placeItems: "center", background: `${group.color}26`, color: group.color, font: `700 18px ${theme.mono}` }}>
                    {group.sign}
                  </span>
                  <span style={{ font: `500 19px ${theme.mono}`, color: theme.text }}>{file}</span>
                  {group.note && <span style={{ font: `600 13px ${theme.mono}`, letterSpacing: "0.1em", color: YELLOW, border: `1px solid ${YELLOW}60`, borderRadius: 6, padding: "2px 8px" }}>{group.note.toUpperCase()}</span>}
                </div>
              );
            })}
          </div>
        ))}
        <div style={{ font: `600 18px ${theme.body}`, color: theme.muted, marginTop: 22, opacity: rowIn() }}>▸ Files already up to date (289)</div>
        <div style={{ font: `500 16px/1.5 ${theme.body}`, color: theme.muted, marginTop: 22, maxWidth: 900, opacity: rowIn() }}>
          Replaced files are backed up on this computer before anything changes. <code style={{ fontFamily: theme.mono }}>_save</code>, firmware and files outside Kit folders are never touched.
        </div>
        {/* Actions, then progress */}
        <div style={{ position: "absolute", left: 34, right: 34, top: 530 }}>
          {!applied ? (
            <div style={{ display: "flex", gap: 18, alignItems: "center", opacity: rowIn() }}>
              <div style={{ height: 54, padding: "0 28px", borderRadius: 10, display: "flex", alignItems: "center", background: theme.accent, color: "#121519", font: `700 19px ${theme.body}` }}>
                Apply Build
              </div>
              <span style={{ font: `500 19px ${theme.body}`, color: theme.soft }}>Cancel</span>
            </div>
          ) : (
            <div>
              <ProgressRow
                label={backup < 1 ? "Backing up 1 replaced file…" : "Backup verified"}
                progress={backup}
                color={GREEN}
                icon={backup >= 1}
              />
              <ProgressRow
                label={done ? "Build complete · 3 files written" : `Writing ${writing[Math.min(2, Math.floor(write * 3))]}`}
                progress={write}
                color={theme.accent}
                icon={done}
                dim={step < WRITE.from}
              />
            </div>
          )}
        </div>
      </div>
      <SdCard step={step} write={write} done={done} doneIn={doneIn} backup={backup} />
      <Path stops={stops} step={step} />
    </div>
  );
}

function ProgressRow({ label, progress, color, icon, dim }: { label: string; progress: number; color: string; icon: boolean; dim?: boolean }) {
  return (
    <div style={{ marginBottom: 20, opacity: dim ? 0.35 : 1 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10, font: `500 18px ${theme.body}`, color: icon ? color : theme.text, marginBottom: 10 }}>
        {icon && <Check size={20} strokeWidth={2.8} />}
        <span style={{ fontFamily: label.startsWith("Writing") ? theme.mono : theme.body }}>{label}</span>
      </div>
      <div style={{ height: 8, borderRadius: 4, background: "#2b3139", overflow: "hidden" }}>
        <div style={{ height: "100%", width: `${progress * 100}%`, background: color, borderRadius: 4 }} />
      </div>
    </div>
  );
}

function SdCard({ step, write, done, doneIn, backup }: { step: number; write: number; done: boolean; doneIn: number; backup: number }) {
  const enter = ramp(step, from + 2, from + 6, Easing.out(Easing.back(1.4)));
  const writingNow = write > 0 && !done;
  const pulse = writingNow ? 0.5 + 0.5 * Math.sin(step * 3) : 0;
  return (
    <div style={{ position: "absolute", left: 1270, top: 300, width: 470, opacity: enter, transform: `translateY(${(1 - enter) * 40}px) rotate(${(1 - enter) * 4}deg)` }}>
      <div
        style={{
          position: "relative",
          height: 560,
          borderRadius: 26,
          background: "linear-gradient(160deg, #2a3038, #1b1f25)",
          clipPath: "polygon(0 0, 78% 0, 100% 14%, 100% 100%, 0 100%)",
          boxShadow: done ? `0 0 80px -20px ${theme.voices[1]}` : "0 40px 80px -30px #000",
          padding: "40px 38px",
        }}
      >
        <div style={{ display: "flex", gap: 10, marginBottom: 28 }}>
          {Array.from({ length: 7 }, (_, i) => (
            <div key={i} style={{ width: 22, height: 44, borderRadius: 4, background: "#c8a85a", opacity: 0.85 }} />
          ))}
        </div>
        <div style={{ font: `700 34px ${theme.display}`, letterSpacing: "0.12em", color: theme.text }}>RAMPLE</div>
        <div style={{ font: `500 15px ${theme.mono}`, color: theme.muted, marginTop: 4 }}>SD · FAT32</div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginTop: 30 }}>
          {folders.map((f) => {
            const target = f === "D0";
            return (
              <div
                key={f}
                style={{
                  height: 52,
                  borderRadius: 10,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "0 16px",
                  background: target ? (done ? `${theme.voices[1]}30` : `rgba(246, 128, 97, ${0.1 + pulse * 0.25})`) : "#ffffff08",
                  border: `1px solid ${target ? (done ? theme.voices[1] : theme.accent) : theme.line}`,
                  font: `600 20px ${theme.mono}`,
                  color: target ? theme.text : theme.soft,
                }}
              >
                {f}
                {target && done && <Check size={20} color={theme.voices[1]} strokeWidth={3} style={{ transform: `scale(${doneIn})` }} />}
              </div>
            );
          })}
        </div>
      </div>
      <div
        style={{
          position: "absolute",
          left: -40,
          bottom: -26,
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "14px 20px",
          borderRadius: 14,
          background: "#1f2a27",
          border: `1px solid ${theme.voices[1]}60`,
          color: theme.voices[1],
          font: `600 18px ${theme.body}`,
          opacity: backup >= 1 ? 1 : 0,
          transform: `scale(${backup >= 1 ? 1 : 0.8})`,
          boxShadow: "0 20px 40px -20px #000",
        }}
      >
        <ShieldCheck size={24} />
        Backup verified · Restore any time
      </div>
    </div>
  );
}
