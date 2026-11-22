// The Desk chat as the board's chat (#688, option D of the chat audit): the Desk's own lines leave every Desk chat, each
// run of them between your turns one plain count line ("N Desk updates · gist · Activity ›", the newest with a dot)
// opening the Activity tab (Dispatcher Actions until now), where each of them is. Lines meant for you stay. The panel
// ends with Activity's last 3 rows. No capitals on the Desk; only your messages have a background.
import { test, expect, openDesk, deskShowPanel } from "./cue.mjs";

const push = (page, sid, e) => page.evaluate(([sid, e]) => { sessionOf(sid).thread.push({ at_ms: Date.now(), ...e }); renderMain(); }, [sid, e]);
const SHOTS = process.env.CUE_SHOTS || "";

for (const scheme of ["light", "dark"]) {
  test(`the lead's chat: Desk lines become one count line per run between your turns, opening Activity (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    await push(page, "lead", { role: "you", text: "Hand these out please." });
    await push(page, "lead", { role: "agent", text: "I'll hand these out now.\n\nROUTE: #d2 -> web-lead | why: it knows the header\nDONE: #d3\nDECIDED: kept it small — one change at a time" });
    await push(page, "lead", { role: "agent", text: "ROUTE: #d4 -> new | why: nobody is free" });
    await push(page, "lead", { role: "agent", text: "Both are moving. Anything else?" });
    const chat = page.locator(".dk-chat");
    // No switch, no Desk line left in the chat; the lead's words stay.
    for (const w of ["Conversation only", "Everything"]) await expect(page.getByRole("button", { name: w, exact: true })).toHaveCount(0);
    await expect(page.locator(".dk-right .seg2, .dk-run, [data-chat-mode]")).toHaveCount(0);
    await expect(chat).toContainText("I'll hand these out now.");
    await expect(chat).toContainText("Both are moving. Anything else?");
    await expect(chat.locator('.dk-sg[data-sg="route"], .dk-sg[data-sg="done"], .decided')).toHaveCount(0);
    // One count line for the run after your words, the newest, with its dot; it says what they were.
    const last = chat.locator(".dk-cnt").last();
    await expect(last).toHaveClass(/\bnew\b/);
    await expect(last.locator(".dk-cnt-dot")).toHaveCount(1);
    await expect(last).toHaveText(/^4 Desk updates · gave #d2 and #d4, #d3 done, decided 1 · Activity ›$/);
    await expect(chat.locator(".dk-cnt-dot")).toHaveCount(1);
    // It grows as lines come in; no "new" badge anywhere.
    await push(page, "lead", { role: "agent", text: "TITLE: #d2 | Header fix" });
    await expect(last).toHaveText(/^5 Desk updates · /);
    await expect(page.locator(".dk-board")).not.toContainText(/\b\d+ new\b/);
    // Your next words start a new run: the old line loses its dot.
    await push(page, "lead", { role: "you", text: "Thanks." });
    await push(page, "lead", { role: "agent", text: "ROUTE: #d5 -> web-lead | why: same area" });
    await expect(chat.locator(".dk-cnt")).toHaveCount(await chat.locator(".dk-cnt").count());
    await expect(chat.locator(".dk-cnt").last()).toHaveText(/^1 Desk update · gave #d5 · Activity ›$/);
    await expect(chat.locator(".dk-cnt-dot")).toHaveCount(1);
    // A line meant for you stays as it is.
    await push(page, "lead", { role: "agent", text: "USER VERIFY: open the header and look at the logo" });
    await expect(chat.locator('.dk-sg[data-sg="user-verify"]')).toHaveCount(1);
    await expect(chat).toContainText("For you to try:");
    // No hover text, no capitals: every card tag and heading as written.
    await expect(page.locator(".dk-board [title]")).toHaveCount(0);
    const caps = await page.evaluate(() => [...document.querySelectorAll(".dk-board *")].filter((el) => el.offsetParent && getComputedStyle(el).textTransform === "uppercase").map((el) => el.className));
    expect(caps).toEqual([]);
    // Only your messages have a fill in the chat (code blocks aside). The chat's own surface (a sticky footer's mask) and
    // status dots are not fills.
    const filled = await page.evaluate(() => {
      const clear = (bg) => !bg || bg === "rgba(0, 0, 0, 0)" || bg === "transparent";
      let s = document.querySelector(".dk-chat"), surface = "";
      while (s && clear(surface = getComputedStyle(s).backgroundColor)) s = s.parentElement;
      return [...document.querySelectorAll(".dk-chat *")].filter((el) => {
        if (el.closest(".mg-you, .cv-you, pre, code, .code-wrap, button, .dk-sg, .dk-band, .mg-id, .mg-chip, .mg-tag, .thumb, kbd")) return false;
        const bg = getComputedStyle(el).backgroundColor, r = el.getBoundingClientRect();
        return !clear(bg) && bg !== surface && (r.width > 12 || r.height > 12);
      }).map((el) => el.className);
    });
    expect(filled).toEqual([]);
    if (SHOTS) await page.locator(".dk2b-chat").screenshot({ path: `${SHOTS}/lead-chat-${scheme}.png` });
    // A click opens the Activity tab: each line is there, newest first.
    await chat.locator(".dk-cnt").first().locator(".dk-cnt-b").click();
    const tab = page.locator('.dk-tab[data-desk-tab="moves"]');
    await expect(tab).toHaveText("Activity");
    await expect(tab).toHaveAttribute("aria-pressed", "true");
    const act = page.locator(".dk-moves");
    await expect(act.locator(".dk-mv-t")).toHaveText("Activity");
    await expect(act.locator('.dk-mv-ln .dk-sg[data-sg="route"]')).toHaveCount(3);
    await expect(act.locator(".dk-mv-ln", { hasText: "Renamed" })).toHaveCount(1);
    await expect(act.locator(".dk-mv-ln .decided")).toHaveCount(1);
    await expect(act.locator(".dk-mv-ln").first()).toContainText("#d5");
  });
}

test("the panel: Now, Waits on you and Working, then Activity's last 3 rows and N more in Activity ›", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskShowPanel(page);
  for (const id of ["d2", "d3", "d4", "d5"]) await push(page, "lead", { role: "agent", text: `ROUTE: #${id} -> web-lead | why: it knows it` });
  const panel = page.locator(".dk2b-chat .dk-panel");
  const heads = await panel.locator(".dk-psh").allTextContents();
  expect(heads.at(-1)).toBe("Activity");
  expect(heads.some((h) => /Just did/.test(h))).toBe(false);
  const sec = panel.locator(".dk-psec.dk-pact");
  await expect(sec.locator(".dk-pdid")).toHaveCount(3);
  await expect(sec.locator(".dk-pdid").first()).toContainText("#d5");
  const more = sec.locator(".dk-pall");
  await expect(more).toHaveText(/^\d+ more in Activity ›$/);
  await more.click();
  await expect(page.locator('.dk-tab[data-desk-tab="moves"]')).toHaveAttribute("aria-pressed", "true");
});

