// Small labels — a role tag (LEAD), a mode (AUTO), a state, a count, a stop's kind: every one is a full capsule
// (--pill-radius in styles/base.css). Found by what they are on screen, not by a list of their classes, so a new one
// can't come in with a shape of its own: a short line of text, not something you click or type in, with a background
// or a border of its own, or one named a pill, tag, chip or badge. Each is measured in the page in one go.

/** The labels on screen in `root` whose corners aren't a full capsule: "class “text” radius" for each, once a class. */
export function pillProblems(page, root = "#app") {
  return page.evaluate((root) => {
    // Not labels: what you click or type in (buttons keep their own corners), code and key caps, highlights, art.
    const NOT = "button, a, input, textarea, select, code, kbd, pre, mark, svg, canvas, iframe, .xterm, [role=button]";
    // Not labels either, though shaped like one: the agent's letter mark (C, π, X) is an icon, square by design; and
    // a key's name (Tab, esc) is drawn as its key cap.
    const ICONS = ".badge, .sug-k, .esc-hint";
    // ...unless it's named as one: a pill, a tag, a chip, a badge you can click is still one of these.
    const NAMED = /(^|-)(pill|tag|chip|badge)s?($|-)/;
    const seen = new Set(), out = [];
    const visible = (c) => c && c !== "transparent" && !/rgba\([^)]*,\s*0\)$/.test(c);
    for (const el of document.querySelector(root)?.querySelectorAll("*") || []) {
      const named = [...el.classList].some((c) => NAMED.test(c));
      if ((!named && el.closest(NOT)) || el.matches(ICONS)) continue;
      const cs = getComputedStyle(el);
      if (!(named || cs.display.startsWith("inline")) || cs.visibility === "hidden" || cs.display === "none") continue;
      const own = visible(cs.backgroundColor) || (parseFloat(cs.borderTopWidth) > 0 && cs.borderTopStyle !== "none" && visible(cs.borderTopColor));
      if (!own) continue;
      const r = el.getBoundingClientRect();
      const text = (el.textContent || "").trim();
      // One short line: a label, not a box of things.
      if (!text || text.length > 48 || r.height < 8 || r.height > 30 || r.width < 6) continue;
      if ([...el.children].some((c) => !getComputedStyle(c).display.startsWith("inline"))) continue;
      const radius = parseFloat(cs.borderTopLeftRadius);
      if (radius >= r.height / 2 - 0.5) continue;
      const cls = `${el.tagName.toLowerCase()}.${[...el.classList].join(".")}`;
      if (seen.has(cls)) continue;
      seen.add(cls);
      out.push(`${cls} “${text.slice(0, 24)}” radius ${cs.borderTopLeftRadius}`);
    }
    return out;
  }, root);
}
