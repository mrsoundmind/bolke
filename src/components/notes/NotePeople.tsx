import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, Copy, Download, RotateCcw, Users } from "lucide-react";
import { commands, type Note, type SpeakerModels } from "@/bindings";
import type { ActiveJobStatus } from "@/stores/transcriptionJobsStore";
import { notify } from "@/stores/noticeStore";
import { copyToClipboard } from "../settings/history/clipboard";
import { colorOf, formatBytes, formatDuration } from "./noteUtils";
import { PersonCard } from "./PersonCards";

/** Matches MIN_PERSON_TALK_MS in commands/notes.rs: less gets no summary. */
const MIN_SUMMARY_TALK_MS = 15_000;

/** "03:12", or "1:03:12" past an hour. */
const clock = (ms: number) => {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
};

const useSpeakerModels = () => {
  const [models, setModels] = useState<SpeakerModels | null>(null);
  const [progress, setProgress] = useState<{
    downloaded: number;
    total: number;
  } | null>(null);
  const reload = useCallback(
    () =>
      commands
        .getSpeakerModels()
        .then(setModels)
        .catch(() => {}),
    [],
  );
  useEffect(() => {
    reload();
    const offs = [
      listen("speaker-models-changed", () => reload()),
      listen<{ downloaded: number; total: number }>(
        "speaker-models-progress",
        ({ payload }) => setProgress(payload),
      ),
    ];
    return () => {
      offs.forEach((off) => off.then((fn) => fn()));
    };
  }, [reload]);
  return { models, progress };
};

const chip =
  "inline-flex items-center gap-2 min-h-9 px-[14px] rounded-full border text-[13px] font-medium whitespace-nowrap cursor-pointer focus-visible:outline-2 focus-visible:outline-accent-soft";
const chipOff =
  "border-[rgba(140,170,255,0.18)] bg-[rgba(20,30,64,0.5)] text-[#c9d4f2] hover:border-[rgba(140,170,255,0.4)] hover:text-white";
const chipOn = "border-[#8fb4ff] bg-[rgba(47,107,255,0.2)] text-white";
const smallBtn =
  "inline-flex items-center gap-2 min-h-9 px-4 rounded-full border border-edge text-[13px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)] disabled:opacity-50";
const primaryBtn =
  "inline-flex items-center gap-2 min-h-10 px-5 rounded-full bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] text-[14px] font-medium text-white cursor-pointer shadow-[0_6px_24px_rgba(47,107,255,0.35)] hover:brightness-110";

interface NotePeopleProps {
  note: Note;
  job: ActiveJobStatus | undefined;
  onNote: (note: Note) => void;
  /** Writes the note's summary, with a card for each person. */
  onSummarize: () => void;
  summarizing: boolean;
}

/** Who said what in a meeting (design/wireframes/MeetingSpeakers.dc.html):
 * everyone who spoke, and tapping one shows only their lines. */
