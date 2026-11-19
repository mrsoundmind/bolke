import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Bell, Check, Mic, Plus, Trash2, Users, X } from "lucide-react";
import {
  commands,
  type MeetingTodo,
  type Todo,
  type TodoList,
} from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { notify } from "@/stores/noticeStore";
import { Keycaps } from "../shared/Keycaps";
import { daysFromToday, formatDay, formatDue } from "@/lib/todos";
import { useNotesChanged } from "../notes/noteUtils";

/** One row: a to-do of your own, or an action point from a meeting. */
type Item =
  | { kind: "own"; key: string; todo: Todo; done: boolean }
  | { kind: "meeting"; key: string; point: MeetingTodo; done: boolean };

type Filter = "all" | "today" | "meetings" | "done";
type Group = "late" | "today" | "later" | "anytime";

const seg =
  "inline-flex items-center gap-2 min-h-10 px-4 rounded-full border text-[13px] font-medium cursor-pointer focus-visible:outline-2 focus-visible:outline-accent-soft";
const chip =
  "inline-flex items-center gap-[6px] min-h-6 px-[10px] rounded-full border text-[12px]";
const pillBtn =
  "inline-flex items-center gap-[10px] min-h-12 px-6 rounded-full border border-edge bg-[rgba(20,30,64,0.6)] text-[14px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)] focus-visible:outline-2 focus-visible:outline-accent-soft";

const nowSecs = () => Math.floor(Date.now() / 1000);

