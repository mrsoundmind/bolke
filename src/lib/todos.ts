import type { TFunction } from "i18next";

/** What the reminder bubble shows (`reminders::ReminderBubble` in Rust). */
export interface ReminderBubble {
  id: number;
  text: string;
  due_at: number | null;
  can_type: boolean;
  /** What was just done to it by voice, with Undo; null for a new one. */
  changed: "deleted" | "done" | "moved" | "not_found" | null;
}

const startOfDay = (d: Date) =>
  new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** Days from today to `d`: 0 today, 1 tomorrow, -1 yesterday. */
export const daysFromToday = (d: Date, now = new Date()) =>
  Math.round((startOfDay(d) - startOfDay(now)) / 86_400_000);

/** "Today", "Tomorrow", "Friday" or "21 Nov". */
export const formatDay = (secs: number, language: string, t: TFunction) => {
  const d = new Date(secs * 1000);
  const days = daysFromToday(d);
  if (days === 0) return t("todo.today");
  if (days === 1) return t("todo.tomorrow");
  if (days === -1) return t("todo.yesterday");
  if (days > 1 && days < 7)
    return d.toLocaleDateString(language, { weekday: "long" });
  return d.toLocaleDateString(language, { day: "numeric", month: "short" });
};

/** "Tomorrow, 5:00 pm". */
export const formatDue = (secs: number, language: string, t: TFunction) => {
  const time = new Date(secs * 1000).toLocaleTimeString(language, {
    hour: "numeric",
    minute: "2-digit",
  });
  return `${formatDay(secs, language, t)}, ${time}`;
};
