import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Note, type NoteAnswer } from "@/bindings";
import { colorOf } from "./noteUtils";

interface NoteAskProps {
  note: Note;
  /** Shows the transcript at the moment a quote was said. */
  onJump: (startMs: number | null) => void;
}

const clock = (ms: number) => {
  const s = Math.floor(ms / 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return s >= 3600
    ? `${Math.floor(s / 3600)}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}`
    : `${Math.floor(s / 60)}:${two(s % 60)}`;
};

/** Questions about one meeting, answered from its own words
 * (design/wireframes/MeetingPage.dc.html, the Ask tab). */
export const NoteAsk: React.FC<NoteAskProps> = ({ note, onJump }) => {
  const { t } = useTranslation();
  const [answers, setAnswers] = useState<NoteAnswer[]>([]);
  const [typed, setTyped] = useState("");
  const [asking, setAsking] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    commands.listNoteAnswers(note.id).then((r) => {
      if (r.status === "ok") setAnswers(r.data);
    });
  }, [note.id]);

  useEffect(() => {
    end.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [answers.length, asking]);

  const nameOf = (id: number) =>
    note.people?.people.find((p) => p.id === id)?.name ??
    t("notes.people.person", { number: id });

  // Ready-made questions: the meeting as a whole, then the people who
  // talked most.
  const suggestions = useMemo(() => {
    const talkers = [...(note.people?.people ?? [])]
      .filter((p) => p.name)
      .sort((a, b) => b.talk_ms - a.talk_ms)
      .slice(0, 2)
      .map((p) => t("notes.ask.whatDid", { name: p.name }));
    const all = [
      t("notes.ask.decided"),
      t("notes.ask.open"),
      ...talkers,
      t("notes.ask.next"),
    ];
    const asked = new Set(answers.map((a) => a.question));
    return all.filter((q) => !asked.has(q));
  }, [note.people, answers, t]);

  const ask = async (question: string) => {
    const q = question.trim();
    if (!q || asking) return;
    setAsking(q);
    setError(null);
    setTyped("");
    const result = await commands.askNote(note.id, q);
    setAsking(null);
    if (result.status === "ok") {
      setAnswers((a) => [...a, result.data]);
    } else {
      setTyped(q);
      setError(
        result.error === "no_ai_provider"
          ? t("notes.ask.noModel")
          : t("notes.ask.failed"),
      );
    }
  };

  const clear = async () => {
    const result = await commands.clearNoteAnswers(note.id);
    if (result.status === "ok") setAnswers([]);
  };

  const mine =
    "self-end max-w-[85%] md:max-w-[70%] px-4 py-[10px] rounded-[18px_18px_6px_18px] bg-[#2f6bff] text-white text-[14.5px] leading-[1.5] select-text";

  return (
    <section aria-label={t("notes.tabs.ask")} className="flex flex-col gap-4">
      {answers.length === 0 && !asking && (
        <p className="m-0 text-[14px] leading-[1.55] text-[#c9d4f2]">
          {t("notes.ask.intro")}
        </p>
      )}

      {answers.map((a) => (
        <React.Fragment key={a.id}>
          <div className={mine}>{a.question}</div>
          <div className="self-start max-w-full md:max-w-[86%] flex flex-col gap-3 px-[18px] py-4 rounded-[18px_18px_18px_6px] bg-[rgba(14,20,44,0.85)] border border-[rgba(140,170,255,0.14)]">
            <p className="m-0 text-[15px] leading-[1.6] text-[#eaf0ff] select-text">
              {a.answer}
            </p>
            {a.quotes.length > 0 && (
              <>
                <span className="text-[10.5px] font-semibold tracking-[0.07em] uppercase text-[#8fb4ff]">
                  {t("notes.ask.fromMeeting")}
                </span>
                <div className="flex flex-col gap-[6px]">
                  {a.quotes.map((q, i) => (
                    <button
                      key={i}
                      type="button"
                      onClick={() => onJump(q.start_ms)}
                      className="grid grid-cols-[auto_1fr] md:grid-cols-[auto_1fr_auto] items-baseline gap-x-2 gap-y-[2px] text-start px-3 py-2 rounded-[12px] bg-[rgba(140,170,255,0.07)] text-[13px] leading-[1.5] text-[#c9d4f2] cursor-pointer hover:bg-[rgba(140,170,255,0.14)]"
                    >
                      {q.person != null ? (
                        <b
                          className="font-semibold"
                          style={{ color: colorOf(q.person) }}
                        >
                          {nameOf(q.person)}
                        </b>
                      ) : (
                        <span />
                      )}
                      <span>“{q.text}”</span>
                      {q.start_ms != null && (
                        <small className="col-start-2 md:col-start-auto text-[12px] text-[#7f8db5] tabular-nums">
                          {clock(q.start_ms)}
                        </small>
                      )}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        </React.Fragment>
      ))}

      {asking && (
        <>
          <div className={mine}>{asking}</div>
          <span className="text-[13px] text-[#8d9ac0]" role="status">
            {t("notes.ask.reading")}
          </span>
        </>
      )}

      {suggestions.length > 0 && !asking && (
        <div className="flex gap-2 flex-wrap">
          {suggestions.map((q) => (
            <button
              key={q}
              type="button"
              onClick={() => ask(q)}
              className="min-h-[34px] px-[14px] rounded-full border border-[rgba(140,170,255,0.22)] bg-[rgba(14,20,44,0.6)] text-[13.5px] font-medium text-[#c9d4f2] cursor-pointer hover:text-white hover:border-[rgba(143,194,255,0.45)]"
            >
              {q}
            </button>
          ))}
        </div>
      )}

      {error && (
        <p className="m-0 text-[13.5px] text-[#ffb4a8]" role="alert">
          {error}
        </p>
      )}

      <form
        onSubmit={(e) => {
          e.preventDefault();
          ask(typed);
        }}
        className="flex items-center gap-[10px] ps-[18px] pe-[6px] py-[6px] rounded-full bg-[rgba(14,20,44,0.9)] border border-[rgba(140,170,255,0.25)] focus-within:border-[rgba(143,194,255,0.6)]"
      >
        <input
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          placeholder={t("notes.ask.placeholder")}
          aria-label={t("notes.ask.placeholder")}
          disabled={!!asking}
          className="flex-1 min-w-0 bg-transparent border-0 text-[14.5px] text-text placeholder:text-[#7f8db5] focus:outline-none"
        />
        <button
          type="submit"
          disabled={!typed.trim() || !!asking}
          className="min-h-[38px] px-[18px] rounded-full bg-[#2f6bff] text-[14px] font-medium text-white cursor-pointer disabled:opacity-50 disabled:cursor-default"
        >
          {t("notes.ask.ask")}
        </button>
      </form>

      <div ref={end} className="flex items-center justify-between gap-3">
        <p className="m-0 text-[12.5px] leading-[1.55] text-[#7f8db5]">
          {t("notes.ask.private")}
        </p>
        {answers.length > 0 && !asking && (
          <button
            type="button"
            onClick={clear}
            className="text-[12.5px] text-[#8fb4ff] cursor-pointer hover:text-white"
          >
            {t("notes.ask.clear")}
          </button>
        )}
      </div>
    </section>
  );
};