for (const scheme of ["light", "dark"]) {
  test(`an item's chat: its holder's Desk lines are a count line; its panel ends with them (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    await page.locator('.dk-items .dk-it[data-desk-item="d2"]').first().click();
    const right = page.locator(".dk-right");
    const ichat = right.locator(".dk-ichat");
    await ichat.waitFor();
    const sid = await page.evaluate(() => deskHolderSid(deskItem(deskOpen.project, "d2")));
    await push(page, sid, { role: "agent", text: "Working on the header.\n\nTO LEAD: #d2 needs the logo file\nDECIDED (minor): kept the old size — less churn" });
    await expect(ichat).toContainText("Working on the header.");
    await expect(ichat.locator(".dk-cnt").last()).toHaveText(/^2 Desk updates · .*Activity ›$/);
    await expect(ichat.locator('.dk-sg[data-sg="to-lead"], .dk-sg[data-sg="minor"]')).toHaveCount(0);
    await expect(page.locator(".dk-board [title]")).toHaveCount(0);
    const caps = await page.evaluate(() => [...document.querySelectorAll(".dk-board *")].filter((el) => el.offsetParent && getComputedStyle(el).textTransform === "uppercase").map((el) => el.className));
    expect(caps).toEqual([]);
    if (SHOTS) await right.screenshot({ path: `${SHOTS}/item-chat-${scheme}.png` });
    // Its panel (shown) ends with those rows.
    const tog = right.locator(".dk-ptog");
    if ((await tog.count()) && (await tog.textContent()).includes("Show panel")) await tog.click();
    const sec = right.locator(".dk-panel .dk-psec.dk-pact");
    if (await right.locator(".dk-panel").isVisible()) await expect(sec.locator(".dk-pdid")).toHaveCount(2);
  });
}

test("a crew member's page: its panel ends with its lead's chat's last 3 Activity rows, though no chat is drawn there", async ({ page, cue }) => {
  await cue.open("desk");
  // A session's crew (not the lead's, whose chat is always drawn at the left).
  await cue.change((st) => { const lead = st.sessions.find((x) => x.session_id === "lead"); st.sessions.find((x) => x.session_id === "host").team = lead.team; lead.team = []; });
  await openDesk(page);
  const crew = await page.evaluate(() => (sessionOf("host").team || []).map((t) => t.agent_id || t.call));
  expect(crew.length).toBeGreaterThan(0);
  // Its lead's Desk lines, its chat never opened.
  for (const id of ["d2", "d3", "d4", "d5"]) await push(page, "host", { role: "agent", text: `Working on it.\n\nTO LEAD: #${id} needs a look` });
  await page.evaluate(([lead, agent]) => deskShow({ kind: "crew", lead, agent }), ["host", crew[0]]);
  await expect(page.locator(".dk-right .dk-crewpg")).toBeVisible();
  const tog = page.locator(".dk-right .dk-ptog");
  if ((await tog.textContent()).includes("Show panel")) await tog.click();
  const sec = page.locator(".dk-right .dk-panel .dk-psec.dk-pact");
  await expect(sec.locator(".dk-psh")).toHaveText("Activity");
  await expect(sec.locator(".dk-pdid")).toHaveCount(3);
  await expect(sec.locator(".dk-pdid").first()).toContainText("#d5");
  await expect(sec.locator(".dk-pall")).toHaveText("1 more in Activity ›");
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
});