export const NotePeople: React.FC<NotePeopleProps> = ({
  note,
  job,
  onNote,
  onSummarize,
  summarizing,
}) => {
  const { t } = useTranslation();
  const { models, progress } = useSpeakerModels();
  const [who, setWho] = useState<number | null>(null);
  const [naming, setNaming] = useState<number | null>(null);
  const [knownNames, setKnownNames] = useState<string[]>([]);
  const [typed, setTyped] = useState("");
  const [copied, setCopied] = useState(false);
  const people = note.people;

  useEffect(() => {
    if (naming == null) return;
    commands.getVoiceNames().then((r) => {
      if (r.status === "ok") setKnownNames(r.data);
    });
  }, [naming]);

  const nameOf = useCallback(
    (id: number) =>
      people?.people.find((p) => p.id === id)?.name ??
      t("notes.people.person", { number: id }),
    [people, t],
  );

  const lines = useMemo(
    () => (people?.lines ?? []).filter((l) => who == null || l.person === who),
    [people, who],
  );

  if (!models?.available) return null;
  if (note.kind === "idea" && !people) return null;
  if (!note.audio_file && !people) return null;

  const find = async () => {
    const result = await commands.findNotePeople(note.id);
    if (result.status === "ok") onNote(result.data);
    else
      notify({
        title: t("notes.people.failed"),
        detail: result.error,
        level: "error",
      });
  };

  const download = async () => {
    const result = await commands.downloadSpeakerModels();
    if (result.status === "error") {
      notify({
        title: t("notes.people.downloadFailed"),
        detail: result.error,
        level: "error",
      });
    } else if (result.data && !note.text_pending) {
      find();
    }
  };

  const name = async (person: number, value: string) => {
    const result = await commands.nameNotePerson(note.id, person, value);
    if (result.status === "ok") {
      onNote(result.data);
      setNaming(null);
      setTyped("");
      // Joined into someone else: follow them.
      if (
        who === person &&
        !result.data.people?.people.some((p) => p.id === person)
      )
        setWho(null);
    }
  };

  const copyWords = async () => {
    const text = lines
      .map((l) => (who == null ? `${nameOf(l.person)}: ${l.text}` : l.text))
      .join("\n\n");
    if (await copyToClipboard(text)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  const working = note.people_state === "working";
  const percent = job?.state === "running" ? job.percent : 0;
  const frame =
    "flex flex-col gap-[14px] px-5 py-[18px] rounded-[24px] bg-[rgba(12,18,40,0.6)] border border-[rgba(140,170,255,0.14)]";
  const heading = (
    <h2
      id="note-people"
      className="m-0 flex items-center gap-2 text-[15px] font-medium text-[#eaf0ff]"
    >
      <Users size={16} aria-hidden="true" />
      {t("notes.people.title")}
    </h2>
  );

  if (working) {
    return (
      <section aria-labelledby="note-people" className={frame}>
        {heading}
        <div className="flex items-center gap-3">
          <span className="flex-[0_1_260px] h-[6px] rounded-[3px] bg-[#1a2244] overflow-hidden">
            <span
              className="block h-full rounded-[3px] bg-linear-to-r from-[#3a78ff] to-[#8fc2ff] transition-[width]"
              style={{ width: `${percent}%` }}
            />
          </span>
          <span className="text-[13px] text-ink-2">
            {job?.state === "running"
              ? t("notes.people.finding", { percent })
              : t("notes.waiting")}
          </span>
        </div>
      </section>
    );
  }

  if (!people) {
    // A meeting still being written gets its people in the same pass.
    if (note.text_pending && models.downloaded) return null;
    const downloadPct =
      progress && progress.total > 0
        ? Math.floor((progress.downloaded / progress.total) * 100)
        : 0;
    return (
      <section aria-labelledby="note-people" className={frame}>
        {heading}
        <p className="m-0 text-[14px] leading-[1.55] text-[#c9d4f2]">
          {note.people_state === "failed"
            ? t("notes.people.couldNot")
            : t("notes.people.intro")}
        </p>
        {models.downloading ? (
          <div className="flex items-center gap-[14px] flex-wrap">
            <span
              role="progressbar"
              aria-label={t("notes.people.downloading")}
              aria-valuenow={downloadPct}
              aria-valuemin={0}
              aria-valuemax={100}
              className="flex-[1_1_200px] h-[6px] rounded-[3px] bg-[#1a2244] overflow-hidden"
            >
              <span
                className="block h-full rounded-[3px] bg-linear-to-r from-[#3a78ff] to-[#8fc2ff] transition-[width]"
                style={{ width: `${downloadPct}%` }}
              />
            </span>
            <span className="text-[13px] text-ink-2 tabular-nums">
              {t("notes.people.downloadProgress", { percent: downloadPct })}
            </span>
            <button
              type="button"
              className={smallBtn}
              onClick={() => commands.cancelSpeakerModelsDownload()}
            >
              {t("notes.ai.cancel")}
            </button>
          </div>
        ) : models.downloaded ? (
          <div>
            <button
              type="button"
              className={smallBtn}
              onClick={find}
              disabled={note.text_pending}
            >
              {note.people_state === "failed" ? (
                <RotateCcw size={14} aria-hidden="true" />
              ) : (
                <Users size={14} aria-hidden="true" />
              )}
              {note.people_state === "failed"
                ? t("notes.note.tryAgain")
                : t("notes.people.find")}
            </button>
          </div>
        ) : (
          <div>
            <button type="button" className={primaryBtn} onClick={download}>
              <Download size={16} aria-hidden="true" />
              {t("notes.people.download", {
                size: formatBytes(models.size_bytes),
              })}
            </button>
          </div>
        )}
      </section>
    );
  }

  const named = naming != null;
  const choices = Array.from(
    new Set([
      ...people.people.filter((p) => p.name).map((p) => p.name as string),
      ...knownNames,
    ]),
  ).filter((n) => n !== people.people.find((p) => p.id === naming)?.name);
  const selected = people.people.find((p) => p.id === who);
  // Times they spoke: a paragraph straight after their own is the same turn.
  const turnCount = people.lines.filter(
    (l, i) =>
      l.person === who && (i === 0 || people.lines[i - 1].person !== who),
  ).length;
  const firstWords = (id: number) =>
    people.lines.find((l) => l.person === id)?.text.slice(0, 120);

  return (
    <section aria-labelledby="note-people" className={frame}>
      <div className="flex justify-between items-center gap-3 flex-wrap">
        {heading}
        <button
          type="button"
          className={smallBtn}
          onClick={copyWords}
          aria-label={t("notes.people.copy")}
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
          {who == null ? t("notes.people.copy") : t("notes.people.copyTheirs")}
        </button>
      </div>

      <div className="flex gap-2 flex-wrap" role="group">
        <button
          type="button"
          aria-pressed={who == null}
          onClick={() => setWho(null)}
          className={`${chip} ${who == null ? chipOn : chipOff}`}
        >
          {t("notes.people.everyone")}
        </button>
        {people.people.map((p) => (
          <button
            key={p.id}
            type="button"
            aria-pressed={who === p.id}
            onClick={() => setWho(who === p.id ? null : p.id)}
            className={`${chip} ${who === p.id ? chipOn : chipOff} ${
              p.name ? "" : "border-dashed"
            }`}
          >
            <span
              aria-hidden="true"
              className="w-[9px] h-[9px] rounded-full shrink-0"
              style={{ background: colorOf(p.id) }}
            />
            {nameOf(p.id)}
            <small className="text-[12px] text-[#8d9ac0] tabular-nums">
              {formatDuration(p.talk_ms, t)}
            </small>
            {(!p.name || p.guessed) && (
              <span
                role="button"
                tabIndex={0}
                onClick={(e) => {
                  e.stopPropagation();
                  setNaming(p.id);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    e.stopPropagation();
                    setNaming(p.id);
                  }
                }}
                className="text-[#8fb4ff] hover:text-white"
              >
                · {p.name ? t("notes.people.notThem") : t("notes.people.who")}
              </span>
            )}
          </button>
        ))}
      </div>

      {named && (
        <div
          role="dialog"
          aria-labelledby="name-person"
          className="flex flex-col gap-3 px-4 py-[14px] rounded-[18px] bg-[#070a14] border border-[rgba(140,170,255,0.2)]"
        >
          <h3 id="name-person" className="m-0 text-[15px] font-medium">
            {t("notes.people.whoIs", { name: nameOf(naming) })}
          </h3>
          {firstWords(naming) && (
            <p className="m-0 text-[13px] leading-[1.5] text-[#9aa7c7]">
              {t("notes.people.firstWords")}{" "}
              <i className="text-[#d6e1ff]">“{firstWords(naming)}”</i>
            </p>
          )}
          {choices.length > 0 && (
            <div className="flex gap-2 flex-wrap">
              {choices.map((n) => (
                <button
                  key={n}
                  type="button"
                  className={smallBtn}
                  onClick={() => name(naming, n)}
                >
                  {n}
                </button>
              ))}
            </div>
          )}
          <form
            className="flex gap-2 flex-wrap"
            onSubmit={(e) => {
              e.preventDefault();
              if (typed.trim()) name(naming, typed);
            }}
          >
            <input
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              placeholder={t("notes.people.someoneElse")}
              aria-label={t("notes.people.someoneElse")}
              autoFocus={choices.length === 0}
              className="flex-[1_1_200px] min-h-9 px-[14px] rounded-full border border-[rgba(140,170,255,0.3)] bg-[#0c1226] text-[14px] text-text placeholder:text-ink-3 focus:outline-none focus:border-accent-soft"
            />
            <button type="submit" className={smallBtn} disabled={!typed.trim()}>
              {t("notes.people.save")}
            </button>
            <button
              type="button"
              className={smallBtn}
              onClick={() => {
                setNaming(null);
                setTyped("");
              }}
            >
              {t("notes.ai.cancel")}
            </button>
          </form>
          <p className="m-0 text-[12px] text-[#8d9ac0]">
            {t("notes.people.sameNameHint")}
          </p>
        </div>
      )}

      {selected &&
        (() => {
          // Their summary, from the note's AI summary.
          const card = note.summary?.people?.find(
            (c) => c.person === selected.id,
          );
          if (card)
            return <PersonCard card={card} name={nameOf(selected.id)} />;
          const tooLittle = selected.talk_ms < MIN_SUMMARY_TALK_MS;
          return (
            <div className="flex items-center justify-between gap-3 flex-wrap px-4 py-3 rounded-[18px] bg-[rgba(10,18,48,0.45)] border border-[rgba(140,170,255,0.18)]">
              <span className="text-[13px] text-[#c9d4f2]">
                {summarizing
                  ? t("notes.ai.writing")
                  : tooLittle
                    ? t("notes.people.tooLittle", { name: nameOf(selected.id) })
                    : t("notes.people.noSummary", {
                        name: nameOf(selected.id),
                      })}
              </span>
              {!summarizing && !tooLittle && (
                <button
                  type="button"
                  className={smallBtn}
                  onClick={onSummarize}
                >
                  {t("notes.people.writeSummary")}
                </button>
              )}
            </div>
          );
        })()}

      <div className="flex flex-col gap-[2px] max-h-[480px] overflow-y-auto">
        {selected && (
          <div className="flex items-center justify-between gap-3 px-[10px] pt-1 pb-2">
            <span className="text-[13px] text-[#c9d4f2]">
              {t("notes.people.everythingSaid", {
                name: nameOf(selected.id),
                count: turnCount,
              })}
            </span>
            <button
              type="button"
              onClick={() => setNaming(selected.id)}
              className="text-[13px] font-medium text-[#8fb4ff] underline underline-offset-[3px] cursor-pointer hover:text-white"
            >
              {t("notes.people.rename")}
            </button>
          </div>
        )}
        {lines.map((l, i) => {
          // The same person going on: only the time, not their name again.
          const more = i > 0 && lines[i - 1].person === l.person;
          return (
            <div
              key={`${l.start_ms}-${i}`}
              className={`grid grid-cols-[128px_1fr] gap-3 px-[10px] rounded-[12px] text-[14px] leading-[1.5] hover:bg-[rgba(120,150,255,0.06)] ${
                more ? "pt-0 pb-2" : "pt-2 pb-2"
              }`}
            >
              <span
                className="self-start flex items-center gap-[7px] min-w-0 h-[21px] text-[13px] font-semibold"
                style={{ color: colorOf(l.person) }}
              >
                {!more && (
                  <>
                    <span
                      aria-hidden="true"
                      className="w-[9px] h-[9px] rounded-full shrink-0"
                      style={{ background: colorOf(l.person) }}
                    />
                    <span className="truncate">{nameOf(l.person)}</span>
                  </>
                )}
                <span
                  className={`text-[12px] font-normal text-[#7f8db5] tabular-nums ${
                    more ? "pl-4" : ""
                  }`}
                >
                  {clock(l.start_ms)}
                </span>
              </span>
              <span className="text-[#d6e1ff] select-text">{l.text}</span>
            </div>
          );
        })}
      </div>
    </section>
  );
};
