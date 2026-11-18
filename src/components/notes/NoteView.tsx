import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  Check,
  Copy,
  Download,
  FolderOpen,
  RotateCcw,
  Sparkles,
  Trash2,
} from "lucide-react";
import {
  commands,
  type Note,
  type Project,
  type SummarySource,
} from "@/bindings";
import type { ActiveJobStatus } from "@/stores/transcriptionJobsStore";
import { useOsType } from "@/hooks/useOsType";
import { AudioPlayer } from "../ui/AudioPlayer";
import { copyToClipboard } from "../settings/history/clipboard";
import {
  downloadPercent,
  downloadSummaryModel,
  formatBytes,
  formatDuration,
  formatNoteDate,
  recordingUrl,
  useNotesChanged,
  useSummaryModels,
  wordCount,
} from "./noteUtils";
import { NotePeople } from "./NotePeople";
import { listen } from "@tauri-apps/api/event";

/** Typing pauses this long before the text is saved. */
const SAVE_DELAY_MS = 700;

type SummaryState = "idle" | "writing" | "failed";

interface NoteViewProps {
  id: number;
  projects: Project[];
  jobs: Record<number, ActiveJobStatus>;
  onBack: () => void;
  onDeleted: () => void;
  onOpenPostProcess: () => void;
}

const iconBtn =
  "w-9 h-9 rounded-full inline-flex items-center justify-center bg-[rgba(255,255,255,0.06)] text-[#c3ceea] cursor-pointer hover:bg-[rgba(255,255,255,0.14)] hover:text-white focus-visible:outline-2 focus-visible:outline-accent-soft";
const primaryBtn =
  "inline-flex items-center gap-2 min-h-11 px-5 rounded-full bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] text-[14px] font-medium text-white cursor-pointer shadow-[0_6px_24px_rgba(47,107,255,0.35)] hover:brightness-110";
const lightBtn =
  "inline-flex items-center gap-2 min-h-10 px-5 rounded-full bg-[rgba(5,10,30,0.4)] border border-[rgba(200,215,255,0.3)] text-[13px] font-medium text-white cursor-pointer hover:border-[rgba(200,215,255,0.6)] disabled:opacity-50";

/** One note (design/wireframes/NoteOpen.dc.html): its recording, AI summary
 * and full text, which can be edited. */
