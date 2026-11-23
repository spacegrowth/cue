// The Desk's look: the head strip, Run by, the lead's card, the session rows and their dots, the Working group,
// and Commit to main on a row.
import { test, expect, openDesk } from "./cue.mjs";
import { listOrder, crewNotLead, blinkOf, boxOf, openDone, openSessions, groupOf, statusOf } from "./desk.mjs";

// ---------- the left column's head and rows (#153: option B) ----------

const cssOf = (loc, ...props) => loc.evaluate((el, ps) => { const c = getComputedStyle(el); return ps.map((p) => c.getPropertyValue(p)); }, props);
// The same, found and measured in one go by a CSS selector: a redraw replaces the Desk, and a locator finds its element in
// one call and reads it in the next, which a redraw can land between (then it reads the old element, gone: no style).
const liveCss = (page, sel, ...props) => page.evaluate(([sel, ps]) => { const c = getComputedStyle(document.querySelector(sel)); return ps.map((p) => c.getPropertyValue(p)); }, [sel, props]);

test("the head is one strip in the accent, flush across the card's top: the name (a switcher), its meta, then the lead's card in it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const left = page.locator(".dk-sessions"), strip = left.locator(".dk-strip");
  await expect(strip).toHaveCount(1);
  // Switching Desks is the window header's now (#666): the strip leads with ‹ Items N, back to the list.
  await expect(strip.locator("[data-desk-switch]")).toHaveCount(0);
  await expect(strip.locator(".dk2b-items")).toHaveText(/^‹ Items \d+$/);
  await expect(strip.locator(".dk-sh b")).toHaveText("cue");
  await expect(strip.locator(".dk-sub")).toHaveText("1 session · 1 working");
  await expect(strip.locator("[data-desk-change]")).toHaveCount(1);
  // The strip comes first in the card, flush to its top and sides, round only at the top as the card is.
  expect(await left.evaluate((el) => el.firstElementChild.classList.contains("dk-strip"))).toBe(true);
  const card = await boxOf(left), s = await boxOf(strip);
  expect(Math.round(s.x - card.x)).toBe(0);
  expect(Math.round(card.right - s.right)).toBe(0);
  expect(Math.round(s.y - card.y)).toBe(0);
  // Light: the accent (the window's dark green) with on-accent text, no edge (#153, option B).
  const [bg, ink, edge, radius, bottom] = await cssOf(strip, "background-color", "color", "box-shadow", "border-top-left-radius", "border-bottom-left-radius");
  expect(bg).toBe("rgb(63, 74, 34)");
  expect(ink).toBe("rgb(255, 255, 255)");
  expect(edge).toBe("none");
  expect(radius).toBe("14px");
  expect(bottom).toBe("0px");
  // Pick O: the column lifts off the window with a light hairline and a shadow; a 3px lime line closes the strip (the
  // lead's working line, #263: lime while it's idle, the chat's working light while it works; its own test below).
  expect((await cssOf(left, "box-shadow"))[0]).toMatch(/0px 0px 0px 1px.*0px 10px 28px/);
  expect(await cssOf(left.locator(".dk-strip > .dk-lbar"), "height")).toEqual(["3px"]);
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("Run by shows the current Dispatcher's accessible agent icon", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const runBy = page.locator(".dk-lcard");
  await expect(runBy.getByRole("img", { name: "Claude Code" })).toBeVisible();
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    const s = st.sessions.find((x) => x.session_id === "host");
    s.harness = "codex";
    d.lead = { sid: s.session_id, name: s.name, state: "live" };
  });
  await expect(runBy.getByRole("img", { name: "Codex" })).toHaveText("X");
  await expect(runBy.getByRole("img", { name: "Claude Code" })).toHaveCount(0);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").harness = "pi"; });
  await expect(runBy.getByRole("img", { name: "Pi" })).toHaveText("π");
  await expect(runBy.getByRole("img", { name: "Dispatcher" })).toBeVisible();
});

