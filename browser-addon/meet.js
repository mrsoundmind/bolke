// Runs on meet.google.com. Watches who is in the call and whose tile lights
// up while they talk, and hands that to the Bolke app every couple of
// seconds. It reads names and the "talking" ring only: never video, chat or
// what's on screen.
//
// Meet changes its class names often, so this leans on what has held up:
// tiles carry [data-participant-id], the name sits in span.notranslate, the
// user's own name is in [data-self-name], and a talking tile gets a coloured
// ring (outline, border or box-shadow). Known "talking" classes are only a
// second hint. If captions happen to be on, each caption's speaker counts as
// talking too.

(() => {
  const POLL_MS = 250;
  const REPORT_MS = 2000;
  /** A pause shorter than this is the same stretch of talking. */
  const GAP_MS = 700;
  const TALKING_HINTS = [
    ".Oaajhc",
    ".HX2H7",
    ".wEsLMd",
    ".OgVli",
    ".IisKdb",
    '[data-is-speaking="true"]',
    '[data-audio-level]:not([data-audio-level="0"])',
  ].join(",");
  const CAPTION_ROWS = 'div[jsname="dsyhDe"], div.CNusmb, div.TBMuR';
  const CAPTION_WHO = 'div.KcIKyf, div.zs7s8d, span[jsname="YSxPC"]';

  /** name -> { start, last } for someone talking now. */
  const open = new Map();
  /** Finished stretches not yet sent: { name, start, end }. */
  let done = [];
  const people = new Set();
  let selfName = null;

  const clean = (text) =>
    (text || "")
      .replace(/\s+/g, " ")
      .replace(/\((you|You)\)$/, "")
      .trim();

  const usable = (name) =>
    !!name &&
    name.length <= 80 &&
    !/^Google Participant \(|spaces\//.test(name) &&
    !/^(you|You)$/.test(name);

  function readSelf() {
    const el = document.querySelector("[data-self-name]");
    const name = clean(el && el.getAttribute("data-self-name"));
    if (usable(name)) selfName = name;
  }

  /** The person shown on a tile. */
  function nameOf(tile) {
    const spans = [...tile.querySelectorAll("span.notranslate")].filter(
      (s) => !s.closest("button, [role=menu], [role=menuitem], i"),
    );
    const texts = [...new Set(spans.map((s) => clean(s.textContent)))].filter(
      Boolean,
    );
    let name = texts.length === 1 ? texts[0] : null;
    if (!name) {
      const el = tile.querySelector("[data-participant-name], [data-name]");
      name = clean(
        el &&
          (el.getAttribute("data-participant-name") ||
            el.getAttribute("data-name")),
      );
    }
    if (!name) name = clean(tile.getAttribute("aria-label"));
    // The user's own tile says "You".
    if (/^(you|You)$/.test(name || "") && selfName) return selfName;
    return usable(name) ? name : null;
  }

  /** A visible coloured ring at least 2px wide. */
  function hasRing(el) {
    const st = getComputedStyle(el);
    const coloured = (c) => {
      const m = (c || "").match(/rgba?\(([^)]+)\)/);
      if (!m) return false;
      const [r, g, b, a = 1] = m[1].split(",").map((x) => parseFloat(x));
      if (a < 0.3) return false;
      return Math.max(r, g, b) - Math.min(r, g, b) > 40;
    };
    if (
      parseFloat(st.outlineWidth) >= 2 &&
      st.outlineStyle !== "none" &&
      coloured(st.outlineColor)
    )
      return true;
    if (
      parseFloat(st.borderTopWidth) >= 2 &&
      st.borderTopStyle !== "none" &&
      coloured(st.borderTopColor)
    )
      return true;
    const shadow = st.boxShadow;
    if (shadow && shadow !== "none") {
      // "rgb(...) 0px 0px 0px 3px" — the 4th length is the spread.
      const m = shadow.match(
        /(rgba?\([^)]+\))\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+([\d.]+)px\s+([\d.]+)px/,
      );
      if (m && parseFloat(m[5]) >= 2 && coloured(m[1])) return true;
    }
    return false;
  }

  function talking(tile) {
    if (tile.matches(TALKING_HINTS) || tile.querySelector(TALKING_HINTS))
      return true;
    if (hasRing(tile)) return true;
    // The ring is usually on a frame a few levels in.
    const frames = tile.querySelectorAll(":scope > div, :scope > div > div");
    for (const f of frames) if (hasRing(f)) return true;
    return false;
  }

  function heard(name, now) {
    const s = open.get(name);
    if (s && now - s.last <= GAP_MS) {
      s.last = now;
    } else {
      if (s) done.push({ name, start: s.start, end: s.last });
      open.set(name, { start: now, last: now });
    }
  }

  function closeQuiet(now) {
    for (const [name, s] of open) {
      if (now - s.last > GAP_MS) {
        done.push({ name, start: s.start, end: s.last });
        open.delete(name);
      }
    }
  }

  // The newest caption's speaker is talking now.
  let lastCaption = "";
  function readCaptions(now) {
    const rows = document.querySelectorAll(CAPTION_ROWS);
    const row = rows[rows.length - 1];
    if (!row) return;
    const who = clean(row.querySelector(CAPTION_WHO)?.textContent);
    const text = row.textContent || "";
    if (!usable(who) || text === lastCaption) return;
    lastCaption = text;
    people.add(who);
    heard(who, now);
  }

  function poll() {
    const now = Date.now();
    readSelf();
    if (selfName) people.add(selfName);
    for (const tile of document.querySelectorAll("[data-participant-id]")) {
      // Nested ids belong to the same tile.
      if (tile.parentElement?.closest("[data-participant-id]")) continue;
      const name = nameOf(tile);
      if (!name) continue;
      people.add(name);
      if (talking(tile)) heard(name, now);
    }
    // The People panel, when open, lists everyone, not only those on screen.
    for (const item of document.querySelectorAll(
      '[role="list"][aria-label="Participants"] [role="listitem"]',
    )) {
      const name = clean(item.querySelector("span.notranslate")?.textContent);
      if (usable(name)) people.add(name);
    }
    readCaptions(now);
    closeQuiet(now);
  }

  function report(final) {
    if (!people.size) return;
    if (final) {
      for (const [name, s] of open)
        done.push({ name, start: s.start, end: s.last });
      open.clear();
    }
    const speaking = done;
    done = [];
    try {
      chrome.runtime.sendMessage({
        type: "report",
        report: {
          platform: "meet",
          self: selfName,
          people: [...people],
          speaking,
        },
        // For the add-on's own window only.
        talkingNow: [...open.keys()],
      });
    } catch {
      // The add-on was reloaded; this page's copy is stale.
    }
  }

  setInterval(poll, POLL_MS);
  setInterval(() => report(false), REPORT_MS);
  window.addEventListener("pagehide", () => report(true));
})();
