import React from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { CircleCheck, Download } from "lucide-react";
import { commands } from "@/bindings";
import {
  downloadPercent,
  downloadSummaryModel,
  formatBytes,
  useSummaryModels,
} from "@/components/notes/noteUtils";

const smallBtn =
  "inline-flex items-center min-h-9 px-[14px] rounded-full border border-edge text-[13px] font-medium text-text cursor-pointer hover:border-[rgba(140,170,255,0.4)] disabled:opacity-50 disabled:cursor-default";

/** The free summary models, under the speech models on the Models page
 * (design/wireframes/SummaryModel.dc.html). */
export const SummaryModels: React.FC = () => {
  const { t } = useTranslation();
  const { models, progress } = useSummaryModels();
  if (models.length === 0) return null;
  const busy = models.some((m) => m.downloading || m.queued);

  const remove = async (id: string, name: string) => {
    const ok = await ask(t("notes.summaryModel.deleteConfirm", { name }), {
      kind: "warning",
    });
    if (ok) await commands.deleteSummaryModel(id);
  };

  return (
    <section
      aria-labelledby="summary-models"
      className="mt-6 flex flex-col gap-[6px] p-[14px] rounded-[20px] bg-[#0a0f20] border border-[rgba(140,170,255,0.12)]"
    >
      <div className="flex flex-col gap-1 px-[6px] pt-1 pb-[10px]">
        <h2 id="summary-models" className="m-0 text-[16px] font-medium">
          {t("notes.summaryModel.title")}
        </h2>
        <p className="m-0 text-[13px] leading-[1.5] text-ink-3">
          {t("notes.summaryModel.intro")}
        </p>
      </div>
      {models.map((model) => {
        const { percent } = downloadPercent(model, progress);
        return (
          <div
            key={model.id}
            className={`flex items-center gap-4 px-4 py-3 rounded-[14px] border ${
              model.active
                ? "bg-[rgba(47,107,255,0.12)] border-[rgba(140,170,255,0.25)]"
                : "border-transparent"
            }`}
          >
            {model.downloaded ? (
              <CircleCheck
                size={20}
                aria-hidden="true"
                className={model.active ? "text-[#8fb4ff]" : "text-ink-3"}
              />
            ) : (
              <Download size={20} aria-hidden="true" className="text-ink-3" />
            )}
            <div className="flex-1 min-w-0 flex flex-col gap-[3px]">
              <span className="flex items-center gap-2 text-[14px] font-medium flex-wrap">
                {model.name}
                <span className="inline-flex items-center min-h-[22px] px-[9px] rounded-full border border-[rgba(140,170,255,0.25)] text-[11px] font-medium text-[#c3ceea]">
                  {model.recommended
                    ? t("notes.summaryModel.recommended")
                    : t("notes.summaryModel.better")}
                </span>
                {model.active && (
                  <span className="text-[12px] font-medium text-[#8fb4ff]">
                    {t("notes.summaryModel.inUse")}
                  </span>
                )}
              </span>
              <span className="text-[13px] text-ink-3">
                {t(`notes.summaryModel.descriptions.${model.id}`)}
              </span>
              {model.downloading && (
                <div
                  role="progressbar"
                  aria-label={t("notes.ai.downloadLabel")}
                  aria-valuenow={percent}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  className="mt-1 h-[5px] max-w-[320px] rounded-[3px] bg-[#1a2244] overflow-hidden"
                >
                  <span
                    className="block h-full rounded-[3px] bg-linear-to-r from-[#3a78ff] to-[#8fc2ff] transition-[width]"
                    style={{ width: `${percent}%` }}
                  />
                </div>
              )}
            </div>
            <span className="text-[13px] text-ink-2 tabular-nums whitespace-nowrap">
              {model.queued
                ? t("notes.summaryModel.waiting")
                : model.downloading
                  ? `${percent}%`
                  : formatBytes(model.size_bytes)}
            </span>
            {model.downloading || model.queued ? (
              <button
                type="button"
                className={smallBtn}
                onClick={() => commands.cancelSummaryModelDownload(model.id)}
              >
                {t("notes.ai.cancel")}
              </button>
            ) : model.downloaded ? (
              <div className="flex gap-2">
                {!model.active && (
                  <button
                    type="button"
                    className={smallBtn}
                    onClick={() => commands.selectSummaryModel(model.id)}
                  >
                    {t("notes.summaryModel.use")}
                  </button>
                )}
                <button
                  type="button"
                  className={smallBtn}
                  onClick={() => remove(model.id, model.name)}
                >
                  {t("notes.summaryModel.delete")}
                </button>
              </div>
            ) : (
              <button
                type="button"
                className={smallBtn}
                disabled={busy}
                onClick={() => downloadSummaryModel(model.id, t)}
              >
                {t("notes.summaryModel.download")}
              </button>
            )}
          </div>
        );
      })}
    </section>
  );
};
