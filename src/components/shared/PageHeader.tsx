import React from "react";
import { useTranslation } from "react-i18next";

/** A page's big gradient title and one-line subtitle (design/wireframes). */
export const PageHeader: React.FC<{
  titleKey: string;
  subtitleKey: string;
}> = ({ titleKey, subtitleKey }) => {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-[6px]">
      <h1 className="text-grad m-0 text-[36px] leading-[1.15] font-light tracking-[-0.02em]">
        {t(titleKey)}
      </h1>
      <p className="m-0 text-[14px] text-ink-2">{t(subtitleKey)}</p>
    </div>
  );
};
