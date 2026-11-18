import React, { useMemo, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { open, ask } from "@tauri-apps/plugin-dialog";
import {
  Pencil,
  PenLine,
  Plus,
  Trash2,
  Upload,
  Users,
  Mic,
} from "lucide-react";
import { commands, type Note, type Project } from "@/bindings";
import type { ActiveJobStatus } from "@/stores/transcriptionJobsStore";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { notify } from "@/stores/noticeStore";
import { Keycaps } from "../shared/Keycaps";
import { KindIcon, formatDuration, formatNoteDate } from "./noteUtils";

const MEDIA_EXTENSIONS = [
  "mp3",
  "m4a",
  "aac",
  "wav",
  "flac",
  "ogg",
  "oga",
  "opus",
  "aif",
  "aiff",
  "mp4",
  "m4v",
  "mov",
  "webm",
];

/** Which notes show: all, one project, or those in none. */
type Filter = "all" | "none" | number;

interface NotesListProps {
  notes: Note[];
  projects: Project[];
  jobs: Record<number, ActiveJobStatus>;
  loaded: boolean;
  onOpen: (id: number) => void;
  onRecordMeeting: () => void;
  onChangeShortcut: () => void;
  onChanged: () => void;
}

const pillBtn =
  "inline-flex items-center gap-[10px] min-h-12 px-6 rounded-full border border-edge bg-[rgba(20,30,64,0.6)] text-[14px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)] focus-visible:outline-2 focus-visible:outline-accent-soft";
const seg =
  "inline-flex items-center gap-2 min-h-10 px-4 rounded-full border text-[13px] font-medium cursor-pointer focus-visible:outline-2 focus-visible:outline-accent-soft";
const tag =
  "inline-flex items-center min-h-6 px-[10px] rounded-full bg-[rgba(20,30,64,0.9)] border border-[rgba(140,170,255,0.16)] text-[12px] text-[#c3ceea]";

export const NotesList: React.FC<NotesListProps> = ({
  notes,
  projects,
  jobs,
  loaded,
  onOpen,
  onRecordMeeting,
  onChangeShortcut,
  onChanged,
}) => {
  const { t, i18n } = useTranslation();
  const osType = useOsType();
  const { getSetting } = useSettings();
  const ideaBinding = getSetting("bindings")?.record_idea?.current_binding;
  const [filter, setFilter] = useState<Filter>("all");
  const [naming, setNaming] = useState<null | "new" | number>(null);
  const [name, setName] = useState("");

  const projectName = (id: number | null) =>
    projects.find((p) => p.id === id)?.name;
  const shown = useMemo(
    () =>
      notes.filter((n) =>
        filter === "all"
          ? true
          : filter === "none"
            ? n.project_id == null
            : n.project_id === filter,
      ),
    [notes, filter],
  );
  const projectId = typeof filter === "number" ? filter : null;
  const unfiled = notes.filter((n) => n.project_id == null).length;

  const writeNote = async () => {
    const result = await commands.createTextNote("", projectId);
    if (result.status === "ok") onOpen(result.data.id);
  };

  const addAudio = async () => {
    const picked = await open({
      multiple: false,
      filters: [{ name: t("notes.audioFiles"), extensions: MEDIA_EXTENSIONS }],
    });
    if (typeof picked !== "string") return;
    const result = await commands.importAudioNote(picked, projectId);
    if (result.status === "ok") onOpen(result.data.id);
    else notify({ title: t("notes.importFailed"), detail: result.error });
  };

  const saveName = async () => {
    const trimmed = name.trim();
    if (trimmed) {
      if (naming === "new") {
        const result = await commands.createProject(trimmed);
        if (result.status === "ok") setFilter(result.data.id);
      } else if (typeof naming === "number") {
        await commands.renameProject(naming, trimmed);
      }
      onChanged();
    }
    setNaming(null);
    setName("");
  };

  const deleteProject = async (project: Project) => {
    const ok = await ask(
      t("notes.deleteProjectConfirm", { name: project.name }),
      {
        kind: "warning",
      },
    );
    if (!ok) return;
    await commands.deleteProject(project.id);
    setFilter("all");
    onChanged();
  };

  const nameField = (
    <form
      className="inline-flex items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        saveName();
      }}
    >
      <input
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && setNaming(null)}
        onBlur={saveName}
        placeholder={t("notes.projectNamePlaceholder")}
        aria-label={t("notes.projectNamePlaceholder")}
        maxLength={80}
        className="min-h-10 w-[180px] px-4 rounded-full border border-accent-soft bg-[#0c1226] text-[13px] text-text placeholder:text-ink-3 select-text focus:outline-none"
      />
    </form>
  );

  const segButton = (key: Filter, label: string, count: number) => {
    const on = filter === key;
    return (
      <button
        key={String(key)}
        type="button"
        aria-pressed={on}
        onClick={() => setFilter(key)}
        className={`${seg} ${
          on
            ? "bg-[#2f6bff] border-[#2f6bff] text-white"
            : "border-[rgba(140,170,255,0.14)] text-[#aeb9d6] hover:border-[rgba(140,170,255,0.35)] hover:text-text"
        }`}
      >
        {label}
        <span className="text-[12px] opacity-75">{count}</span>
      </button>
    );
  };

  const selected =
    typeof filter === "number"
      ? projects.find((p) => p.id === filter)
      : undefined;

  return (
    <div className="w-full max-w-[1040px] mx-auto flex flex-col gap-[22px] px-4 pt-1 pb-6">
      <div className="flex justify-between items-end gap-4 flex-wrap">
        <div className="flex flex-col gap-[6px]">
          <h1 className="text-grad m-0 text-[36px] leading-[1.15] font-light tracking-[-0.02em]">
            {t("notes.title")}
          </h1>
          <p className="m-0 text-[14px] text-ink-2">{t("notes.subtitle")}</p>
        </div>
        <div className="flex gap-[10px] flex-wrap">
          <button type="button" className={pillBtn} onClick={writeNote}>
            <PenLine size={16} aria-hidden="true" />
            {t("notes.writeNote")}
          </button>
          <button type="button" className={pillBtn} onClick={addAudio}>
            <Upload size={16} aria-hidden="true" />
            {t("notes.addAudio")}
          </button>
        </div>
      </div>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
        <div className="relative flex rounded-[24px] bg-linear-160 from-[#2e2266] to-[#15123a] border border-[rgba(167,139,255,0.35)] hover:border-[rgba(167,139,255,0.6)]">
          <button
            type="button"
            onClick={() => commands.toggleIdeaRecording()}
            className="flex-1 flex items-center gap-[18px] p-[22px] pe-[120px] rounded-[24px] text-start cursor-pointer text-[#f3efff] focus-visible:outline-2 focus-visible:outline-[#a78bff]"
          >
            <span
              aria-hidden="true"
              className="shrink-0 w-14 h-14 rounded-full inline-flex items-center justify-center bg-[rgba(167,139,255,0.2)] text-[#c4b5ff] shadow-[0_0_30px_rgba(140,110,255,0.3)]"
            >
              <Mic size={24} strokeWidth={1.8} />
            </span>
            <span className="flex flex-col gap-[6px] min-w-0">
              <span className="text-[17px] font-medium">
                {t("notes.recordIdea")}
              </span>
              <span className="text-[13px] leading-[1.45] text-[#d8d0ff]">
                {ideaBinding ? (
                  <Trans
                    i18nKey="notes.recordIdeaHint"
                    components={{
                      keys: (
                        <Keycaps
                          binding={ideaBinding}
                          osType={osType}
                          size="line"
                        />
                      ),
                    }}
                  />
                ) : (
                  t("notes.recordIdeaHintNoKeys")
                )}
              </span>
            </span>
          </button>
          <button
            type="button"
            onClick={onChangeShortcut}
            className="absolute top-3 end-3 min-h-8 px-3 rounded-full text-[12px] font-medium text-[#d8d0ff] cursor-pointer bg-[rgba(5,10,30,0.35)] border border-[rgba(200,190,255,0.25)] hover:text-white hover:border-[rgba(200,190,255,0.5)] focus-visible:outline-2 focus-visible:outline-[#a78bff]"
          >
            {t("notes.changeShortcut")}
          </button>
        </div>
        <button
          type="button"
          onClick={onRecordMeeting}
          className="flex items-center gap-[18px] p-[22px] rounded-[24px] text-start cursor-pointer bg-linear-160 from-[#1d3fa8] to-[#10225e] border border-[rgba(140,170,255,0.25)] text-white hover:border-[rgba(140,170,255,0.5)] focus-visible:outline-2 focus-visible:outline-accent-soft"
        >
          <span
            aria-hidden="true"
            className="shrink-0 w-14 h-14 rounded-full inline-flex items-center justify-center bg-[rgba(5,10,30,0.35)] text-[#d6e1ff]"
          >
            <Users size={24} strokeWidth={1.8} />
          </span>
          <span className="flex flex-col gap-[6px] min-w-0">
            <span className="text-[17px] font-medium">
              {t("notes.recordMeeting")}
            </span>
            <span className="text-[13px] leading-[1.45] text-[#d6e1ff]">
              {t("notes.recordMeetingHint")}
            </span>
          </span>
        </button>
      </div>

      <div className="flex gap-2 flex-wrap items-center">
        {segButton("all", t("notes.allNotes"), notes.length)}
        {projects.map((p) =>
          naming === p.id ? (
            <React.Fragment key={p.id}>{nameField}</React.Fragment>
          ) : (
            segButton(p.id, p.name, p.note_count)
          ),
        )}
        {unfiled > 0 &&
          projects.length > 0 &&
          segButton("none", t("notes.noProject"), unfiled)}
        {naming === "new" ? (
          nameField
        ) : (
          <button
            type="button"
            onClick={() => {
              setName("");
              setNaming("new");
            }}
            className={`${seg} border-dashed border-[rgba(140,170,255,0.25)] text-[#aeb9d6] hover:text-text hover:border-[rgba(140,170,255,0.45)]`}
          >
            <Plus size={14} aria-hidden="true" />
            {t("notes.newProject")}
          </button>
        )}
        {selected && naming === null && (
          <span className="inline-flex gap-1 ms-1">
            <button
              type="button"
              aria-label={t("notes.renameProject")}
              title={t("notes.renameProject")}
              onClick={() => {
                setName(selected.name);
                setNaming(selected.id);
              }}
              className="w-9 h-9 rounded-full inline-flex items-center justify-center text-ink-2 cursor-pointer hover:bg-wash hover:text-text"
            >
              <Pencil size={15} aria-hidden="true" />
            </button>
            <button
              type="button"
              aria-label={t("notes.deleteProject")}
              title={t("notes.deleteProject")}
              onClick={() => deleteProject(selected)}
              className="w-9 h-9 rounded-full inline-flex items-center justify-center text-ink-2 cursor-pointer hover:bg-wash hover:text-text"
            >
              <Trash2 size={15} aria-hidden="true" />
            </button>
          </span>
        )}
      </div>

      <div className="flex flex-col gap-[10px]">
        {loaded && shown.length === 0 && (
          <p className="m-0 px-5 py-4 rounded-[20px] bg-card border border-line text-[13px] text-ink-2">
            {filter === "all" ? t("notes.empty") : t("notes.emptyProject")}
          </p>
        )}
        {shown.map((note) => {
          const job = note.entry_id != null ? jobs[note.entry_id] : undefined;
          const percent = job?.state === "running" ? job.percent : 0;
          const project = projectName(note.project_id);
          const meta = [
            formatNoteDate(note.created_at, i18n.language),
            formatDuration(note.duration_ms, t),
          ]
            .filter(Boolean)
            .join(" · ");
          return (
            <button
              type="button"
              key={note.id}
              onClick={() => onOpen(note.id)}
              className="flex items-start gap-4 px-5 py-4 rounded-[20px] text-start cursor-pointer bg-[rgba(14,20,44,0.8)] border border-[rgba(140,170,255,0.1)] hover:border-[rgba(140,170,255,0.3)] focus-visible:outline-2 focus-visible:outline-accent-soft"
            >
              <KindIcon kind={note.kind} />
              <span className="flex-1 min-w-0 flex flex-col gap-[7px]">
                <span className="flex items-center gap-[10px] flex-wrap">
                  <span className="text-[15px] font-medium text-text">
                    {note.title || t("notes.untitled")}
                  </span>
                  {project && <span className={tag}>{project}</span>}
                  {note.summary && (
                    <span className={`${tag} text-[#c9d8ff]`}>
                      {t("notes.summaryTag")}
                    </span>
                  )}
                </span>
                {note.text_pending ? (
                  <span className="flex items-center gap-3">
                    <span className="flex-[0_1_220px] h-[6px] rounded-[3px] bg-[#1a2244] overflow-hidden">
                      <span
                        className="block h-full rounded-[3px] bg-linear-to-r from-[#3a78ff] to-[#8fc2ff] transition-[width]"
                        style={{ width: `${percent}%` }}
                      />
                    </span>
                    <span className="text-[13px] text-ink-2">
                      {job?.state === "running"
                        ? t("notes.writing", { percent })
                        : t("notes.waiting")}
                    </span>
                  </span>
                ) : (
                  note.text && (
                    <span className="text-[13px] leading-[1.5] text-[#b7c3e0] truncate">
                      {note.text}
                    </span>
                  )
                )}
                <span className="text-[12px] text-ink-3">{meta}</span>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
};