test("Run by gives idle and its icons contrasting colors in both themes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    const lead = st.sessions.find((s) => s.session_id === "lead");
    Object.assign(lead, { state: "idle", harness: "codex" });
    st.items = st.items.filter((item) => item.session_id !== lead.session_id);
  });
  const card = page.locator(".dk-lcard");
  const status = card.locator(".dk-st");
  await expect(status).toHaveText("idle");
  for (const colorScheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme });
    await card.click();
    await expect(card).toHaveClass(/\bon\b/);
    const [surface, ink] = await liveCss(page, ".dk-lcard", "background-color", "color");
    expect(await liveCss(page, ".dk-lcard .dk-st", "color", "opacity")).toEqual([ink, "1"]);
    expect((await liveCss(page, ".dk-lcard .dk-st i.idle", "box-shadow"))[0]).toContain(ink);
    expect(await liveCss(page, ".dk-lcard .dk-ric svg", "color")).toEqual([ink]);
    expect(await liveCss(page, ".dk-lcard .dk-sren", "color")).toEqual([ink]);
    expect(await liveCss(page, '.dk-lcard [role="img"][aria-label="Codex"]', "background-color", "color")).toEqual([ink, surface]);
    // Another session's chat takes the list's place (#666): back in the list, the card isn't the open one.
    await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
    await page.locator(".dk-right .dkn-sback .dk-back").first().click();
    await expect(page.locator('.dk-sessions [data-desk-chat-of="host"]')).toHaveClass(/\bon\b/);
    await expect(card).not.toHaveClass(/\bon\b/);
    const [statusInk] = await liveCss(page, ".dk-lcard .dk-st", "color");
    expect((await liveCss(page, ".dk-lcard .dk-st i.idle", "box-shadow"))[0]).toContain(statusInk);
  }
});

