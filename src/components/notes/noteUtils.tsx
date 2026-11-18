import React from "react";
import type { TFunction } from "i18next";
import { FileText, Lightbulb, Users } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { commands, type MeetingStatus, type NoteKind } from "@/bindings";
import type { OSType } from "@/lib/utils/keyboard";

/** "18 min", or "45 s" under a minute. */
export const formatDuration = (ms: number | null, t: TFunction) => {
  if (ms == null || ms <= 0) return null;
  const seconds = Math.round(ms / 1000);
  return seconds < 60
    ? t("notes.durationSeconds", { count: seconds })
    : t("notes.durationMinutes", { count: Math.round(seconds / 60) });
};

/** "16 Nov, 4:10 pm" in the app's language. */
export const formatNoteDate = (seconds: number, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(seconds * 1000));

export const wordCount = (text: string) =>
  text.trim() ? text.trim().split(/\s+/).length : 0;

/** The round kind icon on a note row; ideas are violet. */
export const KindIcon: React.FC<{ kind: NoteKind; size?: number }> = ({
  kind,
  size = 40,
}) => {
  const Icon =
    kind === "idea" ? Lightbulb : kind === "meeting" ? Users : FileText;
  return (
    <span
      aria-hidden="true"
      style={{ width: size, height: size }}
      className={`shrink-0 inline-flex items-center justify-center rounded-full ${
        kind === "idea"
          ? "bg-[rgba(167,139,255,0.16)] text-[#c4b5ff]"
          : "bg-[rgba(47,107,255,0.14)] text-[#8fb4ff]"
      }`}
    >
      <Icon size={Math.round(size * 0.45)} strokeWidth={1.8} />
    </span>
  );
};

/** A URL the webview can play a recording from. */
export const recordingUrl = async (
  fileName: string,
  osType: OSType,
): Promise<string | null> => {
  const result = await commands.getAudioFilePath(fileName);
  if (result.status !== "ok") return null;
  if (osType === "linux") {
    const data = await readFile(result.data);
    return URL.createObjectURL(new Blob([data], { type: "audio/wav" }));
  }
  return convertFileSrc(result.data, "asset");
};

const IDLE: MeetingStatus = {
  recording: false,
  title: "",
  project_id: null,
  elapsed_ms: 0,
};

/** The meeting recording under way, checked every second. */
export const useMeetingStatus = () => {
  const [status, setStatus] = useState<MeetingStatus>(IDLE);
  useEffect(() => {
    let alive = true;
    const check = () =>
      commands
        .getMeetingStatus()
        .then((s) => alive && setStatus(s))
        .catch(() => {});
    check();
    const id = setInterval(check, 1000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);
  return status;
};

/** Runs `onChange` when Rust saves a note (an idea, a meeting). */
export const useNotesChanged = (onChange: () => void) => {
  useEffect(() => {
    const off = listen("notes-changed", () => onChange());
    return () => {
      off.then((fn) => fn());
    };
  }, [onChange]);
};
