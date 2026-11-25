// A message between Desks in a lead's chat (#472, #540; #640 option B): a band across the whole chat in the sending
// Desk's colour, headed by a tile and the Desk, who sent it and the item; the words; a foot saying whether the lead
// answered, and Open <Desk>. The lead's own TELL DESK is the same band, "To <Desk>", with whether that lead answered.
// One from the lead's own Desk keeps the usual "From …" bubble. Since #837 a message another session sent the lead (a
// peer) is the lead's Activity's, the band drawn there, with the lead's answer under it: the lead's chat holds only your
// messages and the lead's answers to them. A TELL DESK band stays in the chat.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

/** The band's look: tinted (not the background under it), no edge stripe, the chat's text size; in the chat, across
 *  its full width. */
async function isBand(page, b, inChat = true) {
  if (inChat) {
    const [box, inner] = await Promise.all([b.boundingBox(), page.locator(".dk-chat").evaluate((e) => { const r = e.getBoundingClientRect(); return { x: r.x + e.clientLeft, w: e.clientWidth }; })]);
    expect(Math.abs(box.x - inner.x)).toBeLessThan(2);
    expect(Math.abs(box.width - inner.w)).toBeLessThan(2);
  }
  const css = await b.evaluate((e) => { const c = getComputedStyle(e); let p = e.parentElement; while (p && getComputedStyle(p).backgroundColor === "rgba(0, 0, 0, 0)") p = p.parentElement; return [c.borderLeftWidth, c.backgroundColor, p ? getComputedStyle(p).backgroundColor : "", getComputedStyle(e.querySelector(".dk-band-t")).fontSize]; });
  expect(css[0]).toBe("0px");
  expect(css[1]).not.toBe("rgba(0, 0, 0, 0)");
  expect(css[1]).not.toBe(css[2]);
  expect(css[3]).toBe(await page.locator(inChat ? ".dk-chat" : ".dk-moves").first().evaluate((e) => getComputedStyle(e).getPropertyValue("--type-body").trim())); // the chat's text size
}

const docsSite = (st, at) => {
  const site = st.desk.projects.find((p) => p.project === "website");
  site.name = "Docs site";
  site.items = [...(site.items || []), { id: "w118", project: "website", kind: "task", title: "Fix the scraper", state: "working", created: at, updated: at, holder: { kind: "session", sid: "site", name: "site-lead" }, from: "you" }];
  return site;
};

for (const scheme of ["light", "dark"]) {
  test(`a message from another Desk's session is the band in that Desk's colour, in Activity, not the lead's chat; one from its own Desk isn't a band (${scheme}) (#472, #640, #837)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    const said = "The Docs site build copies the lead's question-card markup. Can #452 wait about 20 min for my scraper fix?";
    await cue.change((st, { said, at, docsSite }) => {
      new Function("return " + docsSite)()(st, at);
      const lead = st.sessions.find((x) => x.session_id === "lead");
      // From the other Desk's session (its id noted by the hub), then by name only, then one from the lead's own Desk.
      lead.thread.push({ role: "peer", from: "site-lead", from_sid: "site", text: said, at_ms: at });
      lead.thread.push({ role: "peer", from: "site-lead", text: "And one more, by name only.", at_ms: at + 1000 });
      lead.thread.push({ role: "peer", from: "host-lead", from_sid: "host", text: "From this Desk's own session.", at_ms: at + 2000 });
    }, { said, at: NOW - 2 * 60 * 1000, docsSite: docsSite.toString() });
    // None of them in the lead's chat (#837): it holds only your messages and the lead's answers.
    const chat = page.locator(".dk-chat");
    await expect(chat.locator(".dk-band, .peer-msg")).toHaveCount(0);
    for (const w of [said, "by name only", "own session"]) await expect(chat).not.toContainText(w);
    await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
    const moves = page.locator(".dk-moves"), band = moves.locator('.dk-band[data-band="from"]');
    await expect(band).toHaveCount(2);
    const b = moves.locator('.dk-band[data-band="from"]', { hasText: said });
    await expect(b.locator(".dk-band-desk")).toHaveText("Docs site Desk");
    await expect(b.locator(".dk-band-tile svg")).toHaveCount(1);
    await expect(b.locator(".dk-band-who")).toHaveText("site-lead");
    await expect(b.locator(".dk-band-sub")).toHaveText("#w118 Fix the scraper");
    await expect(b.locator(".dk-band-age")).toHaveText("2m");
    await expect(b.locator(".dk-band-t")).toHaveText(said);
    await expect(b.locator(".dk-band-st")).toContainText("answers it below; you don't need to.");
    await expect(b.locator(".dk-band-open")).toHaveText("Open Docs site Desk");
    await isBand(page, b, false);
    // The one from the lead's own Desk keeps the usual bubble, there too.
    await expect(moves.locator(".peer-msg")).toHaveCount(1);
    await expect(moves.locator(".peer-msg")).toContainText("From this Desk's own session.");
    await expect(page.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-peer-band-${scheme}.png` });
  });
}