test("the lead's card in the strip: a Lead pill, its state and the item it holds; open, lit lime as any open row, in both themes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const lead = page.locator('.dk-strip .dk-lcard[data-desk-lead-chat="lead"]');
  await expect(lead.locator(".dk-larrow")).toHaveText("›");   // open, so it has the › (#219)
  // Pick M1 (#446): no "Run by" head; "Lead" in a lime pill right after its name, before its state.
  await expect(lead.locator(".dk-lk")).toHaveCount(0);
  await expect(page.locator(".dk-strip .dk-lh")).toHaveCount(0);
  await expect(lead.locator(".dk-s1 .dk-ltag")).toHaveText("Lead");
  expect(await lead.locator(".dk-ltag").evaluate((el) => !!el.previousElementSibling && el.nextElementSibling.matches(".dk-st"))).toBe(true);
  await expect(page.locator(".dk-scroll .dk-ltag")).toHaveCount(0);   // plain session rows stay as they are
  // A divider under the card, between it and the sessions.
  expect(await cssOf(lead, "border-bottom")).toEqual([expect.stringMatching(/^1px solid/)]);
  // The line sits over the card, not under it (#359); it runs the strip's full width.
  expect(await page.locator(".dk-strip > .dk-lbar").evaluate((el) => el.nextElementSibling.matches(".dk-lcard"))).toBe(true);
  // Not open, the card is white like the rows and its pill is the selection lime; open, the pill turns over.
  const pillBg = () => lead.locator(".dk-ltag").evaluate((el) => getComputedStyle(el).backgroundColor);
  const litPill = await pillBg();
  await page.locator(".dk-sessions .dk-scroll .dk-srow:not(.crew)").first().click();
  await page.locator(".dk-right .dkn-sback .dk-back").first().click();   // its chat took the list's place (#666): back
  await expect(page.locator(".dk-sessions .dk-scroll .dk-srow:not(.crew)").first()).toHaveClass(/\bon\b/);
  await expect(lead).not.toHaveClass(/\bon\b/);
  expect(await pillBg()).not.toBe(litPill);
  await expect(lead.locator(".dk-ltag")).toBeVisible();
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await page.waitForTimeout(150); await page.screenshot({ clip: await page.locator(".dk-sessions").boundingBox(), path: `${process.env.SHOT_DIR}/cue-desk-m1-${c}.png` }); }
  await page.emulateMedia({ colorScheme: "light" });
  await lead.click();
  await expect(lead).toHaveClass(/\bon\b/);
  const [sb, lb, cb] = await page.evaluate(() => [".dk-sessions .dk-strip", ".dk-strip .dk-lbar", ".dk-strip .dk-lcard"].map((q) => document.querySelector(q).getBoundingClientRect()));
  expect(Math.round(lb.bottom)).toBe(Math.round(cb.top));
  expect([Math.round(lb.left), Math.round(lb.right)]).toEqual([Math.round(sb.left), Math.round(sb.right)]);
  expect(Math.round(cb.bottom)).toBe(Math.round(sb.bottom));
  await expect(lead.locator(".dk-nm")).toHaveText("web-lead");
  await expect(lead.locator(".dk-st")).toHaveText("needs you");
  await expect(lead.locator(".dk-st > i.ask")).toHaveCount(1);
  await expect(lead.locator(".dk-s2 .dk-id")).toHaveText("Feature #d2");
  await expect(lead.locator(".dk-s2 svg.dk-kic")).toHaveCount(1);
  // Its chat shows: the card lights lime, as an open row does.
  await expect(lead).toHaveClass(/\bon\b/);
  let [bg, ring] = await cssOf(lead, "background-color", "box-shadow");
  expect(bg).toBe("rgb(210, 222, 150)");
  // Another session's chat: the card goes back to white, that row lights in place.
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await page.locator(".dk-right .dkn-sback .dk-back").first().click();   // its chat took the list's place (#666): back
  await expect(page.locator('.dk-sessions [data-desk-chat-of="host"]')).toHaveClass(/\bon\b/);
  await expect(lead).not.toHaveClass(/\bon\b/);
  [bg, ring] = await cssOf(lead, "background-color", "box-shadow");
  expect(bg).toBe("rgb(255, 255, 255)");
  expect(ring).toBe("none");
  await lead.click();
  await expect(lead).toHaveClass(/\bon\b/);
  // B2 (#666): the lead's chat is the card the sessions list stands in for; picking the lead lights it there, the
  // list staying open (desk/act.js deskLeadChat).
  await expect(page.locator(".dk-board > .dk-sessions")).toHaveCount(1);
  expect(await page.evaluate(() => deskTab)).toBe("lead");
  // Dark: the strip is a dark green with the ink, not lime, so the card lights exactly as an open row does.
  await page.emulateMedia({ colorScheme: "dark" });
  const strip = page.locator(".dk-strip");
  const [sbg, sink] = await cssOf(strip, "background-color", "color");
  expect(sbg).not.toBe("rgb(181, 194, 122)");
  expect(sink).not.toBe("rgb(21, 26, 8)");
  const [onBg, onRing] = await cssOf(lead, "background-color", "box-shadow");
  expect(onRing).toBe("none");
  const host = page.locator('.dk-sessions [data-desk-chat-of="host"]');
  await host.click();
  await page.locator(".dk-right .dkn-sback .dk-back").first().click();   // back from its chat to the list (#666)
  await expect(host).toHaveClass(/\bon\b/);
  await expect(lead).not.toHaveClass(/\bon\b/);
  const [offBg] = await cssOf(lead, "background-color");
  expect(offBg).not.toBe(onBg);
  expect(offBg).not.toBe(sbg);
  const [rowBg] = await cssOf(host, "background-color");
  expect(rowBg).toBe(onBg);
  await lead.click();
  await expect(lead).toHaveClass(/\bon\b/);
  // At work, its blinking dot takes the card's ink, so it shows on the lime.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").state = "working"; st.items = st.items.filter((i) => i.session_id !== "lead"); });
  const dot = lead.locator(".dk-st > i");
  await expect(dot).not.toHaveClass(/ask|idle/);
  const [dbg, dink] = await cssOf(dot, "background-color", "color");
  expect(dbg).toBe(dink);
  expect(await blinkOf(dot)).toBe("pulse");
});

