// Does every word on screen fit (#610)? Found by what's drawn, not by a list of classes, so a new screen is checked the
// day it lands: each line of text on screen, measured in the page in one go, and said in sentences when it's
//   - cut: a line clipped by a box that hides what runs past it (overflow hidden or clip), part or all of it, unless
//     the box says so itself: an ellipsis (…) at the cut (with a few letters before it), a line clamp, or a box you
//     can scroll to the rest;
//   - past the window: a line beyond the window's edge where nothing scrolls to it;
//   - out of its box: a line running past the edge of the box it's drawn in (one you can see: a background or a
//     border), with nothing clipping it in between;
//   - over other words: two lines of different text drawn on top of each other.
// Words under something on top (a sheet, a menu) aren't measured: they can't be seen, and the thing on top is.

/** What doesn't fit under `root`, one sentence a problem ([] when every word fits). `skip`: a selector of parts that
 *  draw their own text (a terminal's canvas) or aren't Cue's (a file's own lines). */
export function fitProblems(page, root = "body", skip = "") {
  return page.evaluate(([root, skip]) => {
    const out = [];
    const W = innerWidth, H = innerHeight, GIVE = 1;
    const style = new Map();
    const cs = (el) => { let s = style.get(el); if (!s) style.set(el, (s = getComputedStyle(el))); return s; };
    const hiddenMemo = new Map();
    const hidden = (el) => {
      if (!el || el === document.documentElement) return false;
      if (hiddenMemo.has(el)) return hiddenMemo.get(el);
      const s = cs(el);
      const h = s.display === "none" || s.visibility === "hidden" || +s.opacity === 0 || hidden(el.parentElement);
      hiddenMemo.set(el, h);
      return h;
    };
    const say = (el) => {
      const c = typeof el.className === "string" && el.className.trim() ? "." + el.className.trim().split(/\s+/).join(".") : el.tagName.toLowerCase();
      return `${c} “${el.textContent.trim().replace(/\s+/g, " ").slice(0, 40)}”`;
    };
    const seen = (el) => { const s = cs(el); return s.backgroundColor !== "rgba(0, 0, 0, 0)" && s.backgroundColor !== "transparent" || ["Top", "Right", "Bottom", "Left"].some((k) => parseFloat(s[`border${k}Width`]) > 0 && s[`border${k}Style`] !== "none" && !/rgba\([^)]*,\s*0\)$/.test(s[`border${k}Color`])); };
    // A box's inside: where its children are drawn and, if it clips, the part of them you see.
    const inner = (el) => { const r = el.getBoundingClientRect(); return { left: r.left + el.clientLeft, top: r.top + el.clientTop, right: r.left + el.clientLeft + el.clientWidth, bottom: r.top + el.clientTop + el.clientHeight }; };
    const own = (el) => { const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }; };
    const SKIP = "script, style, noscript, svg, canvas, textarea, input, select, option, .xterm" + (skip ? `, ${skip}` : "");

    const lines = [];
    const top = document.querySelector(root);
    if (!top) return [`nothing matches ${root}`];
    const walk = document.createTreeWalker(top, NodeFilter.SHOW_TEXT);
    for (let t = walk.nextNode(); t; t = walk.nextNode()) {
      if (!t.data.trim()) continue;
      const el = t.parentElement;
      if (!el || el.closest(SKIP) || hidden(el)) continue;
      // Each word's own boxes, joined into one per line: a node's range reaches over the spaces at its ends, which
      // may hang past a box with nothing to see.
      const words = [];
      for (const m of t.data.matchAll(/\S+/g)) {
        const r = document.createRange();
        r.setStart(t, m.index);
        r.setEnd(t, m.index + m[0].length);
        for (const x of r.getClientRects()) if (x.width > 0.5 && x.height > 0.5) words.push(x);
      }
      const byLine = new Map();
      for (const x of words) {
        const k = Math.round(x.top);
        const l = byLine.get(k);
        if (l) { l.left = Math.min(l.left, x.left); l.right = Math.max(l.right, x.right); l.bottom = Math.max(l.bottom, x.bottom); }
        else byLine.set(k, { left: x.left, right: x.right, top: x.top, bottom: x.bottom });
      }
      for (const r of byLine.values()) lines.push({ el, r, text: t.data.trim().replace(/\s+/g, " ").slice(0, 40) });
    }

    const problems = new Map();
    const add = (el, what) => { const k = `${say(el)} ${what}`; problems.set(k, k); };
    const covered = (l) => {
      const { r, el } = l;
      const x = Math.min(Math.max((r.left + r.right) / 2, 0), W - 1), y = Math.min(Math.max((r.top + r.bottom) / 2, 0), H - 1);
      const at = document.elementFromPoint(x, y);
      return at && !(at === el || el.contains(at) || at.contains(el));
    };

    const inSight = lines.filter((l) => !covered(l));
    for (const l of inSight) {
      const { el, r } = l;
      // Clipped: by each box above it that hides what runs past it, up to the window. `vis`: the part you see.
      let x = true, y = true, cut = null, fold = false, dots = false;
      const vis = { ...r };
      l.vis = vis;
      for (let a = el; a && a !== document.documentElement; a = a.parentElement) {
        const s = cs(a);
        const ox = s.overflowX, oy = s.overflowY;
        if (ox === "visible" && oy === "visible") continue;
        const c = inner(a);
        if (ox !== "visible") { vis.left = Math.max(vis.left, c.left); vis.right = Math.min(vis.right, c.right); }
        if (oy !== "visible") { vis.top = Math.max(vis.top, c.top); vis.bottom = Math.min(vis.bottom, c.bottom); }
        if (x && (r.left < c.left - GIVE || r.right > c.right + GIVE)) {
          if (/auto|scroll/.test(ox)) x = false;
          else if (s.textOverflow === "ellipsis") { x = false; dots = a; }
          else { cut ??= [a, "at its side"]; x = false; }
        } else if (x && /auto|scroll/.test(ox)) x = false;
        if (y && (r.top < c.top - GIVE || r.bottom > c.bottom + GIVE)) {
          if (/auto|scroll/.test(oy)) y = false;
          else if (s.webkitLineClamp && s.webkitLineClamp !== "none") { y = false; fold = true; }
          else { cut ??= [a, r.bottom > c.top && r.top < c.bottom ? "mid-line" : "out of sight"]; y = false; }
        } else if (y && /auto|scroll/.test(oy)) y = false;
      }
      if (cut) { add(el, `is cut ${cut[1]} by ${say(cut[0])}`); continue; }
      // An ellipsis says words were left out; one with hardly a word before it says nothing.
      if (dots && dots.clientWidth < 2.5 * parseFloat(cs(dots).fontSize)) { add(dots, "is cut to almost nothing"); continue; }
      if (fold) continue;
      if ((x && (r.left < -GIVE || r.right > W + GIVE)) || (y && (r.top < -GIVE || r.bottom > H + GIVE))) { add(el, "runs past the window"); continue; }
      // Out of the box it's drawn in: the nearest one you can see, unless something placed on its own lies between.
      for (let a = el; a && a !== top.parentElement; a = a.parentElement) {
        const s = cs(a);
        // A box that clips (or scrolls) was measured above.
        if (s.overflowX !== "visible" || s.overflowY !== "visible") break;
        if (seen(a)) {
          const b = own(a);
          if (r.left < b.left - GIVE || r.right > b.right + GIVE || r.top < b.top - GIVE || r.bottom > b.bottom + GIVE) add(el, `runs out of ${a === el ? "its own box" : say(a)}`);
          break;
        }
        if (s.position === "absolute" || s.position === "fixed") break;
      }
    }

    // Over other words: lines of different elements, both in sight, drawn on top of each other (more than the
    // half-line two close lines' boxes may share).
    const shown = inSight.filter((l) => l.vis && l.vis.right - l.vis.left > 1 && l.vis.bottom - l.vis.top > 1 && l.vis.bottom > 0 && l.vis.top < H && l.vis.right > 0 && l.vis.left < W);
    for (let i = 0; i < shown.length; i++) for (let j = i + 1; j < shown.length; j++) {
      const a = shown[i], b = shown[j];
      if (a.el === b.el) continue;
      const w = Math.min(a.vis.right, b.vis.right) - Math.max(a.vis.left, b.vis.left);
      const h = Math.min(a.vis.bottom, b.vis.bottom) - Math.max(a.vis.top, b.vis.top);
      if (w > 2 && h > 0.4 * Math.min(a.vis.bottom - a.vis.top, b.vis.bottom - b.vis.top)) add(a.el, `overlaps ${say(b.el)}`);
    }
    return [...problems.values()];
  }, [root, skip]);
}

/** Settings → Text size's steps (config.rs ZOOM_STEPS): the window's zoom. */
export const ZOOMS = [0.9, 1, 1.15, 1.3];
/** The window at zoom `z`: WebKit's page zoom lays the page out in a window that much smaller. */
export const zoomed = ({ width, height }, z) => ({ width: Math.round(width / z), height: Math.round(height / z) });