for (const scheme of ["light", "dark"]) {
  test(`a message from another Desk on an older page of the lead's chat keeps its band, in Activity (${scheme}) (#513, #837)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await cue.change((st, { at, docsSite }) => { new Function("return " + docsSite)()(st, at); }, { at: NOW - 60 * 60 * 1000, docsSite: docsSite.toString() });
    // Cue's log, as the hub reads it back (db::session_log_page): the sender comes with an older message.
    await cue.override("session_log_page", ({ beforeMs }) => [
      { role: "peer", from: "site-lead", from_sid: "site", text: "An older one from the Docs site.", at_ms: beforeMs - 5000, images: [] },
      { role: "agent", text: "Noted.", at_ms: beforeMs - 4000, images: [] },
    ]);
    await openDesk(page);
    await expect(page.locator(".dk-chat")).not.toContainText("An older one from the Docs site.");
    await expect(page.locator(".dk-chat .dk-band")).toHaveCount(0);
    await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
    const band = page.locator(".dk-moves .dk-band[data-from-desk]");
    await expect(band).toHaveCount(1);
    await expect(band.locator(".dk-band-t")).toHaveText("An older one from the Docs site.");
    await expect(band.locator(".dk-band-desk")).toHaveText("Docs site Desk");
  });
}

for (const scheme of ["light", "dark"]) {
  test(`a TELL DESK from one lead to another is the band in both leads' chats, with whether it was answered (${scheme}) (#540, #640)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    const words = "#w118 is 20 min from landing. Land it first?";
    const answer = "Yes: holding #452 until yours lands.";
    await cue.change((st, { words, answer, at, docsSite }) => {
      const site = new Function("return " + docsSite)()(st, at);
      const own = st.desk.projects.find((p) => p.lead?.sid === "lead");
      site.lead = { sid: "site", name: "site-lead", state: "idle" };
      // The hub's bands (desk/tell.rs Band): Docs site's lead told this Desk's, and this lead answered.
      const asked = { from: "website", to: own.project, item: "w118", text: words, by: "site-lead", sid: "site", at, ask: true, answered: at + 60000, asked: 0, reply: false };
      const back = { from: own.project, to: "website", item: null, text: answer, by: own.lead.name, sid: "lead", at: at + 60000, ask: true, answered: 0, asked: 0, reply: true };
      site.bands = [asked, back];
      own.bands = [asked, back];
      const lead = st.sessions.find((x) => x.session_id === "lead");
      // As desk/tell.rs `band_text` writes it, after the Desk's summary block.
      const summary = `<desk id="${own.project}" name="${own.project}" project="${own.project}">\nItems: none.\n</desk>\n\n`;
      lead.thread.push({ role: "you", text: `${summary}[From another Desk: Docs site · lead site-lead · about #w118]\n${words}\n\n(To answer: TELL DESK: Docs site | <your words>.)`, at_ms: at });
      // The lead's answer: its words, then its TELL DESK line.
      lead.thread.push({ role: "agent", text: `Holding #452 for Docs site.\n\nTELL DESK: Docs site | ${answer}`, at_ms: at + 60000 });
      lead.thread.push({ role: "you", text: "[From another Desk: Old site · lead old-lead]\nA heads-up.", at_ms: at + 70000 });
    }, { words, answer, at: NOW - 3 * 60 * 1000, docsSite: docsSite.toString() });
    const chat = page.locator(".dk-chat");
    // Received: from Docs site's lead, about its item, answered by this lead.
    const got = chat.locator('.dk-band[data-band="from"]');
    await expect(got).toHaveCount(2);
    const b = got.first();
    await expect(b.locator(".dk-band-desk")).toHaveText("Docs site Desk");
    await expect(b.locator(".dk-band-who")).toHaveText("site-lead");
    await expect(b.locator(".dk-band-sub")).toHaveText("#w118 Fix the scraper");
    await expect(b.locator(".dk-band-t")).toHaveText(words);
    await expect(b.locator(".dk-band-ok")).toHaveText(/^Answered by .+ · 2m$/);
    await expect(b.locator(".dk-band-open")).toHaveText("Open Docs site Desk");
    await isBand(page, b);
    await expect(chat).not.toContainText("To answer:");
    // Sent: the lead's own TELL DESK line, "To Docs site Desk", the words, not answered yet; not as plain text.
    const sent = chat.locator('.dk-band[data-band="to"]');
    await expect(sent).toHaveCount(1);
    await expect(sent.locator(".dk-band-desk")).toHaveText("To Docs site Desk");
    await expect(sent.locator(".dk-band-t")).toHaveText(answer);
    await expect(sent.locator(".dk-band-st")).toHaveText("Not answered yet");
    await expect(sent.locator(".dk-band-open")).toHaveText("Open Docs site Desk");
    await expect(chat).not.toContainText("TELL DESK:");
    await isBand(page, sent);
    // In the Desk's own colour: the bands to and from Docs site are the same tint.
    const bg = (l) => l.evaluate((e) => getComputedStyle(e).backgroundColor);
    expect(await bg(sent)).toBe(await bg(b));
    // A Desk that isn't loaded still reads as a band, by its name.
    await expect(got.nth(1).locator(".dk-band-desk")).toHaveText("Old site Desk");
    await expect(got.nth(1).locator(".dk-band-who")).toHaveText("old-lead");
    await expect(page.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-lead-to-lead-${scheme}.png` });
    // Open Docs site Desk opens that Desk.
    await sent.locator(".dk-band-open").click();
    await expect(page.locator(".dk-chat .dk-band")).toHaveCount(0);
  });
}

// As the hub sends it (#837): a TELL DESK from another Desk's lead marked Cue's (kind "desk") starts a turn of Cue's,
// so it leaves the lead's chat with the lead's answer: both are Activity's, the band and the lead's own TELL DESK band
// drawn there in full.
test("a TELL DESK marked Cue's (kind desk) leaves the lead's chat with the lead's answer: both in Activity, as bands", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("session_log_page", () => []);
  const words = "#w118 is 20 min from landing. Land it first?", answer = "Yes: holding #452 until yours lands.";
  await cue.change((st, { words, answer, at, docsSite }) => {
    new Function("return " + docsSite)()(st, at);
    const lead = st.sessions.find((x) => x.session_id === "lead");
    lead.thread.push({ role: "you", text: `[From another Desk: Docs site · lead site-lead · about #w118]\n${words}\n\n(To answer: TELL DESK: Docs site | <your words>.)`, cue: true, kind: "desk", at_ms: at });
    lead.thread.push({ role: "agent", text: `Holding #452 for Docs site.\n\nTELL DESK: Docs site | ${answer}`, at_ms: at + 60000 });
  }, { words, answer, at: NOW - 3 * 60 * 1000, docsSite: docsSite.toString() });
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".dk-band")).toHaveCount(0);
  for (const w of [words, "Holding #452", answer]) await expect(chat).not.toContainText(w);
  await page.evaluate(() => { deskMovesShown.set("cue", 200); deskOpenActivity(); });
  const row = page.locator(".dk-moves .dk-mv", { hasText: words });
  await expect(row).toHaveCount(1);
  await expect(row.locator('.dk-band[data-band="from"] .dk-band-desk')).toHaveText("Docs site Desk");
  const ans = row.locator(".dk-ans");
  await expect(ans).toContainText("Holding #452 for Docs site.");
  await expect(ans.locator('.dk-band[data-band="to"] .dk-band-t')).toHaveText(answer);
  await expect(ans).not.toContainText("TELL DESK:");
});