test("no line under the strip: the list starts right after it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await expect(page.locator(".dk-sessions .dk-sum")).toHaveCount(0);
  // The lead's working line (#263) is inside the strip, over the lead's card (#359): nothing between the strip and the list.
  // Never a "Needs your decision" block, even while a request waits (#431): the list right after the strip.
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").ask = null; });
  await expect(page.locator(".dk-sessions .dk-approvals")).toHaveCount(0);
  expect(await page.locator(".dk-sessions .dk-strip").evaluate((el) => el.nextElementSibling.classList.contains("dk-scroll"))).toBe(true);
});

test("the lead's card is a session row's size: same padding, name, item line and indent (#269)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const lead = page.locator(".dk-strip .dk-lcard"), row = page.locator(".dk-sessions .dk-scroll .dk-srow:not(.crew)").first();
  const look = (loc) => loc.evaluate((el) => {
    const c = getComputedStyle(el), nm = getComputedStyle(el.querySelector(".dk-nm")), s2 = el.querySelector(".dk-s2");
    return { pad: c.padding, nm: `${nm.fontSize} ${nm.fontWeight}`, s2: s2 ? getComputedStyle(s2).fontSize : null,
      indent: s2 ? Math.round(s2.getBoundingClientRect().left - el.getBoundingClientRect().left) : null };
  });
  const [a, b] = [await look(lead), await look(row)];
  expect(a.pad).toBe(b.pad);
  expect(a.nm).toBe(b.nm);
  if (a.s2 && b.s2) { expect(a.s2).toBe(b.s2); expect(a.indent).toBe(b.indent); }
  // Like every row, the card has a second line, even idle and on no item: its latest reply, else its open items.
  const leadSid = await lead.getAttribute("data-desk-lead-chat");
  await cue.change((st, sid) => { const s = st.sessions.find((x) => x.session_id === sid); s.state = "idle"; s.doing = ""; for (const i of st.desk.projects.find((x) => x.project === "cue").items) if (i.holder?.sid === sid) i.holder = null; }, leadSid);
  await expect(lead.locator(".dk-s2 .dk-id")).toHaveCount(0);
  await expect(lead.locator(".dk-s2")).toHaveCount(1);
  await expect(lead.locator(".dk-s2")).not.toHaveText("");
  if (process.env.SHOT_DIR) for (const c of ["light", "dark"]) { await page.emulateMedia({ colorScheme: c }); await page.waitForTimeout(150); await page.screenshot({ clip: await page.locator(".dk-sessions").boundingBox(), path: `${process.env.SHOT_DIR}/cue-desk-runby-${c}.png` }); }
});

