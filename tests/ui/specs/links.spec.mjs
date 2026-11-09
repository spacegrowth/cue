// Links take the theme's colour (base.css `a`): in a chat, a link in your message and in the agent's reply is the
// accent of the surface it sits on, never the browser's blue or visited purple, and readable there (WCAG AA,
// 4.5:1), in a light and a dark theme. A path is the same colour, plain until the pointer is on it.
import { test, expect } from "./cue.mjs";

const SAID = "Is it like https://example.com/said-link this?";
const REPLY = "See [the guide](https://example.com/guide) and the plan in docs/rollout-plan.md.";

/** Each link in the open chat: its colour, the accent where it is, and its contrast with the first opaque
 *  background under it. Colours are read back through a canvas, so rgb(), color(srgb …) and oklch all compare. */
const linksIn = (page) => page.evaluate(() => {
  const cv = document.createElement("canvas"); cv.width = cv.height = 1;
  const cx = cv.getContext("2d", { willReadFrequently: true });
  const rgba = (c) => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = "#000"; cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); return [...cx.getImageData(0, 0, 1, 1).data]; };
  const lum = ([r, g, b]) => [r, g, b].map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }).reduce((s, v, i) => s + v * [0.2126, 0.7152, 0.0722][i], 0);
  const bgOf = (el) => { for (let e = el; e; e = e.parentElement) { const c = rgba(getComputedStyle(e).backgroundColor); if (c[3] === 255) return c; } return rgba(getComputedStyle(document.body).backgroundColor); };
  return [...document.querySelectorAll(".active-pane a[data-href], .active-pane a[data-path]")].map((a) => {
    const fg = rgba(getComputedStyle(a).color), bg = bgOf(a);
    const accent = rgba(getComputedStyle(a).getPropertyValue("--accent").trim());
    const [hi, lo] = [lum(fg), lum(bg)].sort((x, y) => y - x);
    return { text: a.textContent, fg: fg.slice(0, 3).join(","), accent: accent.slice(0, 3).join(","), contrast: (hi + 0.05) / (lo + 0.05), underline: getComputedStyle(a).textDecorationLine };
  });
});

for (const mode of ["light", "dark"]) {
  test(`links in a chat take the ${mode} theme's colour`, async ({ cue, page }) => {
    await cue.open("board-waiting");
    await cue.change((s, [mode, said, reply]) => {
      s.settings = { ...s.settings, appearance: { ...s.settings.appearance, mode } };
      const t = s.sessions.find((x) => x.session_id === "relay").thread;
      t.push({ role: "you", text: said, at_ms: t.at(-1).at_ms + 1000 }, { role: "agent", text: reply, at_ms: t.at(-1).at_ms + 2000 });
    }, [mode, SAID, REPLY]);
    await expect(page.locator("html")).toHaveAttribute("data-theme", mode);
    await page.locator('.nrow[data-big="n1"]').click();
    await page.locator('.active-pane a[data-href="https://example.com/guide"]').waitFor();
    await page.mouse.move(0, 0);
    const links = await linksIn(page);
    for (const want of ["https://example.com/said-link", "the guide", "docs/rollout-plan.md"]) expect(links.map((l) => l.text)).toContain(want);
    for (const l of links) {
      expect(l.fg, `${l.text}: the accent, not the browser's colour`).toBe(l.accent);
      expect(["0,0,238", "85,26,139"], `${l.text}: not link blue or visited purple`).not.toContain(l.fg);
      expect(l.contrast, `${l.text}: readable on what it sits on`).toBeGreaterThanOrEqual(4.5);
    }
    expect(links.find((l) => l.text === "the guide").underline).toBe("underline");
    expect(links.find((l) => l.text === "docs/rollout-plan.md").underline).toBe("none");
  });
}
