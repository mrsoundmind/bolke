import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: string;
  children: React.ReactNode;
}

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  children,
}) => {
  return (
    <section className="rounded-[24px] bg-card border border-edge overflow-visible px-2 py-3">
      {title && (
        <div className="px-4 pt-1 pb-2">
          <h2 className="text-[16px] font-medium text-text">{title}</h2>
          {description && (
            <p className="text-[12px] text-ink-3 mt-1">{description}</p>
          )}
        </div>
      )}
      <div className="divide-y divide-line">{children}</div>
    </section>
  );
};