test("the line under Run by is the chat's working light while the lead works, a plain lime line when idle (#263)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const lead = await page.locator(".dk-strip .dk-lcard").getAttribute("data-desk-lead-chat");
  const set = (state, extra = {}) => cue.change((st, a) => { Object.assign(st.sessions.find((x) => x.session_id === a.lead), { stuck_ms: 0, compacting_ms: 0, doing_ms: Date.now(), since_ms: Date.now(), ...a.v }); }, { lead, v: { state, ...extra } });
  const bar = page.locator(".dk-sessions .dk-lbar");
  const sel = () => page.evaluate(() => { const i = document.createElement("i"); i.style.background = "var(--sel)"; document.body.append(i); const c = getComputedStyle(i).backgroundColor; i.remove(); return c; });
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    await set("working");
    // The chat's own .sweep (chat.css), moving: same class, same keyframes, same height, at the line's top.
    const sw = bar.locator("i.sweep");
    await expect(sw).toHaveCount(1);
    await expect(bar).toHaveClass(/\bon\b/);
    // (The harness turns every animation off, so the rule the light matches is read instead.)
    const st = await sw.evaluate((el) => {
      const rules = [...document.styleSheets].flatMap((sh) => [...sh.cssRules]).filter((r) => r.selectorText && !r.selectorText.startsWith("*") && el.matches(r.selectorText) && r.style.animationName);
      return { name: rules.at(-1)?.style.animationName, h: getComputedStyle(el).height, top: el.getBoundingClientRect().top - el.parentElement.getBoundingClientRect().top, chat: el.hasAttribute("data-sweep") };
    });
    expect(st).toEqual({ name: "sweep", h: "3px", top: 0, chat: false });
    // It runs over the panel, as along a chat's top, not over the lime.
    expect(await bar.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(await sel());
    if (process.env.SHOT_DIR) await page.locator(".dk-sessions").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-lead-bar-working-${c}.png` });
    // Stuck: still and amber, as the chat's.
    await set("working", { stuck_ms: Date.now() - 600000 });
    await expect(bar.locator("i.sweep.stalled")).toHaveCount(1);
    // Idle: no light, a plain lime line under the card.
    await set("idle");
    await expect(bar.locator("i")).toHaveCount(0);
    await expect(bar).not.toHaveClass(/\bon\b/);
    expect(await bar.evaluate((el) => getComputedStyle(el).backgroundColor)).toBe(await sel());
    expect(await bar.evaluate((el) => el.getBoundingClientRect().height)).toBe(3);
    if (process.env.SHOT_DIR) await page.locator(".dk-sessions").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-lead-bar-idle-${c}.png` });
  }
});

test("a row's state dot sits after its name with its word: working blinks, waiting on you a still red dot, idle a hollow ring", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    st.sessions.push({ ...site, session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [] },
      { ...site, session_id: "asker", project: "cue", name: "asker", state: "deciding", thread: [] });
    st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id: "n-asker", session_id: "asker" });
    st.desk.projects.find((d) => d.project === "cue").members.push("spare", "asker");
  });
  // The test page stills every animation; take that off to see the dot's own.
  await page.evaluate(() => [...document.querySelectorAll("style")].filter((s) => s.textContent.includes("animation:none!important")).forEach((s) => s.remove()));
  const left = page.locator(".dk-sessions");
  const host = left.locator('[data-desk-chat-of="host"]');
  // Its type's icon, its name, then the dot and its word (#202); no ring.
  expect(await host.locator(".dk-s1").evaluate((el) => [...el.children].map((c) => c.className.split(" ")[0]))).toEqual(["dk-ric", "dk-nm", "dk-ren", "dk-st"]);
  await expect(left.locator(".dk-ring, .dk-mk")).toHaveCount(0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  expect(await cssOf(host.locator(".dk-st > i"), "animation-name")).toEqual(["pulse"]);
  const ask = left.locator('[data-desk-chat-of="asker"] .dk-st > i.ask');
  const [abg, aanim] = await cssOf(ask, "background-color", "animation-name");
  expect(abg).toBe("rgb(155, 44, 31)");
  expect(aanim).toBe("none");
  await expect(left.locator('[data-desk-chat-of="asker"] .dk-st')).toHaveText("needs you");
  const idle = left.locator('[data-desk-chat-of="spare"] .dk-st > i.idle');
  const [ibg, ishadow, ianim] = await cssOf(idle, "background-color", "box-shadow", "animation-name");
  expect(ibg).toBe("rgba(0, 0, 0, 0)");
  expect(ishadow).toContain("inset");
  expect(ianim).toBe("none");
  // Rows stay put: the blink moves nothing.
  const before = await boxOf(host);
  await page.waitForTimeout(400);
  expect(await boxOf(host)).toEqual(before);
});

