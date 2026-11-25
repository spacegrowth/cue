// What Cue told the Desk's lead with your message shows folded under your words (#822): a small grey "+ Desk update",
// as the voice note's +, never the block's raw lines. Opened, it lists the update by section (To route, Moved, Done or
// gone, Free sessions); a whole block says "whole Desk", and its rules are one more fold. A changes-only block, a whole
// one with any settings in its opening, and messages joined while the lead was busy all leave your words alone.
// Laptop size and the smallest window, light and dark, no hover text.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

// As desk/hub/lead/run.rs writes them: a block of changes only, and a whole one with the rules after a hand-off.
const CHANGES = `<desk id="cue" name="cue" project="cue" changes="only">
Only what changed since your last Desk block; everything it doesn't name stands as it was. SHOW: desk sends it whole, SHOW: #<id> an item in full.
Needs routing:
- #d21 feature: Mocks: how the Desk update shows | new | no you'll see yet
Other open items:
- #d3 bug: Search stays centred in the header | finishing | held by host-lead (session)
- #d4 task: Rail dot blinks for a reply you didn't send | working | held by cue-12 (session) | after #d3 | done when: tests pass
Desk routes (or use ROUTE: #<id> -> new):
- cue-6 (sid:abcdef12, idle)
- cue-5 (sid:1234abcd, on Studio, idle, 40k, cold: no new work)
No longer listed: #d7 (done, dropped or moved); session cue-3
</desk>

`;
const WHOLE = `<desk id="cue" name="cue" project="cue" lane="2">
Desk lead: web-lead (you)

Needs routing:
- #d10 feature: Away switch remembers its time | new | done when: a test | you'll see: it remembers

Other open items:
- #d3 bug: Search stays centred in the header | working | held by host-lead (session)

Parked (the owner starts these themselves; don't ROUTE or START them):
- #d11 task: Push the history clean-up | parked

Desk routes (or use ROUTE: #<id> -> new):
- cue-9 (sid:99999999, working)
- cue-6 (sid:abcdef12, idle)
For independent work, prefer a suitable idle Desk session over a busy one.
Your Desk block gives items being routed, at work, finishing or paused in full.

Answer the owner's question or request directly in plain language before any Desk command lines.
</desk>

`;
const ASKED = "What's left before the push?";
const HOLD = "Hold #d4 until #d3 lands; the header counts first.";
const FIRST = "First thing";
const SECOND = "Second thing";
const RAW = ["<desk", "</desk>", "sid:", "Needs routing", "changes=", "No longer listed", "Only what changed"];

async function setUp(page, cue, scheme, size) {
  await page.emulateMedia({ colorScheme: scheme });
  await page.setViewportSize(size);
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st, { msgs }) => {
    st.sessions.find((x) => x.session_id === "lead").thread.push(...msgs);
  }, { msgs: [
    { role: "you", text: WHOLE + HOLD, at_ms: NOW - 50000 },
    { role: "you", text: CHANGES + ASKED, at_ms: NOW - 40000 },
    // Two messages sent while the lead was busy reach it as one, joined by a newline, each with its block.
    { role: "you", text: `${CHANGES}${FIRST}\n${CHANGES}${SECOND}`, at_ms: NOW - 30000 },
    { role: "agent", text: "Noted.", at_ms: NOW - 20000 },
  ] });
}

const bubble = (page, words) => page.locator(".dk-chat .cv-you-text").filter({ hasText: words });

