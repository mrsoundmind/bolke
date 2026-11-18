import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { Users } from "lucide-react";
import {
  commands,
  type MeetingStatus,
  type Note,
  type Project,
} from "@/bindings";
import { notify } from "@/stores/noticeStore";
import BolkeOrb from "../icons/BolkeOrb";

const BARS = 9;

interface MeetingRecordingProps {
  projects: Project[];
  status: MeetingStatus;
  onSaved: (note: Note) => void;
  onDiscarded: () => void;
  onProjectsChanged: () => void;
}

const clock = (ms: number) => {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
};

/** Recording a meeting (design/wireframes/NotesRecording.dc.html). Opening
 * it starts recording, unless one already is. */
export const MeetingRecording: React.FC<MeetingRecordingProps> = ({
  projects,
  status,
  onSaved,
  onDiscarded,
}) => {
  const { t, i18n } = useTranslation();
  const [title, setTitle] = useState(status.title);
  const [projectId, setProjectId] = useState<number | null>(status.project_id);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const [levels, setLevels] = useState<number[]>(Array(BARS).fill(0));
  const [busy, setBusy] = useState(false);
  const started = useRef(false);

  // Start, or pick up the recording already under way.
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    (async () => {
      const current = await commands.getMeetingStatus();
      if (current.recording) {
        setTitle(current.title);
        setProjectId(current.project_id);
        setStartedAt(Date.now() - current.elapsed_ms);
        return;
      }
      const date = new Intl.DateTimeFormat(i18n.language, {
        day: "numeric",
        month: "short",
      }).format(new Date());
      const firstTitle = t("notes.recording.defaultTitle", { date });
      setTitle(firstTitle);
      const result = await commands.startMeeting(firstTitle, null);
      if (result.status === "ok") {
        setStartedAt(Date.now());
      } else {
        notify({
          title: t("notes.recording.startFailed"),
          detail: result.error,
        });
        onDiscarded();
      }
    })();
  }, []);

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 500);
    const off = listen<number>("meeting-level", ({ payload }) =>
      setLevels((prev) => [...prev.slice(1), payload]),
    );
    return () => {
      clearInterval(id);
      off.then((fn) => fn());
    };
  }, []);

  // Title and project are kept with the recording as they change.
  useEffect(() => {
    if (startedAt == null) return;
    const id = setTimeout(() => commands.updateMeeting(title, projectId), 400);
    return () => clearTimeout(id);
  }, [title, projectId, startedAt]);

  const stop = async () => {
    setBusy(true);
    await commands.updateMeeting(title, projectId);
    const result = await commands.stopMeeting();
    setBusy(false);
    if (result.status === "ok" && result.data) onSaved(result.data);
    else if (result.status === "error")
      notify({ title: t("notes.recording.saveFailed"), detail: result.error });
    else onDiscarded();
  };

  const discard = async () => {
    const ok = await ask(t("notes.recording.discardConfirm"), {
      kind: "warning",
    });
    if (!ok) return;
    setBusy(true);
    await commands.discardMeeting();
    setBusy(false);
    onDiscarded();
  };

  const elapsed = startedAt == null ? 0 : now - startedAt;
  const field =
    "w-full min-h-11 px-4 rounded-full border border-[rgba(140,170,255,0.18)] bg-[#0c1226] text-[14px] text-text select-text focus:outline-none focus:border-accent-soft";

  return (
    <div className="w-full max-w-[720px] mx-auto flex flex-col items-center text-center gap-[26px] px-4 py-6 bg-[radial-gradient(520px_400px_at_50%_40%,rgba(47,107,255,0.16),transparent_70%)]">
      <span className="inline-flex items-center gap-2 min-h-8 px-[14px] rounded-full bg-[rgba(20,30,64,0.8)] border border-edge text-[13px] text-[#c3ceea]">
        <Users size={15} strokeWidth={1.8} aria-hidden="true" />
        {t("notes.recording.badge")}
      </span>

      <BolkeOrb width={180} height={180} glow animated />

      <div
        role="timer"
        className="text-[56px] font-light tracking-[-0.02em] tabular-nums"
      >
        {clock(elapsed)}
      </div>

      <div className="flex items-center gap-1 h-10" aria-hidden="true">
        {levels.map((v, i) => (
          <span
            key={i}
            className="block w-[5px] rounded-[3px] bg-[#8fc2ff] transition-[height] duration-100"
            style={{ height: `${6 + Math.min(1, v) * 30}px` }}
          />
        ))}
      </div>

      <div className="grid grid-cols-2 gap-[14px] w-full max-w-[520px] text-start">
        <div className="flex flex-col gap-[6px]">
          <label
            htmlFor="meeting-title"
            className="text-[12px] text-ink-2 ps-4"
          >
            {t("notes.recording.titleLabel")}
          </label>
          <input
            id="meeting-title"
            className={field}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </div>
        <div className="flex flex-col gap-[6px]">
          <label
            htmlFor="meeting-project"
            className="text-[12px] text-ink-2 ps-4"
          >
            {t("notes.recording.projectLabel")}
          </label>
          <select
            id="meeting-project"
            className={field}
            value={projectId ?? ""}
            onChange={(e) =>
              setProjectId(e.target.value ? Number(e.target.value) : null)
            }
          >
            <option value="">{t("notes.noProject")}</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="flex gap-3 flex-wrap justify-center">
        <button
          type="button"
          disabled={busy}
          onClick={discard}
          className="min-h-12 px-6 rounded-full border border-edge bg-[rgba(20,30,64,0.6)] text-[14px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)] disabled:opacity-50"
        >
          {t("notes.recording.discard")}
        </button>
        <button
          type="button"
          disabled={busy || startedAt == null}
          onClick={stop}
          className="inline-flex items-center gap-[10px] min-h-12 px-6 rounded-full text-white text-[14px] font-medium cursor-pointer bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] shadow-[0_8px_30px_rgba(47,107,255,0.4)] hover:brightness-110 disabled:opacity-50"
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
          >
            <rect x="6" y="6" width="12" height="12" rx="2" />
          </svg>
          {t("notes.recording.stop")}
        </button>
      </div>

      <p className="m-0 max-w-[460px] text-[13px] leading-[1.55] text-ink-2">
        {t("notes.recording.hint")}
      </p>
    </div>
  );
};