/** A Unix time as the value of a `datetime-local` input. */
const toLocalInput = (secs: number) => {
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

const groupOf = (todo: Todo): Group => {
  if (todo.due_at == null) return "anytime";
  if (todo.due_at < nowSecs()) return "late";
  return daysFromToday(new Date(todo.due_at * 1000)) <= 0 ? "today" : "later";
};

/** To-do (design/wireframes/TodoTab, ReminderFlow): reminders said with
 * either shortcut, to-dos typed here, and meetings' action points. */
export const TodoPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const osType = useOsType();
  const { getSetting } = useSettings();
  const ideaBinding = getSetting("bindings")?.record_idea?.current_binding;
  const [list, setList] = useState<TodoList>({ todos: [], from_meetings: [] });
  const [loaded, setLoaded] = useState(false);
  const [filter, setFilter] = useState<Filter>("all");
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const [timing, setTiming] = useState<number | null>(null);
  const [timeDraft, setTimeDraft] = useState("");
  const [editing, setEditing] = useState<number | null>(null);
  const [textDraft, setTextDraft] = useState("");
  // Re-render each minute so "Overdue" stays true.
  const [, setTick] = useState(0);

  const load = useCallback(async () => {
    const result = await commands.listTodos();
    if (result.status === "ok") setList(result.data);
    setLoaded(true);
  }, []);

  useEffect(() => {
    load();
    const unlisten = listen("todos-changed", load);
    const timer = setInterval(() => setTick((n) => n + 1), 60_000);
    return () => {
      unlisten.then((f) => f());
      clearInterval(timer);
    };
  }, [load]);
  useNotesChanged(load);

  const items: Item[] = useMemo(
    () => [
      ...list.todos.map(
        (todo): Item => ({
          kind: "own",
          key: `t${todo.id}`,
          todo,
          done: todo.done_at != null,
        }),
      ),
      ...list.from_meetings.map(
        (point): Item => ({
          kind: "meeting",
          key: `m${point.note_id}-${point.index}`,
          point,
          done: point.done,
        }),
      ),
    ],
    [list],
  );

  const open = items.filter((i) => !i.done);
  const finished = items.filter((i) => i.done);
  const group = (i: Item): Group =>
    i.kind === "own" ? groupOf(i.todo) : "anytime";
  const isToday = (i: Item) => ["late", "today"].includes(group(i));
  const shown = open.filter((i) =>
    filter === "today"
      ? isToday(i)
      : filter === "meetings"
        ? i.kind === "meeting"
        : filter === "all",
  );

  const tabs: [Filter, string, number][] = [
    ["all", t("todo.filter.all"), open.length],
    ["today", t("todo.filter.today"), open.filter(isToday).length],
    [
      "meetings",
      t("todo.filter.meetings"),
      open.filter((i) => i.kind === "meeting").length,
    ],
    ["done", t("todo.filter.done"), finished.length],
  ];

  const add = async () => {
    const text = draft.trim();
    if (!text) {
      setAdding(false);
      return;
    }
    const result = await commands.addTodo(text);
    if (result.status === "ok") {
      setDraft("");
      load();
    } else notify({ title: t("todo.addFailed"), detail: result.error });
  };

  const toggle = async (item: Item) => {
    if (item.kind === "own")
      await commands.setTodoDone(item.todo.id, !item.done);
    else
      await commands.setActionPointDone(
        item.point.note_id,
        item.point.index,
        !item.done,
      );
    load();
  };

  const startTiming = (todo: Todo) => {
    setTiming(todo.id);
    // An hour from now, on the hour, when it has no time yet.
    const suggested =
      todo.due_at ?? Math.ceil((nowSecs() + 3600) / 3600) * 3600;
    setTimeDraft(toLocalInput(suggested));
  };

  const saveTime = async (id: number, clear = false) => {
    const due = clear
      ? null
      : Math.floor(new Date(timeDraft).getTime() / 1000) || null;
    await commands.setTodoDue(id, due);
    setTiming(null);
    load();
  };

  const saveText = async (id: number) => {
    if (textDraft.trim()) await commands.setTodoText(id, textDraft);
    setEditing(null);
    load();
  };

  const row = (item: Item) => {
    const own = item.kind === "own" ? item.todo : null;
    const text = own
      ? own.text
      : item.kind === "meeting"
        ? item.point.text
        : "";
    const late = own != null && !item.done && groupOf(own) === "late";
    return (
      <div
        key={item.key}
        className="flex items-start gap-[14px] px-[18px] py-[14px] rounded-[18px] bg-[rgba(14,20,44,0.8)] border border-[rgba(140,170,255,0.1)] group"
      >
        <button
          type="button"
          onClick={() => toggle(item)}
          aria-pressed={item.done}
          aria-label={t("todo.markDone", { text })}
          className={`mt-px w-6 h-6 shrink-0 rounded-full border-2 inline-flex items-center justify-center cursor-pointer focus-visible:outline-2 focus-visible:outline-accent-soft ${
            item.done
              ? "bg-[#3a78ff] border-[#3a78ff] text-white"
              : "border-[rgba(140,170,255,0.45)] text-transparent hover:border-[#8fb4ff]"
          }`}
        >
          <Check size={13} strokeWidth={3} aria-hidden="true" />
        </button>
        <span className="flex-1 min-w-0 flex flex-col gap-2">
          {own && editing === own.id ? (
            <input
              autoFocus
              value={textDraft}
              onChange={(e) => setTextDraft(e.target.value)}
              onBlur={() => saveText(own.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") saveText(own.id);
                if (e.key === "Escape") setEditing(null);
              }}
              aria-label={t("todo.editText")}
              className="min-h-9 px-3 rounded-[10px] border border-accent-soft bg-[#0c1226] text-[15px] text-text select-text focus:outline-none"
            />
          ) : (
            <span
              onDoubleClick={() => {
                if (!own) return;
                setEditing(own.id);
                setTextDraft(own.text);
              }}
              className={`text-[15px] leading-[1.4] select-text ${
                item.done ? "text-[#7d89ac] line-through" : "text-text"
              }`}
            >
              {text}
            </span>
          )}
          <span className="flex gap-2 flex-wrap items-center">
            {own && timing === own.id ? (
              <span className="inline-flex gap-2 items-center flex-wrap">
                <input
                  type="datetime-local"
                  autoFocus
                  value={timeDraft}
                  onChange={(e) => setTimeDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") saveTime(own.id);
                    if (e.key === "Escape") setTiming(null);
                  }}
                  aria-label={t("todo.when")}
                  className="min-h-8 px-3 rounded-full border border-[rgba(242,179,107,0.5)] bg-[#0c1226] text-[12px] text-text [color-scheme:dark]"
                />
                <button
                  type="button"
                  onClick={() => saveTime(own.id)}
                  className="min-h-8 px-3 rounded-full bg-[#f2b36b] text-[#2a1a05] text-[12px] font-semibold cursor-pointer"
                >
                  {t("todo.setTime")}
                </button>
                {own.due_at != null && (
                  <button
                    type="button"
                    onClick={() => saveTime(own.id, true)}
                    className="min-h-8 px-3 rounded-full border border-edge text-[12px] text-ink-2 cursor-pointer hover:text-text"
                  >
                    {t("todo.noReminder")}
                  </button>
                )}
                <button
                  type="button"
                  aria-label={t("todo.cancel")}
                  onClick={() => setTiming(null)}
                  className="w-8 h-8 rounded-full inline-flex items-center justify-center text-ink-2 cursor-pointer hover:bg-wash hover:text-text"
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </span>
            ) : own && own.due_at != null ? (
              <button
                type="button"
                onClick={() => startTiming(own)}
                title={t("todo.changeTime")}
                className={`${chip} cursor-pointer ${
                  late
                    ? "bg-[rgba(255,107,97,0.12)] border-[rgba(255,120,110,0.35)] text-[#ffb3ac]"
                    : "bg-[rgba(242,179,107,0.12)] border-[rgba(242,179,107,0.3)] text-[#f6cb98]"
                }`}
              >
                <Bell size={12} aria-hidden="true" />
                {formatDue(own.due_at, i18n.language, t)}
              </button>
            ) : (
              own &&
              !item.done && (
                <button
                  type="button"
                  onClick={() => startTiming(own)}
                  className={`${chip} cursor-pointer border-dashed border-[rgba(242,179,107,0.35)] text-[#d9b48a] opacity-70 group-hover:opacity-100 focus-visible:opacity-100`}
                >
                  <Bell size={12} aria-hidden="true" />
                  {t("todo.remindMe")}
                </button>
              )
            )}
            {own?.spoken && (
              <span
                title={own.spoken}
                className={`${chip} bg-[rgba(20,30,64,0.9)] border-[rgba(167,139,255,0.3)] text-[#d0c4ff]`}
              >
                <Mic size={12} aria-hidden="true" />
                {t("todo.youSaidIt", {
                  when: formatDay(own.created_at, i18n.language, t),
                })}
              </span>
            )}
            {item.kind === "meeting" && (
              <span
                className={`${chip} bg-[rgba(20,30,64,0.9)] border-[rgba(140,170,255,0.16)] text-[#afc2ee]`}
              >
                <Users size={12} aria-hidden="true" />
                {t("todo.fromMeeting", { meeting: item.point.meeting })}
              </span>
            )}
          </span>
        </span>
        {own && (
          <button
            type="button"
            aria-label={t("todo.delete")}
            title={t("todo.delete")}
            onClick={async () => {
              await commands.deleteTodo(own.id);
              load();
            }}
            className="w-8 h-8 shrink-0 rounded-full inline-flex items-center justify-center text-ink-3 cursor-pointer opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-wash hover:text-text"
          >
            <Trash2 size={15} aria-hidden="true" />
          </button>
        )}
      </div>
    );
  };

  const section = (label: string, rows: Item[]) =>
    rows.length > 0 && (
      <React.Fragment key={label}>
        <h2 className="m-0 mt-[6px] mx-1 text-[12px] font-semibold tracking-[0.06em] uppercase text-ink-3">
          {label}
        </h2>
        {rows.map(row)}
      </React.Fragment>
    );

  const inGroup = (g: Group) => shown.filter((i) => group(i) === g);

  return (
    <div className="w-full max-w-[1040px] mx-auto flex flex-col gap-[18px] px-4 pt-1 pb-6">
      <div className="flex justify-between items-end gap-4 flex-wrap">
        <div className="flex flex-col gap-[6px]">
          <h1 className="text-grad m-0 text-[36px] leading-[1.15] font-light tracking-[-0.02em]">
            {t("todo.title")}
          </h1>
          <p className="m-0 text-[14px] text-ink-2">{t("todo.subtitle")}</p>
        </div>
        <button
          type="button"
          className={pillBtn}
          onClick={() => setAdding(true)}
        >
          <Plus size={16} aria-hidden="true" />
          {t("todo.add")}
        </button>
      </div>

      <div className="flex items-center gap-[18px] px-[22px] py-[18px] rounded-[22px] bg-linear-160 from-[#2e2266] to-[#15123a] border border-[rgba(167,139,255,0.35)] text-[#f3efff]">
        <span
          aria-hidden="true"
          className="shrink-0 w-12 h-12 rounded-full inline-flex items-center justify-center bg-[rgba(167,139,255,0.2)] text-[#c4b5ff] shadow-[0_0_30px_rgba(140,110,255,0.3)]"
        >
          <Mic size={22} strokeWidth={1.8} />
        </span>
        <span className="flex flex-col gap-[5px] min-w-0">
          <span className="text-[16px] font-medium">
            {ideaBinding ? (
              <Trans
                i18nKey="todo.sayIt"
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
              t("todo.sayItNoKeys")
            )}
          </span>
          <span className="text-[13px] leading-[1.45] text-[#d8d0ff]">
            {t("todo.sayItExamples")}
          </span>
        </span>
      </div>

      {adding && (
        <form
          className="flex gap-2 items-center"
          onSubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <input
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setDraft("");
                setAdding(false);
              }
            }}
            placeholder={t("todo.addPlaceholder")}
            aria-label={t("todo.add")}
            maxLength={300}
            className="flex-1 min-h-12 px-5 rounded-full border border-accent-soft bg-[#0c1226] text-[14px] text-text placeholder:text-ink-3 select-text focus:outline-none"
          />
          <button
            type="submit"
            className="min-h-12 px-6 rounded-full bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] text-white text-[14px] font-medium cursor-pointer"
          >
            {t("todo.addButton")}
          </button>
        </form>
      )}

      <div className="flex gap-2 flex-wrap">
        {tabs.map(([id, label, count]) => (
          <button
            key={id}
            type="button"
            aria-pressed={filter === id}
            onClick={() => setFilter(id)}
            className={`${seg} ${
              filter === id
                ? "bg-[#2f6bff] border-[#2f6bff] text-white"
                : "border-[rgba(140,170,255,0.14)] text-[#aeb9d6] hover:border-[rgba(140,170,255,0.35)] hover:text-text"
            }`}
          >
            {label}
            <span className="text-[12px] opacity-75">{count}</span>
          </button>
        ))}
      </div>

      <div className="flex flex-col gap-[10px]">
        {loaded &&
          (filter === "done" ? finished.length === 0 : shown.length === 0) && (
            <p className="m-0 px-5 py-4 rounded-[20px] bg-card border border-line text-[13px] text-ink-2">
              {filter === "done" ? t("todo.emptyDone") : t("todo.empty")}
            </p>
          )}
        {filter === "done" ? (
          finished.map(row)
        ) : (
          <>
            {section(t("todo.group.late"), inGroup("late"))}
            {section(t("todo.group.today"), inGroup("today"))}
            {section(t("todo.group.later"), inGroup("later"))}
            {section(t("todo.group.anytime"), inGroup("anytime"))}
            {filter === "all" &&
              section(t("todo.group.done"), finished.slice(0, 8))}
          </>
        )}
      </div>
    </div>
  );
};