test("under each session's name: the item it holds with its kind's icon, lined up with the name", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  const host = page.locator('.dk-sessions [data-desk-chat-of="host"]');
  await expect(host.locator(".dk-s2 svg.dk-kic")).toHaveCount(1);
  await expect(host.locator(".dk-s2 .dk-id")).toHaveText("Bug #d5");
  await expect(host.locator(".dk-s2")).toContainText("Hosted sessions can't open the canvas");
  // It starts under the name, not under the type's icon.
  const name = await boxOf(host.locator(".dk-nm")), what = await boxOf(host.locator(".dk-s2"));
  expect(Math.abs(what.x - name.x)).toBeLessThanOrEqual(1);
});

test("done teammates fold under their lead as \"N done · show\", opened in place; done but not committed stays out, tagged", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    const lead = st.sessions.find((x) => x.session_id === "lead"), t = Date.now(), base = lead.team[0];
    const mate = (agent_id, name, state) => ({ ...base, agent_id, call: agent_id, name, state, what: `${name}'s work`, step: "", ended_ms: t - 60000 });
    lead.team.push({ ...mate("id-tidy", "tidy-copy", "done"), uncommitted: true }, mate("id-a", "old-a", "committed"), mate("id-b", "old-b", "committed"), mate("id-c", "old-c", "aside"));
  });
  await crewNotLead(cue);
  const left = page.locator(".dk-sessions");
  const crewNames = () => left.locator(".dk-srow.crew").evaluateAll((els) => els.map((el) => el.dataset.deskCrew));
  // Folded: the three finished ones are one line; the one not committed stays, tagged.
  const fold = left.locator('[data-desk-done-fold="lead"]');
  await expect(fold).toHaveText("3 done · show");
  expect(await crewNames()).toEqual(["id-search", "id-focus", "id-tidy"]);
  await expect(left.locator('[data-desk-crew="id-tidy"] .dk-pill.loose')).toHaveText("not committed");
  await expect(left.locator('[data-desk-crew="id-search"] .dk-pill.loose')).toHaveCount(0);
  // Opened in place, under the line; nothing above it moves.
  await fold.evaluate((el) => el.scrollIntoView({ block: "nearest" }));
  const above = await left.locator('[data-desk-crew="id-tidy"]').boundingBox();
  await fold.click();
  await expect(fold).toHaveText("3 done · hide");
  await expect(fold).toHaveAttribute("aria-expanded", "true");
  expect(await crewNames()).toEqual(["id-search", "id-focus", "id-tidy", "id-a", "id-b", "id-c"]);
  await expect(left.locator('[data-desk-crew="id-c"] .dk-st')).toHaveText("set aside");
  await expect(left.locator(".dk-srow.past")).toHaveCount(3);
  expect((await left.locator('[data-desk-crew="id-tidy"]').boundingBox()).y).toBe(above.y);
  expect((await left.locator('[data-desk-crew="id-a"]').boundingBox()).y).toBeGreaterThan(above.y);
  await fold.click();
  await expect(left.locator(".dk-srow.past")).toHaveCount(0);
  // Git says its files are committed (Cue marks it): nothing of it left in play, it folds too.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").team.find((t) => t.agent_id === "id-tidy").uncommitted = false; });
  await expect(fold).toHaveText("4 done · show");
  expect(await crewNames()).toEqual(["id-search", "id-focus"]);
  // Committed: still in the fold.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").team.find((t) => t.agent_id === "id-tidy").state = "committed"; });
  await expect(fold).toHaveText("4 done · show");
  // Opened, it stays open across a restart.
  await fold.click();
  await expect(fold).toHaveText("4 done · hide");
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { const lead = st.sessions.find((x) => x.session_id === "lead"); lead.team.push({ ...lead.team[0], agent_id: "id-a", call: "id-a", name: "old-a", state: "committed" }); });
  await crewNotLead(cue);
  await openSessions(page);
  await expect(left.locator('[data-desk-done-fold="lead"]')).toHaveAttribute("aria-expanded", "true");
  await expect(left.locator('[data-desk-crew="id-a"]')).toBeVisible();
});

