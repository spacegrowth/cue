// The Desk chat as the board's chat (#688, #789): the Desk's own lines leave every Desk chat, with no count line in
// their place; each of them is in the Activity tab, which you can search and filter by item, who and kind of move.
// Lines meant for you stay. The panel ends with Activity's last 3 rows. No capitals on the Desk; only your messages
// have a background.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, sid, e) => page.evaluate(([sid, e]) => { sessionOf(sid).thread.push({ at_ms: Date.now(), ...e }); renderMain(); }, [sid, e]);
const SHOTS = process.env.CUE_SHOTS || "";

for (const scheme of ["light", "dark"]) {
  test(`the lead's chat: Desk lines leave it, no count line either; Activity has each, searched and filtered (${scheme})`, async ({ page, cue }) => {
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
    // Nothing in their place: no count line, no "3 Desk updates", and nothing of what Cue told the
    // lead with your own words either (#882).
    await expect(chat.locator(".dk-cnt")).toHaveCount(0);
    await expect(chat).not.toContainText(/\d+ Desk updates?/);
    await push(page, "lead", { role: "agent", text: "TITLE: #d2 | Header fix" });
    await push(page, "lead", { role: "you", text: "Thanks." });
    await push(page, "lead", { role: "agent", text: "ROUTE: #d5 -> web-lead | why: same area" });
    await expect(chat).toContainText("Thanks.");
    await expect(chat.locator(".dk-cnt, .dk-sg[data-sg=\"route\"], .dk-sg[data-sg=\"title\"]")).toHaveCount(0);
    await expect(page.locator(".dk-board")).not.toContainText(/\b\d+ new\b/);
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
    // The Activity tab: each line is there, newest first.
    await page.locator('.dk-tab[data-desk-tab="moves"]').click();
    const tab = page.locator('.dk-tab[data-desk-tab="moves"]');
    await expect(tab).toHaveText("Activity");
    await expect(tab).toHaveAttribute("aria-pressed", "true");
    const act = page.locator(".dk-moves");
    await expect(act.locator(".dk-mv-t")).toHaveText("Activity");
    await expect(act.locator('.dk-mv-ln .dk-sg[data-sg="route"]')).toHaveCount(3);
    await expect(act.locator(".dk-mv-ln", { hasText: "Renamed" })).toHaveCount(1);
    await expect(act.locator(".dk-mv-ln .decided")).toHaveCount(1);
    await expect(act.locator(".dk-mv-ln").first()).toContainText("#d5");
    // Search and filters (#789): by kind of move, by item, by words; Every… puts them all back.
    const routes = await act.locator(".dk-mv").count();
    await act.locator('select[data-dk-act-f="kind"]').selectOption("route");
    await expect(act.locator(".dk-mv")).toHaveCount(3);
    await act.locator('select[data-dk-act-f="item"]').selectOption("d5");
    await expect(act.locator(".dk-mv")).toHaveCount(1);
    await act.locator('select[data-dk-act-f="item"]').selectOption("");
    await act.locator('select[data-dk-act-f="kind"]').selectOption("");
    await expect(act.locator(".dk-mv")).toHaveCount(routes);
    await act.locator(".dk-mv-q").fill("nobody is free");
    await expect(act.locator(".dk-mv")).toHaveCount(1);
    await expect(act.locator(".dk-mv")).toContainText("#d4");
    await expect(act.locator(".dk-mv-q")).toBeFocused();
    await act.locator(".dk-mv-q").fill("no such words here");
    await expect(act.locator(".dk-mv")).toHaveCount(0);
    await expect(act).toContainText("Nothing here fits that search.");
  });
}

test("the lead's chat: no panel, Now and Waits on you in its strip, its routes in Activity newest first", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // The lead's chat has no panel and no Hide/Show panel (692dc421): the strip over the chat instead.
  await expect(page.locator(".dk2b-chat .dk2b-strip")).toBeVisible();
  await expect(page.locator('.dk2b-chat [data-desk-panel="dispatcher-hidden"]')).toHaveCount(0);
  await expect(page.locator(".dk2b-chat .dk-pact")).toBeHidden();
  for (const id of ["d2", "d3", "d4", "d5"]) await push(page, "lead", { role: "agent", text: `ROUTE: #${id} -> web-lead | why: it knows it` });
  await expect(page.locator(".dk2b-chat")).not.toContainText("Just did");
  await page.locator('.dk-tab[data-desk-tab="moves"]').click();
  await expect(page.locator('.dk-tab[data-desk-tab="moves"]')).toHaveAttribute("aria-pressed", "true");
  const rows = page.locator(".dk-moves .dk-mv", { hasText: "web-lead" });
  await expect(rows).toHaveCount(4);
  await expect(rows.first()).toContainText("#d5");
});

