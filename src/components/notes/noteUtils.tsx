import React from "react";
import type { TFunction } from "i18next";
import { FileText, Lightbulb, Users } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import {
  commands,
  type MeetingStatus,
  type NoteKind,
  type SummaryModel,
} from "@/bindings";
import { BYTES_PER_MB, formatModelSize } from "@/lib/utils/format";
import { notify } from "@/stores/noticeStore";
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

/** "2.3 GB", counted like the speech models' sizes. */
export const formatBytes = (bytes: number) =>
  formatModelSize(bytes / BYTES_PER_MB);

export interface SummaryDownload {
  downloaded: number;
  total: number;
}

/** The free summary models, kept current as they download or change. */
export const useSummaryModels = () => {
  const [models, setModels] = useState<SummaryModel[]>([]);
  const [progress, setProgress] = useState<Record<string, SummaryDownload>>({});
  const reload = useCallback(
    () =>
      commands
        .getSummaryModels()
        .then(setModels)
        .catch(() => {}),
    [],
  );
  useEffect(() => {
    reload();
    const offs = [
      listen("summary-models-changed", () => reload()),
      listen<SummaryDownload & { id: string }>(
        "summary-model-progress",
        ({ payload }) =>
          setProgress((p) => ({
            ...p,
            [payload.id]: {
              downloaded: payload.downloaded,
              total: payload.total,
            },
          })),
      ),
    ];
    return () => {
      offs.forEach((off) => off.then((fn) => fn()));
    };
  }, [reload]);
  const recommended = models.find((m) => m.recommended) ?? models[0];
  // A model queued in setup shows as downloading, waiting its turn.
  const downloading = models.find((m) => m.downloading || m.queued);
  return { models, progress, recommended, downloading, reload };
};

/** Starts a summary model download; a failure shows as a notice. */
export const downloadSummaryModel = async (
  id: string,
  t: TFunction,
): Promise<void> => {
  const result = await commands.downloadSummaryModel(id);
  if (result.status === "error") {
    notify({
      title: t("notes.summaryModel.downloadFailed"),
      detail: result.error,
      level: "error",
    });
  }
};

/** A summary model download's percent, from its events or what's on disk. */
export const downloadPercent = (
  model: SummaryModel,
  progress: Record<string, SummaryDownload>,
) => {
  const p = progress[model.id];
  const done = p?.downloaded ?? model.partial_bytes;
  const total = p?.total || model.size_bytes;
  return {
    percent: total > 0 ? Math.min(100, Math.floor((done / total) * 100)) : 0,
    done,
  };
};