test("selecting a session lights nothing on the right: it can hold several items (#170)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const items = page.locator(".dkn-items");
  const before = await listOrder(page);
  const ys = () => items.locator("[data-desk-item]").evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().y)));
  const top = await ys();
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
  // A session opened from All sessions (#666), on the right in the list's place; back to the Items: none lit, none moved.
  await openSessions(page);
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await expect(page.locator('.dk-right [data-desk-chat="host"]')).toHaveCount(1);
  await page.locator(".dk-right .dkn-sback .dk-back").first().click();
  await page.locator(".dk-sessions .dk-back").click();
  await expect(items.locator("[data-desk-item]").first()).toBeVisible();
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
  expect(await listOrder(page)).toEqual(before);
  expect(await ys()).toEqual(top);
  // The lead's card picked: still none.
  await openSessions(page);
  await page.locator('.dk-sessions [data-desk-lead-chat="lead"]').click();
  await page.locator(".dk-sessions .dk-back").click();
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
});

test("the dispatcher composer stays below the conversation and hides on Decisions", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const dispatcher = page.locator('[data-desk-page="dispatcher"]');
  const chat = dispatcher.locator(".dk-chat");
  const box = dispatcher.locator(".foot:not(.held) .composer");
  // The box has the page's own foot row, under the chat and the panel both (the panel stops above it).
  await expect(dispatcher.locator(".dk-pgf > .foot:not(.held) .composer")).toHaveCount(1);
  const [panel, foot] = await Promise.all([dispatcher.locator(".dk-panel").boundingBox(), dispatcher.locator(".dk-pgf").boundingBox()]);
  if (panel) expect(panel.y + panel.height).toBeLessThanOrEqual(foot.y + 1);
  await box.locator("textarea").fill("Keep this draft");
  await dispatcher.locator('[data-desk-tab="decisions"]').click();
  await expect(box).toHaveCount(0);
  await expect(chat).toHaveCount(0);
  await dispatcher.locator('[data-desk-tab="lead"]').click();
  await expect(chat).toBeVisible();
  await expect(box.locator("textarea")).toHaveValue("Keep this draft");
  const conversation = await chat.boundingBox(), composer = await box.boundingBox();
  expect(conversation.y + conversation.height).toBeLessThanOrEqual(composer.y);
});

test("the lead's box takes pictures and has the mic, as a session's chat box does", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-right .foot:not(.held) .field");
  const key = await box.locator("textarea").getAttribute("data-text");
  await expect(box).toHaveAttribute("data-drop", key);
  await expect(box.locator("[data-mic]")).toHaveAttribute("data-mic", key);
  await expect(box.locator("[data-attach]")).toHaveAttribute("data-attach", key);
  // A picture pasted (as the paste handler adds it): shown on the box, and sent with the words.
  const png = "data:image/png;base64,iVBORw0KGgo=";
  await page.evaluate(([k, data]) => { draft(k).images.push({ name: "pasted image", mime: "image/png", data }); renderMain(); }, [key, png]);
  await expect(box.locator(".chips .chip")).toHaveCount(1);
  await box.locator("textarea").fill("This is cut off");
  await box.locator("textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "This is cut off", images: [{ name: "pasted image", mime: "image/png", data: png }] });
  await expect(page.locator(".dk-right .foot:not(.held) .composer .chips .chip")).toHaveCount(0);
});

