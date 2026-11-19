import React from "react";
import { useTranslation } from "react-i18next";
import type { NotePeople, PersonSummary } from "@/bindings";
import { colorOf } from "./noteUtils";

/** One person's summary: what they said, agreed to and will do
 * (design/wireframes/PersonSummary.dc.html). */
export const PersonCard: React.FC<{
  card: PersonSummary;
  name: string;
}> = ({ card, name }) => {
  const { t } = useTranslation();
  const list = (label: string, items: string[] | undefined) =>
    (items ?? []).length > 0 && (
      <div className="flex flex-col gap-1">
        <span className="text-[11px] font-semibold tracking-[0.06em] uppercase text-[#a9c2ff]">
          {label}
        </span>
        <ul className="m-0 pl-[18px] flex flex-col gap-1">
          {(items ?? []).map((item, i) => (
            <li
              key={i}
              className="text-[14px] leading-[1.5] text-[#e2e9fa] select-text"
            >
              {item}
            </li>
          ))}
        </ul>
      </div>
    );
  return (
    <div className="flex flex-col gap-3 p-4 rounded-[18px] bg-[rgba(10,18,48,0.45)] border border-[rgba(140,170,255,0.18)]">
      <div className="flex items-center gap-2">
        <span
          aria-hidden="true"
          className="w-[10px] h-[10px] rounded-full shrink-0"
          style={{ background: colorOf(card.person) }}
        />
        <span className="text-[14px] font-semibold text-[#eaf0ff]">{name}</span>
      </div>
      {list(t("notes.ai.said"), card.said)}
      {list(t("notes.ai.agreed"), card.agreed)}
      {list(t("notes.ai.willDo"), card.will_do)}
    </div>
  );
};

/** A meeting's summary for each person. Names come from the note's people,
 * so renaming someone renames their card. */
export const PersonCards: React.FC<{
  cards: PersonSummary[];
  people: NotePeople | null;
}> = ({ cards, people }) => {
  const { t } = useTranslation();
  const nameOf = (id: number) =>
    people?.people.find((p) => p.id === id)?.name ??
    t("notes.people.person", { number: id });

  return (
    <div className="col-span-full flex flex-col gap-3">
      <span className="text-[12px] font-semibold tracking-[0.06em] uppercase text-[#a9c2ff]">
        {t("notes.ai.byPerson")}
      </span>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
        {cards.map((card) => (
          <PersonCard
            key={card.person}
            card={card}
            name={nameOf(card.person)}
          />
        ))}
      </div>
    </div>
  );
};
