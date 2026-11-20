// A card that asks several questions (#461): one tab per question, ✓ once it has an answer, one Send for all. What
// you pick or type goes with the question whose tab is open, never another one.
import { test, expect } from "./cue.mjs";

const Q = ["Keep the stop button?", "Should the new band replace the old one, or sit above it?", "Which tests should it run after?"];
// Runs in the page: its questions written out, not Q.
const threeQuestions = (st) => {
  st.items.find((i) => i.id === "n6").tool_input.questions = [
    { question: "Keep the stop button?", header: "Stop", multiSelect: false, options: [{ label: "Keep it" }, { label: "Drop it" }] },
    { question: "Should the new band replace the old one, or sit above it?", header: "Band", multiSelect: false, options: [{ label: "Replace it" }, { label: "Sit above" }, { label: "Show me both" }] },
    { question: "Which tests should it run after?", header: "Tests", multiSelect: false, options: [{ label: "Band only" }, { label: "Whole UI suite" }] },
  ];
};
/** The Inbox, open on the session with the three questions. */
async function open(cue, page) {
  await cue.open("board-waiting");
  await cue.change(threeQuestions);
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="api"]').click();
  const pg = page.locator(".inbox-page");
  return {
    tabs: pg.locator(".qc-tab"),
    tab: (n) => pg.locator(".qc-tab").nth(n).click(),
    pick: (label) => pg.locator(".qc-opt", { hasText: label }).click(),
    type: async (text) => { await pg.locator('textarea[data-text="n6"]').fill(text); await pg.locator('[data-act="send"][data-id="n6"]').click(); },
    send: pg.locator('[data-act="submit-answers"]'),
    shown: pg.locator(".qc-sec .qtext"),
  };
}
const answers = async (cue) => (await cue.called("respond", { id: "n6" })).args.decision.answers;
const ticked = (c) => c.tabs.filter({ has: c.tabs.page().locator(".qc-tn", { hasText: "✓" }) });

test("one tab per question, the open one's choices under them; one Send; no hover text, no left stripe", async ({ cue, page }) => {
  const c = await open(cue, page);
  await expect(c.tabs).toHaveText(["1Stop", "2Band", "3Tests"]);
  await expect(c.tabs.nth(0)).toHaveAttribute("aria-selected", "true");
  await expect(c.shown).toHaveText([Q[0]]);
  await c.tab(1);
  await expect(c.shown).toHaveText([Q[1]]);
  await expect(c.tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator('.inbox-page [data-act="submit-answers"]')).toHaveCount(1);
  await expect(page.locator(".inbox-page .qc [title]")).toHaveCount(0);
  for (const el of [".qc-tabs", ".qc-tab", ".qc-sec"]) expect(await page.locator(`.inbox-page ${el}`).first().evaluate((e) => getComputedStyle(e).borderLeftWidth)).toBe("0px");
});

test("pick only: each tab's pick goes with its own question, its tab ticked", async ({ cue, page }) => {
  const c = await open(cue, page);
  await c.pick("Drop it");
  await expect(ticked(c)).toHaveCount(1);
  await expect(c.tabs.nth(0)).toHaveAttribute("aria-selected", "true");   // picking doesn't move you on
  await c.tab(1); await c.pick("Show me both");
  await c.tab(2); await c.pick("Band only");
  await expect(ticked(c)).toHaveCount(3);
  await expect(c.send).toHaveText("Send 3 answers");
  await c.send.click();
  expect(await answers(cue)).toEqual({ [Q[0]]: "Drop it", [Q[1]]: "Show me both", [Q[2]]: "Band only" });
});

test("type only: each typed answer goes with the question whose tab is open", async ({ cue, page }) => {
  const c = await open(cue, page);
  await c.type("only if it fits");
  await expect(page.locator(".inbox-page .typed-answer")).toHaveText("Your answer: “only if it fits”");
  await c.tab(1); await c.type("above, smaller");
  await c.tab(2); await c.type("none yet");
  await expect(ticked(c)).toHaveCount(3);
  expect(await cue.calls("respond")).toEqual([]);
  await c.send.click();
  expect(await answers(cue)).toEqual({ [Q[0]]: "only if it fits", [Q[1]]: "above, smaller", [Q[2]]: "none yet" });
});

test("pick on one tab, pick on another, type on a third: each with its own question", async ({ cue, page }) => {
  const c = await open(cue, page);
  await c.pick("Keep it");
  await c.tab(1); await c.pick("Sit above");
  await c.tab(2);
  await expect(page.locator('.inbox-page textarea[data-text="n6"]')).toHaveAttribute("placeholder", "Your own answer to question 3…");
  await c.type("the band's and the chat's");
  await expect(c.shown).toHaveText([Q[2]]);
  await c.send.click();
  expect(await answers(cue)).toEqual({ [Q[0]]: "Keep it", [Q[1]]: "Sit above", [Q[2]]: "the band's and the chat's" });
});

test("changing an answer: back on its tab, the new pick (or typed answer) replaces the old one", async ({ cue, page }) => {
  const c = await open(cue, page);
  await c.pick("Keep it");
  await c.tab(1); await c.pick("Replace it");
  await c.tab(2); await c.pick("Band only");
  await c.tab(0); await c.pick("Drop it");
  await expect(page.locator(".inbox-page .qc-opt.on .qc-lt")).toHaveText(["Drop it"]);
  await c.tab(1); await c.type("ask me later");
  await expect(page.locator(".inbox-page .qc-opt.on")).toHaveCount(0);
  await c.tab(2);
  await expect(page.locator(".inbox-page .qc-opt.on .qc-lt")).toHaveText(["Band only"]);
  await c.send.click();
  expect(await answers(cue)).toEqual({ [Q[0]]: "Drop it", [Q[1]]: "ask me later", [Q[2]]: "Band only" });
});

test("Send with one left unanswered: it says how many, stays off, and its tab has no tick", async ({ cue, page }) => {
  const c = await open(cue, page);
  await c.pick("Keep it");
  await c.tab(2); await c.pick("Whole UI suite");
  await expect(c.send).toHaveText("2 of 3 answered");
  await expect(c.send).toBeDisabled();
  await expect(c.tabs.nth(1).locator(".qc-tn")).toHaveText("2");
  await expect(c.tabs.nth(1)).not.toHaveClass(/\bdone\b/);
  await c.send.click({ force: true });
  expect(await cue.calls("respond")).toEqual([]);
  await c.tab(1); await c.pick("Sit above");
  await expect(c.send).toBeEnabled();
});