for (const scheme of ["light", "dark"]) {
  test(`an item's chat: its holder's Desk lines leave it, for the item's Activity tab (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await openDesk(page);
    await page.locator('.dk-items .dk-it[data-desk-item="d2"]').first().click();
    const right = page.locator(".dk-right");
    // The item opens its holder's chat (desk/c.js deskCItemChat).
    const sid = await page.evaluate(() => deskHolderSid(deskItem(deskOpen.project, "d2")));
    const ichat = right.locator(`.dkn-sp[data-desk-page="session"] [data-desk-chat="${sid}"]`);
    await ichat.waitFor();
    await push(page, sid, { role: "agent", text: "Working on the header.\n\nTO LEAD: #d2 needs the logo file\nDECIDED (minor): kept the old size — less churn" });
    await expect(ichat).toContainText("Working on the header.");
    await expect(ichat.locator(".dk-cnt")).toHaveCount(0);
    await expect(ichat.locator('.dk-sg[data-sg="to-lead"], .dk-sg[data-sg="minor"]')).toHaveCount(0);
    await expect(ichat).not.toContainText("needs the logo file");
    // The item's page has them in its Activity tab.
    await right.locator('.dkn-stab[data-desk-c-stab="activity"]').click();
    await expect(right.locator(".dkn-sact")).toContainText("needs the logo file");
    await expect(page.locator(".dk-board [title]")).toHaveCount(0);
    const caps = await page.evaluate(() => [...document.querySelectorAll(".dk-board *")].filter((el) => el.offsetParent && getComputedStyle(el).textTransform === "uppercase").map((el) => el.className));
    expect(caps).toEqual([]);
    if (SHOTS) await right.screenshot({ path: `${SHOTS}/item-chat-${scheme}.png` });
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
  // A crew member's page has no panel since desk-live (53bb5b7296): its lead's Desk lines are its Activity tab, all of
  // them, newest first.
  await expect(page.locator(".dk-right .dk-panel")).toHaveCount(0);
  await page.locator('.dk-right .dkn-stab[data-desk-c-stab="activity"]').click();
  const sec = page.locator(".dk-right .dkn-crew .dkn-sfiles");
  await expect(sec.locator(".dk-pdid")).toHaveCount(4);
  await expect(sec.locator(".dk-pdid").first()).toContainText("#d5");
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
  expect(await page.evaluate(() => deskHolderSid(deskItem(deskSelection(), "d2")))).toBe("lead");
  await push(page, "lead", { role: "you", text: "Hand these out please." });
  await push(page, "lead", { role: "agent", text: "ROUTE: #d3 -> web-lead | why: it knows it\nROUTE: #d4 -> web-lead | why: free" });
  await push(page, "lead", { role: "you", text: "And #d5." });
  await push(page, "lead", { role: "agent", text: "ROUTE: #d5 -> web-lead | why: same area" });
  // The lead's chat has no panel to show (692dc421); its Activity rows are still drawn, out of sight.
  const sec = page.locator(".dk2b-chat .dk-panel .dk-psec.dk-pact");
  await expect(sec.locator(".dk-pdid")).toHaveCount(3);
  // Each row's words, without its age (it ticks from 0s to 1s between two reads).
  const words = (ts) => ts.map((t) => t.replace(/\d+[smhd]$/, ""));
  const before = words(await sec.locator(".dk-pdid").allTextContents());
  const all = () => page.evaluate(() => deskActivity(deskOf(deskSelection())).map((r) => (r.x ? `mv:${r.x.id}` : r.c ? `chk:${r.c.id}` : `${r.line.sid}:${r.line.key}`)));
  const whole = await all();
  // Its page (the lead's chat, desk/c.js deskCItemChat): Activity stays the lead's, whole.
  await page.evaluate(() => deskShow({ kind: "item", id: "d2" }));
  await expect(page.locator('.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="lead"]')).toBeVisible();
  expect(await all()).toEqual(whole);
  const panel = await page.evaluate(() => { const d = deskOf(deskSelection()), t = document.createElement("div"); t.innerHTML = deskPanelActivity(deskActivity(d), d); return [...t.querySelectorAll(".dk-pdid")].map((r) => r.textContent); });
  expect(words(panel)).toEqual(before);
  if (await sec.isVisible()) expect(words(await sec.locator(".dk-pdid").allTextContents())).toEqual(before);
  await expect(page.locator(".dk-board [title]")).toHaveCount(0);
});
