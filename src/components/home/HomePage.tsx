import React, { useCallback, useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Check, Copy, Mic } from "lucide-react";
import { commands, events, type HistoryEntry } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { Keycaps } from "../shared/Keycaps";
import BolkeOrb from "../icons/BolkeOrb";
import { copyToClipboard } from "../settings/history/clipboard";
import { formatDateTime } from "@/utils/dateFormat";

const RECENT_COUNT = 4;

interface HomePageProps {
  onOpenSettings: () => void;
  onOpenHistory: () => void;
}

const greetingKey = (hour: number) =>
  hour < 12
    ? "home.greetingMorning"
    : hour < 18
      ? "home.greetingAfternoon"
      : "home.greetingEvening";

/** Home (design/wireframes/Main.dc.html): the orb, a place to try
 * dictation, the shortcut, and the latest few transcripts. */
export const HomePage: React.FC<HomePageProps> = ({
  onOpenSettings,
  onOpenHistory,
}) => {
  const { t, i18n } = useTranslation();
  const osType = useOsType();
  const { getSetting } = useSettings();
  const binding = getSetting("bindings")?.transcribe?.current_binding ?? "";
  const activation = getSetting("shortcut_activation") ?? "hold_or_toggle";

  const [trying, setTrying] = useState(false);
  const [tryText, setTryText] = useState("");
  const tryRef = useRef<HTMLTextAreaElement>(null);

  const [recent, setRecent] = useState<HistoryEntry[]>([]);
  const [copiedId, setCopiedId] = useState<number | null>(null);

  const loadRecent = useCallback(async () => {
    try {
      const result = await commands.getHistoryEntries(null, RECENT_COUNT);
      if (result.status === "ok") setRecent(result.data.entries);
    } catch (error) {
      console.error("Failed to load recent transcripts:", error);
    }
  }, []);

  useEffect(() => {
    loadRecent();
    const unlisten = events.historyUpdatePayload.listen(() => loadRecent());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadRecent]);

  useEffect(() => {
    if (trying) tryRef.current?.focus();
  }, [trying]);

  const copy = async (entry: HistoryEntry) => {
    const text = entry.post_processed_text ?? entry.transcription_text;
    if (await copyToClipboard(text)) {
      setCopiedId(entry.id);
      setTimeout(() => setCopiedId(null), 1500);
    }
  };

  const shortcutHelp =
    activation === "push_to_talk"
      ? t("home.shortcutPushToTalk")
      : activation === "toggle"
        ? t("home.shortcutToggle")
        : t("home.shortcutHoldOrToggle");

  return (
    <div className="w-full max-w-[1040px] mx-auto flex flex-col gap-6 px-4 pt-3 pb-6">
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <span className="inline-flex items-center min-h-8 px-[14px] rounded-full bg-[rgba(20,30,64,0.8)] border border-edge text-[13px] text-[#c3ceea]">
          {t(greetingKey(new Date().getHours()))}
        </span>
        {binding && (
          <span className="text-[13px] text-ink-2">
            <Trans
              i18nKey="home.holdToSpeak"
              components={{
                keys: <Keycaps binding={binding} osType={osType} size="line" />,
              }}
            />
          </span>
        )}
      </div>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(300px,1fr))] gap-6 items-start">
        <section
          aria-labelledby="home-title"
          className="flex flex-col items-center text-center gap-5 px-6 pt-8 pb-7 rounded-[28px] bg-[radial-gradient(closest-side_at_50%_62%,rgba(47,107,255,0.16),transparent)]"
        >
          <h1
            id="home-title"
            className="text-grad m-0 text-[48px] leading-[1.04] font-light tracking-[-0.03em]"
          >
            {t("home.titleLine1")}
            <br />
            {t("home.titleLine2")}
          </h1>
          <p className="m-0 max-w-[380px] text-[15px] leading-[1.55] text-ink-2">
            {t("home.subtitle")}
          </p>
          <BolkeOrb width={200} height={200} glow animated className="my-3" />
          {!trying ? (
            <button
              type="button"
              onClick={() => setTrying(true)}
              className="inline-flex items-center gap-[10px] min-h-11 px-[22px] rounded-full text-white text-[14px] font-medium cursor-pointer bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] shadow-[0_8px_30px_rgba(47,107,255,0.4)] hover:brightness-110 focus-visible:outline-2 focus-visible:outline-accent-soft"
            >
              <Mic size={18} aria-hidden="true" />
              {t("home.tryIt")}
            </button>
          ) : (
            <div className="w-full max-w-[440px] flex flex-col gap-2">
              {binding && (
                <p className="m-0 text-[13px] text-ink-2">
                  <Trans
                    i18nKey="home.tryHint"
                    components={{
                      keys: (
                        <Keycaps
                          binding={binding}
                          osType={osType}
                          size="line"
                        />
                      ),
                    }}
                  />
                </p>
              )}
              <textarea
                ref={tryRef}
                value={tryText}
                onChange={(e) => setTryText(e.target.value)}
                placeholder={t("home.tryPlaceholder")}
                rows={3}
                className="w-full resize-none rounded-[18px] border border-[rgba(140,170,255,0.2)] bg-[#0c1226] px-[18px] py-[14px] text-[15px] text-text placeholder:text-ink-3 select-text focus:outline-none focus:border-accent-soft"
              />
            </div>
          )}
        </section>

        <div className="flex flex-col gap-5 min-w-0">
          <section
            aria-labelledby="home-shortcut"
            className="flex flex-col gap-[14px] p-[22px] rounded-[24px] bg-linear-160 from-[#1d3fa8] to-[#10225e] border border-[rgba(140,170,255,0.25)]"
          >
            <h2
              id="home-shortcut"
              className="m-0 text-[14px] font-medium text-[#d6e1ff]"
            >
              {t("home.shortcutTitle")}
            </h2>
            {binding && (
              <Keycaps binding={binding} osType={osType} size="hero" />
            )}
            <p className="m-0 text-[13px] leading-[1.5] text-[#d6e1ff]">
              {shortcutHelp}
            </p>
            <button
              type="button"
              onClick={onOpenSettings}
              className="self-start min-h-11 px-[22px] rounded-full border border-edge bg-[rgba(5,10,30,0.4)] text-[14px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)]"
            >
              {t("home.changeShortcut")}
            </button>
          </section>

          <section
            aria-labelledby="home-recent"
            className="flex flex-col gap-[10px]"
          >
            <div className="flex justify-between items-center">
              <h2 id="home-recent" className="m-0 text-[14px] font-medium">
                {t("home.recentTitle")}
              </h2>
              <button
                type="button"
                onClick={onOpenHistory}
                className="min-h-9 px-4 rounded-full text-[13px] font-medium text-ink-2 cursor-pointer hover:text-text hover:bg-wash"
              >
                {t("home.seeAll")}
              </button>
            </div>
            {recent.length === 0 ? (
              <p className="m-0 px-4 py-3 rounded-[18px] bg-card border border-line text-[13px] text-ink-2">
                {t("home.recentEmpty")}
              </p>
            ) : (
              recent.map((entry) => (
                <div
                  key={entry.id}
                  className="flex items-center gap-3 py-3 pe-2 ps-4 rounded-[18px] bg-card border border-line"
                >
                  <div className="flex-1 min-w-0 flex flex-col gap-1">
                    <span className="text-[13px] leading-[1.45] text-[#dce4f8] truncate">
                      {entry.post_processed_text ?? entry.transcription_text}
                    </span>
                    <span className="text-[12px] text-ink-3">
                      {formatDateTime(String(entry.timestamp), i18n.language)}
                    </span>
                  </div>
                  <button
                    type="button"
                    aria-label={
                      copiedId === entry.id ? t("home.copied") : t("home.copy")
                    }
                    title={
                      copiedId === entry.id ? t("home.copied") : t("home.copy")
                    }
                    onClick={() => copy(entry)}
                    className="w-9 h-9 shrink-0 rounded-full inline-flex items-center justify-center bg-white/5 text-[#c3ceea] cursor-pointer hover:bg-white/15 hover:text-white"
                  >
                    {copiedId === entry.id ? (
                      <Check size={16} aria-hidden="true" />
                    ) : (
                      <Copy size={16} aria-hidden="true" />
                    )}
                  </button>
                </div>
              ))
            )}
          </section>
        </div>
      </div>
    </div>
  );
};