test("a routing call shows once in Activity: the lead's DECIDED line, not again as its move (#420)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await push(page, "lead", { role: "agent", text: "Routed it.\n\nDECIDED: Away mode mocks go to web — it made the last ones" });
  await page.evaluate(() => {
    const d = deskOf(deskSelection());
    d.decisions.push({ project: d.project, id: "c9", call: true, minor: true, act: "call", undo: "can", by: "web-lead", sid: deskLead(d).sid, text: "Away mode mocks go to web", why: "it made the last ones", at: Date.now() / 1000 });
    deskOpenActivity();
  });
  const rows = page.locator(".dk-moves .dk-mv", { hasText: "Away mode mocks go to web" });
  await expect(rows).toHaveCount(1);
  await expect(rows.locator(".decided")).toHaveCount(1);
  // Another call, one no reply said: listed as its move.
  await page.evaluate(() => {
    const d = deskOf(deskSelection());
    d.decisions.push({ project: d.project, id: "c10", call: true, minor: true, act: "call", undo: "can", by: "web-lead", sid: deskLead(d).sid, text: "Header copy stays as it is", why: "", at: Date.now() / 1000 });
    renderMain();
  });
  await expect(page.locator(".dk-moves .dk-mv", { hasText: "Header copy stays as it is" })).toHaveCount(1);
});

test("an item the lead holds: opening its page leaves the panel's Activity the lead's whole Activity", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await deskShowPanel(page);
  expect(await page.evaluate(() => deskHolderSid(deskItem(deskSelection(), "d2")))).toBe("lead");
  await push(page, "lead", { role: "you", text: "Hand these out please." });
  await push(page, "lead", { role: "agent", text: "ROUTE: #d3 -> web-lead | why: it knows it\nROUTE: #d4 -> web-lead | why: free" });
  await push(page, "lead", { role: "you", text: "And #d5." });
  await push(page, "lead", { role: "agent", text: "ROUTE: #d5 -> web-lead | why: same area" });
  const sec = page.locator(".dk2b-chat .dk-panel .dk-psec.dk-pact");
  await expect(sec.locator(".dk-pdid")).toHaveCount(3);
  // Each row's words, without its age (it ticks from 0s to 1s between two reads).
  const words = (ts) => ts.map((t) => t.replace(/\d+[smhd]$/, ""));
  const before = words(await sec.locator(".dk-pdid").allTextContents());
  const all = () => page.evaluate(() => deskActivity(deskOf(deskSelection())).map((r) => (r.x ? `mv:${r.x.id}` : `${r.line.sid}:${r.line.key}`)));
  const whole = await all();
  // Its page folds the lead's chat to #d2's turns: Activity stays the lead's, whole.
  await page.evaluate(() => deskShow({ kind: "item", id: "d2" }));
  await expect(page.locator(".dk-right .dk-ichat")).toBeVisible();
  expect(await all()).toEqual(whole);
  const panel = await page.evaluate(() => { const d = deskOf(deskSelection()), t = document.createElement("div"); t.innerHTML = deskPanelActivity(deskActivity(d), d); return [...t.querySelectorAll(".dk-pdid")].map((r) => r.textContent); });
  expect(words(panel)).toEqual(before);
  if (await sec.isVisible()) expect(words(await sec.locator(".dk-pdid").allTextContents())).toEqual(before);
  await expect(page.locator(".dk-board [title]")).toHaveCount(0);
});