test("working sessions show first in a Working group; a session moves when its state changes, never on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  // spare on this Desk: only its own sessions go up to Working (#189).
  await cue.change((st) => { st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [] }); st.desk.projects.find((d) => d.project === "cue").members.push("spare"); });
  const left = page.locator(".dk-sessions");
  const order = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}`
    : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}`));
  const before = ["# Working", "host", "# Idle", "spare"];
  await expect.poll(order).toEqual(before);
  // A click lights a row where it is (its chat took the list's place, #666: back to see it).
  await left.locator('[data-desk-chat-of="spare"]').click();
  await page.locator(".dk-right .dkn-sback .dk-back").first().click();
  await expect(left.locator('[data-desk-chat-of="spare"]')).toHaveClass(/\bon\b/);
  expect(await order()).toEqual(before);
  // spare starts work: it moves up to Working, after host; host stops: it leaves Working for Idle.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "spare").state = "working"; });
  await expect.poll(order).toEqual(["# Working", "host", "spare"]);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").state = "idle"; });
  await expect.poll(order).toEqual(["# Working", "spare", "# Idle", "host"]);
});

test("an idle session whose crew is at work is listed under Working, its working crew first, never below idle sessions", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").state = "idle";
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [],
      team: [{ agent_id: "m1", name: "tidy", state: "done", uncommitted: true }, { agent_id: "m2", name: "busy-one", state: "working" }] });
    st.desk.projects.find((d) => d.project === "cue").members.push("spare");
  });
  const left = page.locator(".dk-sessions");
  const order = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-sdiv") ? "—" : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}`
    : el.matches(".dk-sfold") ? null : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}`).filter(Boolean));
  await expect.poll(order).toEqual(["# Working", "spare", "  m2", "  m1", "# Idle", "host"]);
  // Its crew works, so its row says working, as the strip's meta line counts it: working means working (#402, #457).
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-st > i.idle')).toHaveCount(0);
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-st b')).toHaveText("working · 1 helper");
  await expect(left.locator(".dk-strip .dk-sub")).toHaveText("2 sessions · 1 working");
});

test("Looks right on an item whose files aren't committed commits them at once: no sheet; a toast says \"Committed <hash> · N files\"", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"]; });
  await openDesk(page);
  // Its files aren't committed: its bar's Looks right commits them (#247).
  await page.locator('.dkn-items [data-desk-item="d2"] .dkn-tt').click();
  await page.locator('.dk-right .dkn-ib [data-desk-act="looks_right"]').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".sheet")).toHaveCount(0);
  await expect(page.locator(".toast")).toContainText("Committed abc1234 · 2 files");
});

test("a done row has no Undo and no ✓: it sits under DONE TODAY with when it landed", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { Object.assign(s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"), { state: "done", done_at: Math.floor(Date.now() / 1000), commit: "abc1234" }); });
  await openDesk(page);
  await openDone(page);
  expect(await groupOf(page, "d2")).toBe("DONE TODAY");
  const row = page.locator('.dkn-items [data-desk-item="d2"]');
  await expect(statusOf(page, "d2")).toHaveText(/landed \d{1,2}(:\d\d)?(am|pm)$/);
  await row.hover();
  await expect(row.locator(".dk-undo, [data-desk-act], .dk-ok")).toHaveCount(0);
});

test("Looks right on an item whose file another session is editing: done at once, \"Done · commit waiting: <file> (<session> is editing it)\"; with no files Looks right, just done", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => {
    s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"];
    s.desk_busy = [{ path: "ui/styles/send.css", by: "tidier" }];
  });
  await openDesk(page);
  // Its files aren't committed: its bar's Looks right commits them (#247).
  await page.locator('.dkn-items [data-desk-item="d2"] .dkn-tt').click();
  await page.locator('.dk-right .dkn-ib [data-desk-act="looks_right"]').click();
  await expect(page.locator(".toast")).toContainText("Done · commit waiting: ui/styles/send.css (tidier is editing it)");

  await cue.change((s) => { const i = s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"); Object.assign(i, { state: "to_check", commit: "", commit_wait: null, files: [] }); });
  // With no files: its bar's Looks right, just done.
  await page.locator('.dk-right .dkn-ib [data-desk-act="looks_right"]').first().click();
  await expect(page.locator(".toast").last()).toContainText("Done");
  await expect(page.locator('[data-desk-item="d2"] .dk-cm')).toHaveCount(0);
});