export const NoteView: React.FC<NoteViewProps> = ({
  id,
  projects,
  jobs,
  onBack,
  onDeleted,
  onOpenPostProcess,
}) => {
  const { t, i18n } = useTranslation();
  const osType = useOsType();
  const [note, setNote] = useState<Note | null>(null);
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const [copied, setCopied] = useState(false);
  const [summaryState, setSummaryState] = useState<SummaryState>("idle");
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [summarySource, setSummarySource] = useState<SummarySource | null>(
    null,
  );
  const saveTimer = useRef<ReturnType<typeof setTimeout>>();
  const dirty = useRef(false);
  const textRef = useRef("");
  textRef.current = text;

  const load = useCallback(async () => {
    const result = await commands.getNote(id);
    if (result.status !== "ok") return;
    setNote(result.data);
    setTitle(result.data.title);
    // Don't overwrite what's being typed.
    if (!dirty.current) setText(result.data.text);
  }, [id]);

  const summaryModels = useSummaryModels();
  const [summaryPart, setSummaryPart] = useState<{
    part: number;
    parts: number;
  } | null>(null);

  // What writes the summary changes when a summary model arrives or goes.
  useEffect(() => {
    commands
      .getSummarySource()
      .then(setSummarySource)
      .catch(() => setSummarySource(null));
  }, [summaryModels.models]);

  // "Part 2 of 4" while a long note is summarized.
  useEffect(() => {
    const off = listen<{ note_id: number; part: number; parts: number }>(
      "summary-progress",
      ({ payload }) => {
        if (payload.note_id === id)
          setSummaryPart({ part: payload.part, parts: payload.parts });
      },
    );
    return () => {
      off.then((fn) => fn());
    };
  }, [id]);

  useEffect(() => {
    load();
  }, [load]);
  // Who said what is saved by Rust when it's found.
  useNotesChanged(load);

  // The transcript arrives when its job ends.
  const job = note?.entry_id != null ? jobs[note.entry_id] : undefined;
  const hasJob = job != null;
  useEffect(() => {
    if (note?.text_pending && !hasJob) load();
  }, [hasJob]);

  const saveText = useCallback(
    async (value: string) => {
      setSaving(true);
      const result = await commands.updateNote(id, {
        title: null,
        text: value,
        kind: null,
      });
      dirty.current = false;
      setSaving(false);
      if (result.status === "ok") setNote(result.data);
    },
    [id],
  );

  // Save what's typed when leaving the note another way (the sidebar).
  useEffect(
    () => () => {
      clearTimeout(saveTimer.current);
      if (dirty.current) {
        commands.updateNote(id, {
          title: null,
          text: textRef.current,
          kind: null,
        });
      }
    },
    [id],
  );

  const onTextChange = (value: string) => {
    setText(value);
    dirty.current = true;
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => saveText(value), SAVE_DELAY_MS);
  };

  const flushAndBack = async () => {
    if (dirty.current) {
      clearTimeout(saveTimer.current);
      await saveText(text);
    }
    onBack();
  };

  const saveTitle = async () => {
    if (!note || title.trim() === note.title) return;
    const result = await commands.updateNote(id, {
      title,
      text: null,
      kind: null,
    });
    if (result.status === "ok") setNote(result.data);
  };

  const move = async (projectId: number | null) => {
    const result = await commands.moveNote(id, projectId);
    if (result.status === "ok") setNote(result.data);
  };

  const copy = async () => {
    if (await copyToClipboard(text)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  const remove = async () => {
    const ok = await ask(t("notes.note.deleteConfirm"), { kind: "warning" });
    if (!ok) return;
    await commands.deleteNote(id);
    onDeleted();
  };

  const summarize = async () => {
    if (dirty.current) {
      clearTimeout(saveTimer.current);
      await saveText(text);
    }
    setSummaryState("writing");
    setSummaryError(null);
    setSummaryPart(null);
    const result = await commands.summarizeNote(id);
    if (result.status === "ok") {
      setNote(result.data);
      setSummaryState("idle");
    } else {
      setSummaryState("failed");
      setSummaryError(result.error);
    }
  };

  const tick = async (index: number, done: boolean) => {
    const result = await commands.setActionPointDone(id, index, done);
    if (result.status === "ok") setNote(result.data);
  };

  if (!note) return null;

  const kindLabel = t(`notes.kind.${note.kind}`);
  const meta = [
    formatNoteDate(note.created_at, i18n.language),
    formatDuration(note.duration_ms, t),
    note.text_pending ? null : t("notes.words", { count: wordCount(text) }),
  ]
    .filter(Boolean)
    .join(" · ");
  const percent = job?.state === "running" ? job.percent : 0;
  const noProvider = summaryError === "no_ai_provider";
  const { recommended, downloading } = summaryModels;
  const download = downloading
    ? downloadPercent(downloading, summaryModels.progress)
    : { percent: 0, done: 0 };
  const summary = note.summary;
  const tag =
    "inline-flex items-center min-h-[30px] px-3 rounded-full bg-[rgba(20,30,64,0.9)] border border-[rgba(140,170,255,0.16)] text-[12px] font-medium text-[#c3ceea]";

  return (
    <div className="w-full max-w-[880px] mx-auto flex flex-col gap-5 px-4 pt-1 pb-6">
      <div className="flex justify-between items-center gap-3 flex-wrap">
        <button
          type="button"
          onClick={flushAndBack}
          className="inline-flex items-center gap-[10px] min-h-11 px-5 rounded-full border border-edge bg-[rgba(20,30,64,0.6)] text-[14px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)]"
        >
          <ArrowLeft size={16} aria-hidden="true" />
          {note.kind === "meeting" ? t("meetings.title") : t("notes.title")}
        </button>
        <div className="flex gap-1">
          <button
            type="button"
            className={iconBtn}
            onClick={copy}
            aria-label={t("notes.note.copy")}
            title={copied ? t("notes.note.copied") : t("notes.note.copy")}
          >
            {copied ? <Check size={16} /> : <Copy size={16} />}
          </button>
          {note.audio_file && (
            <button
              type="button"
              className={iconBtn}
              onClick={() => commands.openRecordingsFolder()}
              aria-label={t("notes.note.showFile")}
              title={t("notes.note.showFile")}
            >
              <FolderOpen size={16} />
            </button>
          )}
          <button
            type="button"
            className={iconBtn}
            onClick={remove}
            aria-label={t("notes.note.delete")}
            title={t("notes.note.delete")}
          >
            <Trash2 size={16} />
          </button>
        </div>
      </div>

      <div className="flex flex-col gap-3">
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={saveTitle}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          placeholder={t("notes.untitled")}
          aria-label={t("notes.recording.titleLabel")}
          className="text-grad m-0 w-full bg-transparent border-0 p-0 text-[34px] font-light tracking-[-0.02em] select-text placeholder:text-ink-3 focus:outline-none"
        />
        <div className="flex gap-2 flex-wrap items-center">
          <span className={tag}>{kindLabel}</span>
          <select
            aria-label={t("notes.note.moveTo")}
            value={note.project_id ?? ""}
            onChange={(e) =>
              move(e.target.value ? Number(e.target.value) : null)
            }
            className={`${tag} cursor-pointer focus:outline-none`}
          >
            <option value="">{t("notes.noProject")}</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <span className="text-[13px] text-ink-3">{meta}</span>
        </div>
      </div>

      {note.audio_file && (
        <div className="px-4 py-2 rounded-full bg-[rgba(14,20,44,0.8)] border border-[rgba(140,170,255,0.12)]">
          <AudioPlayer
            key={note.audio_file}
            onLoadRequest={() => recordingUrl(note.audio_file!, osType)}
            className="w-full"
          />
        </div>
      )}

      <NotePeople note={note} job={job} onNote={setNote} />

      <section
        aria-labelledby="note-summary"
        className="flex flex-col gap-[14px] px-6 py-[22px] rounded-[24px] bg-linear-160 from-[#1d3fa8] to-[#10225e] border border-[rgba(140,170,255,0.25)]"
      >
        <div className="flex justify-between items-center gap-3 flex-wrap">
          <h2
            id="note-summary"
            className="m-0 flex items-center gap-2 text-[15px] font-medium text-[#eaf0ff]"
          >
            <Sparkles size={16} aria-hidden="true" />
            {t("notes.ai.title")}
          </h2>
          {summary && summaryState !== "writing" && (
            <button type="button" className={lightBtn} onClick={summarize}>
              {t("notes.ai.again")}
            </button>
          )}
        </div>

        {summaryState === "writing" ? (
          <div className="flex flex-col gap-1">
            <p className="m-0 text-[14px] text-[#d6e1ff] animate-pulse">
              {summaryPart
                ? t("notes.ai.writingPart", summaryPart)
                : t("notes.ai.writing")}
            </p>
            {summaryPart && (
              <p className="m-0 text-[13px] text-[#a9bdeb]">
                {t("notes.ai.slowHint")}
              </p>
            )}
          </div>
        ) : summary ? (
          <div className="grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-[22px]">
            <div className="flex flex-col gap-2">
              <span className="text-[12px] font-semibold tracking-[0.06em] uppercase text-[#a9c2ff]">
                {t("notes.ai.summary")}
              </span>
              <p className="m-0 text-[14px] leading-[1.6] text-[#eaf0ff] select-text whitespace-pre-wrap">
                {summary.summary}
              </p>
            </div>
            <div className="flex flex-col gap-[10px]">
              <span className="text-[12px] font-semibold tracking-[0.06em] uppercase text-[#a9c2ff]">
                {t("notes.ai.actions")}
              </span>
              {(summary.action_points ?? []).length === 0 ? (
                <p className="m-0 text-[14px] text-[#d6e1ff]">
                  {t("notes.ai.noActions")}
                </p>
              ) : (
                (summary.action_points ?? []).map((point, index) => (
                  <label
                    key={index}
                    className="flex items-start gap-[10px] text-[14px] leading-[1.5] text-[#e2e9fa] cursor-pointer"
                  >
                    <input
                      type="checkbox"
                      checked={point.done}
                      onChange={(e) => tick(index, e.target.checked)}
                      className="w-[18px] h-[18px] mt-[2px] shrink-0 accent-[#2f6bff]"
                    />
                    <span
                      className={point.done ? "line-through opacity-70" : ""}
                    >
                      {point.text}
                    </span>
                  </label>
                ))
              )}
            </div>
          </div>
        ) : note.text_pending ? (
          <p className="m-0 text-[14px] leading-[1.55] text-[#d6e1ff]">
            {t("notes.ai.notReady")}
          </p>
        ) : downloading ? (
          <div className="flex flex-col gap-3">
            <p className="m-0 text-[14px] leading-[1.55] text-[#d6e1ff]">
              {t(
                downloading.queued ? "notes.ai.queued" : "notes.ai.downloading",
              )}
            </p>
            <div className="flex items-center gap-[14px] flex-wrap">
              <div
                role="progressbar"
                aria-label={t("notes.ai.downloadLabel")}
                aria-valuenow={download.percent}
                aria-valuemin={0}
                aria-valuemax={100}
                className="flex-[1_1_200px] h-[6px] rounded-[3px] bg-[rgba(5,10,30,0.45)] overflow-hidden"
              >
                <span
                  className="block h-full rounded-[3px] bg-linear-to-r from-[#8fc2ff] to-white transition-[width]"
                  style={{ width: `${download.percent}%` }}
                />
              </div>
              <span className="text-[13px] text-[#c3d2f5] tabular-nums whitespace-nowrap">
                {t("notes.ai.downloadProgress", {
                  percent: download.percent,
                  done: formatBytes(download.done),
                  total: formatBytes(downloading.size_bytes),
                })}
              </span>
              <button
                type="button"
                className={lightBtn}
                onClick={() =>
                  commands.cancelSummaryModelDownload(downloading.id)
                }
              >
                {t("notes.ai.cancel")}
              </button>
            </div>
          </div>
        ) : summarySource === "none" && recommended ? (
          <div className="flex flex-col gap-3 items-start">
            <p className="m-0 text-[14px] leading-[1.55] text-[#d6e1ff]">
              {t("notes.ai.introDownload", {
                size: formatBytes(recommended.size_bytes),
              })}
            </p>
            <div className="flex items-center gap-4 flex-wrap">
              <button
                type="button"
                className={primaryBtn}
                onClick={() => downloadSummaryModel(recommended.id, t)}
              >
                <Download size={16} aria-hidden="true" />
                {t("notes.ai.download", {
                  size: formatBytes(recommended.size_bytes),
                })}
              </button>
              <button
                type="button"
                onClick={onOpenPostProcess}
                className="text-[13px] font-medium text-[#bfd2ff] underline underline-offset-[3px] cursor-pointer hover:text-white"
              >
                {t("notes.ai.useService")}
              </button>
            </div>
          </div>
        ) : (
          <div className="flex flex-col gap-3 items-start">
            <p className="m-0 text-[14px] leading-[1.55] text-[#d6e1ff]">
              {summarySource === "local_model"
                ? t("notes.ai.introLocal")
                : summarySource === "apple_intelligence"
                  ? t("notes.ai.introOnDevice")
                  : t("notes.ai.intro")}
            </p>
            <button
              type="button"
              className={lightBtn}
              onClick={summarize}
              disabled={!text.trim()}
            >
              {t("notes.ai.write")}
            </button>
          </div>
        )}

        {/* With nothing to write it, the card above offers the download. */}
        {summaryState === "failed" && !noProvider && (
          <p role="alert" className="m-0 text-[13px] text-[#ffd4cf]">
            {t("notes.ai.failed", { error: summaryError })}
          </p>
        )}
      </section>

      <div className="flex flex-col gap-2">
        <div className="flex justify-between items-center gap-3">
          <label htmlFor="note-text" className="text-[14px] font-medium">
            {t("notes.note.fullText")}
          </label>
          <span className="text-[12px] text-ink-3">
            {note.text_pending
              ? ""
              : saving
                ? t("notes.note.saving")
                : t("notes.note.saved")}
          </span>
        </div>
        {note.text_pending ? (
          <div className="flex flex-col gap-3 items-start px-5 py-4 rounded-[20px] bg-[rgba(14,20,44,0.8)] border border-[rgba(140,170,255,0.12)]">
            <div className="flex items-center gap-3 w-full">
              {job && (
                <span className="flex-[0_1_260px] h-[6px] rounded-[3px] bg-[#1a2244] overflow-hidden">
                  <span
                    className="block h-full rounded-[3px] bg-linear-to-r from-[#3a78ff] to-[#8fc2ff] transition-[width]"
                    style={{ width: `${percent}%` }}
                  />
                </span>
              )}
              <span className="text-[13px] text-ink-2">
                {job?.state === "running"
                  ? t("notes.writing", { percent })
                  : job
                    ? t("notes.waiting")
                    : t("notes.note.notWritten")}
              </span>
            </div>
            {!job && note.entry_id != null && (
              <button
                type="button"
                onClick={() =>
                  commands.retryHistoryEntryTranscription(note.entry_id!)
                }
                className="inline-flex items-center gap-2 min-h-9 px-4 rounded-full border border-edge text-[13px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)]"
              >
                <RotateCcw size={14} aria-hidden="true" />
                {t("notes.note.tryAgain")}
              </button>
            )}
          </div>
        ) : (
          <textarea
            id="note-text"
            value={text}
            onChange={(e) => onTextChange(e.target.value)}
            placeholder={t("notes.note.textPlaceholder")}
            rows={14}
            className="w-full min-h-[260px] resize-y rounded-[20px] border border-[rgba(140,170,255,0.18)] bg-[#0c1226] px-5 py-4 text-[15px] leading-[1.65] text-text placeholder:text-ink-3 select-text focus:outline-none focus:border-accent-soft"
          />
        )}
      </div>
    </div>
  );
};