for (const scheme of ["light", "dark"]) {
  for (const [name, size] of [["laptop", { width: 1440, height: 900 }], ["smallest", { width: 900, height: 600 }]]) {
    test(`your message to the lead shows only your words, with a grey "+ Desk update" that opens to a tidy list (${scheme}, ${name}) (#822)`, async ({ page, cue }) => {
      await setUp(page, cue, scheme, size);
      const chat = page.locator(".dk-chat");
      // Folded: your words and the note, nothing of the block.
      for (const raw of RAW) await expect(chat).not.toContainText(raw);
      const asked = bubble(page, ASKED);
      await expect(asked).toHaveCount(1);
      const fold = asked.locator(".cv-desk-note .cv-voice-ask");
      await expect(fold).toHaveText("+ Desk update");
      await expect(fold).toHaveAttribute("aria-expanded", "false");
      // Muted grey, not voice's teal.
      const [note, muted, voice] = await asked.locator(".cv-desk-note").evaluate((el) => {
        const probe = (v) => { const d = document.createElement("div"); d.style.color = `var(${v})`; el.append(d); const c = getComputedStyle(d).color; d.remove(); return c; };
        return [getComputedStyle(el).color, probe("--muted"), probe("--voice")];
      });
      expect(note).toBe(muted);
      expect(note).not.toBe(voice);

      // A changes-only block, opened: by section, each item its #id, title, who has it and where it stands.
      await fold.click();
      await expect(fold).toHaveText("− Desk update");
      const list = asked.locator(".cv-desk-up");
      await expect(list.locator(".cv-du-h")).toHaveText(["To route1", "Moved2", "Done or gone2", "Free sessions2"]);
      const rows = await list.locator(".cv-du-r").evaluateAll((els) => els.map((el) => [el.querySelector(".cv-du-id")?.textContent || "", el.querySelector(".cv-du-tt")?.textContent || "", el.querySelector(".cv-du-pl")?.textContent || ""]));
      expect(rows).toEqual([
        ["#d21", "Mocks: how the Desk update shows", "New"],
        ["#d3", "Search stays centred in the header · host-lead", "Finishing"],
        ["#d4", "Rail dot blinks for a reply you didn't send · cue-12, after #d3", "Working"],
        ["#d7", "Header count as a tint", "Done or moved"],
        ["", "cue-3 (session)", "Gone"],
        ["", "cue-6 · idle, cue-5 · idle", ""],
      ]);
      for (const raw of RAW) await expect(chat).not.toContainText(raw);
      await expect(list.locator(".cv-du-pl.go")).toHaveText(["New"]);
      // It fits the bubble: no row wider than the chat.
      expect(await asked.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
      if (process.env.SHOT_DIR) await asked.screenshot({ path: `${process.env.SHOT_DIR}/desk-update-changes-${scheme}-${name}.png` });

      // A whole block (any settings in its opening): "whole Desk", its own sections, and the rules one more fold.
      const held = bubble(page, HOLD);
      await expect(held).not.toContainText("Desk lead:");
      const whole = held.locator(".cv-desk-note > .cv-voice-ask");
      await expect(whole).toHaveText("+ Desk update · whole Desk");
      await whole.click();
      await expect(held.locator(".cv-desk-up > .cv-du-h")).toHaveText(["To route1", "Open1", "Parked1", "Free sessions1", "Busy sessions1"]);
      await expect(held.locator(".cv-desk-up")).toContainText("Away switch remembers its time");
      await expect(held.locator(".cv-desk-up > .cv-du-r").last()).toHaveText("cue-9Working");
      const rules = held.locator(".cv-du-f .cv-voice-ask");
      await expect(rules).toHaveText("+ The Desk’s rules for the lead");
      await expect(held).not.toContainText("For independent work");
      await rules.click();
      await expect(held.locator(".cv-du-f .cv-voice-line")).toContainText("For independent work, prefer a suitable idle Desk session");
      await expect(held.locator(".cv-du-f .cv-voice-line")).not.toContainText("</desk>");
      if (process.env.SHOT_DIR) await held.screenshot({ path: `${process.env.SHOT_DIR}/desk-update-whole-${scheme}-${name}.png` });

      // Joined: both your messages, one fold for both updates, each opened in turn.
      const joined = bubble(page, FIRST);
      await expect(joined).toContainText(SECOND);
      await expect(joined.locator(".cv-desk-note")).toHaveCount(1);
      const both = joined.locator(".cv-desk-note > .cv-voice-ask");
      await expect(both).toHaveText("+ Desk updates · 2");
      await both.click();
      await expect(joined.locator(".cv-du-sep")).toHaveCount(1);
      for (const raw of RAW) await expect(chat).not.toContainText(raw);
      await expect(page.locator("[title]")).toHaveCount(0);
    });
  }
}

test("the chat takes out every Desk block Cue writes, and nothing of yours (#822)", async ({ page, cue }) => {
  await cue.open("desk");
  const said = await page.evaluate(() => [
    deskSaid('<desk id="cue" name="cue" project="cue" changes="only">\nx\n</desk>\n\nHi'),
    deskSaid('<desk project="cue" lane="2" extra="a b">\nx\n</desk>\n\nHi'),
    deskSaid('<desk id="c" name="A > B" project="cue">\nx\n</desk>\n\nHi'),
    deskSaid('<desk project="cue" changes="only">\nx\n</desk>\n\nOne\n<desk project="cue">\ny\n</desk>\n\nTwo'),
    deskSaid('<desk project="cue">\nx\n</desk>\nHi'),
    deskSaid("<desk> is a tag"),
    deskSaid('I wrote <desk project="cue" changes="only">\nx\n</desk>\n\nhere'),
  ]);
  expect(said).toEqual(["Hi", "Hi", "Hi", "One\nTwo", "Hi", "<desk> is a tag", 'I wrote <desk project="cue" changes="only">\nx\n</desk>\n\nhere']);
  const blocks = await page.evaluate(() => deskBlocks('<desk project="cue" changes="only">\nx\n</desk>\n\nOne\n<desk id="c" project="cue">\ny\nz\n</desk>\n\nTwo'));
  expect(blocks).toEqual([{ attrs: { project: "cue", changes: "only" }, body: "x" }, { attrs: { id: "c", project: "cue" }, body: "y\nz" }]);
});
