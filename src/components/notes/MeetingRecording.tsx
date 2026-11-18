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
/** Remembers whether the last meeting was a call. */
const CALL_KEY = "bolke.meetingIsCall";

const readCallChoice = () => {
  try {
    return localStorage.getItem(CALL_KEY) !== "false";
  } catch {
    return true;
  }
};

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
  // The others on a video call, from what the computer plays.
  const [canCall, setCanCall] = useState(false);
  const [call, setCall] = useState(readCallChoice);
  const started = useRef(false);

  // Start, or pick up the recording already under way.
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    (async () => {
      const current = await commands.getMeetingStatus();
      const callable = await commands.canRecordCalls().catch(() => false);
      setCanCall(callable);
      if (current.recording) {
        setCall(current.call);
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
      const wantCall = readCallChoice();
      const result = await commands.startMeeting(firstTitle, null, callable);
      if (result.status === "ok") {
        setStartedAt(Date.now());
        // Recorded all along, so turning it on later keeps it in time.
        if (callable && !wantCall) commands.setMeetingCall(false);
        if (callable && !result.data.call) setCanCall(false);
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

  const toggleCall = () => {
    const on = !call;
    setCall(on);
    commands.setMeetingCall(on);
    try {
      localStorage.setItem(CALL_KEY, String(on));
    } catch {
      // Only the default for next time is lost.
    }
  };

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

      {canCall && (
        <button
          type="button"
          role="switch"
          aria-checked={call}
          onClick={toggleCall}
          className="flex items-center gap-[14px] w-full max-w-[520px] px-4 py-3 rounded-[18px] border border-[rgba(140,170,255,0.18)] bg-[rgba(12,18,40,0.6)] text-start cursor-pointer hover:border-[rgba(140,170,255,0.4)] focus-visible:outline-2 focus-visible:outline-accent-soft"
        >
          <span className="flex flex-col gap-[2px] flex-1">
            <span className="text-[14px] font-medium text-text">
              {t("notes.recording.call.title")}
            </span>
            <span className="text-[13px] leading-[1.45] text-ink-2">
              {t("notes.recording.call.body")}
            </span>
          </span>
          <span
            aria-hidden="true"
            className={`relative shrink-0 w-10 h-6 rounded-full transition-colors ${
              call ? "bg-[#2a5ff0]" : "bg-[#2a3352]"
            }`}
          >
            <span
              className={`absolute top-[3px] w-[18px] h-[18px] rounded-full bg-white transition-[left] ${
                call ? "left-[19px]" : "left-[3px]"
              }`}
            />
          </span>
        </button>
      )}

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
