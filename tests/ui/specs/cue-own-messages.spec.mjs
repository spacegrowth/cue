// Cue's own messages to a session (a check line to redo, a nudge, any Cue sends it by itself) are marked Cue's by the
// hub as they go (#836, sessions/cue_said.rs: `cue` on the message, `cue_parts` on a kept one). A chat draws one with no
// card of its own as one quiet line of Cue's, what it says in short, a click opening its full text; never your bubble.
// The same words without the mark (you typed them) stay yours. Light and dark, laptop size and the smallest window.
import { test, expect } from "./cue.mjs";

const REDO = "Your DONE for #832 is in, but its check line needs another go: it names code (say the button, page or setting by the name the app shows). Write it again on a line of its own (it replaces the one before; nothing else to redo): CHECK: #832 | Where: <the screen> | Do: <what to click> | Expect: <what they see>, or CHECK: #832 | Nothing to try: <why>.";
const OTHER = "Desk item #d5 (Header): its branch is behind main by 3 commits. Rebase it before you go on.";
const QUIET = "Your background agent “docs” has gone quiet. Ask it where it is and to carry on, with SendMessage (to: \"a1\").";

/** eng's chat open on the Board, its thread as given. */
async function engChat(page, cue, thread, held = null) {
  await cue.change((st, [thread, held]) => {
    const s = st.sessions.find((x) => x.session_id === "eng");
    const t0 = Date.now() - 600000;
    s.thread = thread.map((e, n) => ({ ...e, at_ms: t0 + n * 60000 }));
    if (held) Object.assign(s, { state: "working", held: { role: "you", at_ms: Date.now() - 30000, images: [], ...held } });
  }, [thread, held]);
  await page.evaluate(() => setActive(null, "eng"));
  return page.locator(".active-pane");
}

for (const scheme of ["light", "dark"]) {
  for (const [size, viewport] of [["laptop", { width: 1440, height: 900 }], ["smallest", { width: 900, height: 600 }]]) {
    test(`Cue's own messages are one quiet line of Cue's, opened to their full text; your bubble holds only yours (${scheme}, ${size})`, async ({ cue, page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.setViewportSize(viewport);
      await cue.open("board-waiting");
      const chat = await engChat(page, cue, [
        { role: "you", text: "Refactor the order router" },
        { role: "agent", text: "Done.\n\nDONE: #832" },
        { role: "you", text: REDO, cue: true },
        { role: "agent", text: "Rewrote the check line." },
        { role: "you", text: OTHER, cue: true },
        { role: "you", text: "Can you make the header blue?" },
      ]);
      const lines = chat.locator(".cue-quiet");
      await expect(lines).toHaveCount(2);
      await expect(lines.locator(".dk-cue-t .mg-who")).toHaveText(["Cue", "Cue"]);
      await expect(lines.locator(".dk-cue-w")).toHaveText(["Asked it to redo its check line", "Its branch is behind main by 3 commits."]);
      await expect(lines.first().locator(".mg-id")).toContainText("#832");
      await expect(lines.nth(1).locator(".mg-id")).toContainText("#d5");
      // Your bubbles: only what you typed.
      await expect(chat.locator(".cv-you-text")).toHaveText(["Refactor the order router", "Can you make the header blue?"]);
      await expect(chat.locator(".cv-you", { hasText: "check line" })).toHaveCount(0);
      // Folded: a click opens Cue's full text under it, again closes it.
      await expect(lines.locator(".mg-full")).toHaveCount(0);
      await lines.first().locator(".dk-cue-w").click();
      await expect(chat.locator(".cue-quiet").first().locator(".mg-full")).toContainText("its check line needs another go");
      await expect(chat.locator(".cue-quiet").first().locator(".dk-cue-t")).toHaveAttribute("aria-expanded", "true");
      // No left stripe, no hover text; it fits the chat.
      const look = await page.evaluate(() => [...document.querySelectorAll(".active-pane .cue-quiet")].map((el) => ({
        border: getComputedStyle(el).borderLeftWidth, titled: !!el.querySelector("[title]") || el.hasAttribute("title"),
        fits: el.getBoundingClientRect().right <= el.closest(".active-pane").getBoundingClientRect().right + 0.5,
      })));
      for (const l of look) expect(l).toEqual({ border: "0px", titled: false, fits: true });
      if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/cue-own-messages-${scheme}-${size}.png` });
      await chat.locator(".cue-quiet").first().locator(".dk-cue-w").click();
      await expect(chat.locator(".cue-quiet .mg-full")).toHaveCount(0);
    });
  }
}

test("a reopen after a failed full run is one quiet line; the failures and the log only in its folded text", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const REOPEN = "Desk item #777 (Rail (dark)) is open again: the full run of main failed after it landed (hub::tests::header_stays, tests/rail.test.js). Fix it, then say DONE: #777.\nIts fixes stay in #777; don't add new items for them.\nThe end of its output:\nthread 'hub::tests::header_stays' panicked at src/hub.rs:12\nFAIL tests/rail.test.js";
  const NOTE = "Desk item #778 (Footer): the full run of main failed on its commit abc1234 (tests/rail.test.js). It is open with you already: fix that within #778 too.\nThe end of its output:\nFAIL tests/rail.test.js";
  const chat = await engChat(page, cue, [{ role: "you", text: REOPEN, cue: true }, { role: "you", text: NOTE, cue: true }]);
  const lines = chat.locator(".cue-quiet");
  await expect(lines.locator(".dk-cue-w")).toHaveText(["Reopened #777: the full test run failed", "Told it the full test run failed on #778's commit: fix it there too"]);
  await expect(lines).not.toContainText(["header_stays"]);
  await expect(chat.locator(".cv-you")).toHaveCount(0);
  await lines.first().locator(".dk-cue-w").click();
  const full = chat.locator(".cue-quiet").first().locator(".mg-full");
  await expect(full).toContainText("hub::tests::header_stays, tests/rail.test.js");
  await expect(full).toContainText("panicked at src/hub.rs:12");
});

test("the same words without Cue's mark are yours: a bubble, as you typed them", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const chat = await engChat(page, cue, [{ role: "you", text: OTHER }]);
  await expect(chat.locator(".cue-quiet")).toHaveCount(0);
  await expect(chat.locator(".cv-you-text")).toHaveText([OTHER]);
});

test("Cue's words kept with yours while it works: your kept bubble holds only yours, Cue's a quiet line going after it", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const chat = await engChat(page, cue, [{ role: "you", text: "Refactor the order router" }], { text: `fix the footer\n\n${QUIET}`, cue_parts: [QUIET] });
  const kept = chat.locator(".held .cv-you-text");
  await expect(kept).toHaveText(["fix the footer"]);
  const line = chat.locator(".cue-quiet");
  await expect(line).toHaveCount(1);
  await expect(line.locator(".dk-cue-w")).toHaveText("Asked it to wake its background agent docs");
  await expect(line.locator(".dk-cue-t")).toContainText("goes when its turn ends");
});

test("a kept message of Cue's alone is its quiet line, not a kept bubble of yours", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const chat = await engChat(page, cue, [{ role: "you", text: "Refactor the order router" }], { text: REDO, cue_parts: [REDO] });
  await expect(chat.locator(".held .cv-you-text")).toHaveCount(0);
  await expect(chat.locator(".cue-quiet .dk-cue-w")).toHaveText("Asked it to redo its check line");
});
