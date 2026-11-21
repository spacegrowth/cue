// A message from another Desk's session in the lead's chat (#472; #501: the message family's card): that Desk's chip,
// the session, a FROM ANOTHER DESK tag, its item there, the age, the message, and a line that the lead answers it. One
// from the lead's own Desk keeps the usual "From …" bubble.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";


for (const scheme of ["light", "dark"]) {
  test(`a message from another Desk's session is a card with that Desk's chip; one from its own Desk doesn't (${scheme}) (#472)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    const said = "The Docs site build copies the lead's question-card markup. Can #452 wait about 20 min for my scraper fix?";
    await cue.change((st, { said, at }) => {
      const site = st.desk.projects.find((p) => p.project === "website");
      site.name = "Docs site";
      site.items = [...(site.items || []), { id: "w118", project: "website", kind: "task", title: "Fix the scraper", state: "working", created: at, updated: at, holder: { kind: "session", sid: "site", name: "site-lead" }, from: "you" }];
      const lead = st.sessions.find((x) => x.session_id === "lead");
      // From the other Desk's session (its id noted by the hub), then by name only, then one from the lead's own Desk.
      lead.thread.push({ role: "peer", from: "site-lead", from_sid: "site", text: said, at_ms: at });
      lead.thread.push({ role: "peer", from: "site-lead", text: "And one more, by name only.", at_ms: at + 1000 });
      lead.thread.push({ role: "peer", from: "host-lead", from_sid: "host", text: "From this Desk's own session.", at_ms: at + 2000 });
    }, { said, at: NOW - 2 * 60 * 1000 });
    const chat = page.locator(".dk-chat"), band = chat.locator(".mg[data-from-desk]");
    await expect(band).toHaveCount(2);
    const b = band.first();
    await expect(b.locator(".mg-tag")).toHaveText("FROM ANOTHER DESK");
    await expect(b.locator(".mg-chip")).toHaveText("Docs site");
    await expect(b.locator(".mg-chip .pdot")).toHaveCount(1);
    await expect(b.locator(".mg-chip svg")).toHaveCount(1);
    await expect(b.locator(".mg-who")).toHaveText("site-lead");
    await expect(b.locator(".mg-id")).toHaveText("#w118");
    await expect(b.locator(".mg-age")).toHaveText("2m");
    await expect(b.locator(".mg-t")).toHaveText(said);
    await expect(b.locator(".mg-f")).toHaveText("The lead answers it below; you don't need to.");
    // The family's card: filled, no edge stripe, its message at the family's body size.
    const css = await b.evaluate((e) => { const c = getComputedStyle(e); return [c.borderLeftWidth, c.backgroundColor, getComputedStyle(e.querySelector(".mg-t")).fontSize]; });
    expect(css[0]).toBe("0px");
    expect(css[1]).not.toBe(await chat.evaluate((e) => getComputedStyle(e.closest(".dk-card, .chat-card, .card") || e).backgroundColor));
    expect(css[2]).toBe("15px"); // the chat's text size at Default (#594)
    // The one from the lead's own Desk keeps the usual bubble.
    await expect(chat.locator(".peer-msg")).toHaveCount(1);
    await expect(chat.locator(".peer-msg")).toContainText("From this Desk's own session.");
    await expect(page.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-peer-band-${scheme}.png` });
  });
}

for (const scheme of ["light", "dark"]) {
  test(`a message from another Desk on an older page of the lead's chat keeps its band (${scheme}) (#513)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await cue.change((st, at) => {
      const site = st.desk.projects.find((p) => p.project === "website");
      site.name = "Docs site";
      site.items = [...(site.items || []), { id: "w118", project: "website", kind: "task", title: "Fix the scraper", state: "working", created: at, updated: at, holder: { kind: "session", sid: "site", name: "site-lead" }, from: "you" }];
    }, NOW - 60 * 60 * 1000);
    // Cue's log, as the hub reads it back (db::session_log_page): the sender comes with an older message.
    await cue.override("session_log_page", ({ beforeMs }) => [
      { role: "peer", from: "site-lead", from_sid: "site", text: "An older one from the Docs site.", at_ms: beforeMs - 5000, images: [] },
      { role: "agent", text: "Noted.", at_ms: beforeMs - 4000, images: [] },
    ]);
    await openDesk(page);
    const band = page.locator(".dk-chat .mg[data-from-desk]");
    await expect(band).toHaveCount(1);
    await expect(band.locator(".mg-t")).toHaveText("An older one from the Docs site.");
    await expect(band.locator(".mg-chip")).toHaveText("Docs site");
  });
}

test("a TELL DESK from another Desk's lead, delivered by Cue, is the same band with its lead as the sender (#540)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1500, height: 950 });
  await cue.open("desk");
  await openDesk(page);
  const words = "#w118 is 20 min from landing. Land it first?";
  await cue.change((st, { words, at }) => {
    const site = st.desk.projects.find((p) => p.project === "website");
    site.name = "Docs site";
    const lead = st.sessions.find((x) => x.session_id === "lead");
    // As desk/tell.rs `band_text` writes it, after the Desk's summary block.
    const summary = `<desk id="cue" name="cue" project="cue">\nItems: none.\n</desk>\n\n`;
    lead.thread.push({ role: "you", text: `${summary}[From another Desk: Docs site · lead site-lead · about #w118]\n${words}\n\n(To answer: TELL DESK: Docs site | <your words>.)`, at_ms: at });
    lead.thread.push({ role: "you", text: "[From another Desk: Gone desk · lead old-lead]\nA heads-up.", at_ms: at + 1000 });
  }, { words, at: NOW - 2 * 60 * 1000 });
  const band = page.locator(".dk-chat .mg[data-from-desk]");
  await expect(band).toHaveCount(2);
  const b = band.first();
  await expect(b.locator(".mg-tag")).toHaveText("FROM ANOTHER DESK");
  await expect(b.locator(".mg-chip")).toHaveText("Docs site");
  await expect(b.locator(".mg-chip .pdot")).toHaveCount(1);
  await expect(b.locator(".mg-id")).toHaveText("#w118");
  await expect(b.locator(".mg-t")).toHaveText(words);
  await expect(b.locator(".mg-f")).toHaveText("The lead answers it below; you don't need to.");
  await expect(band.nth(1).locator(".mg-chip")).toHaveText("Gone desk");
  await expect(band.nth(1).locator(".mg-who")).toHaveText("old-lead");
  await expect(page.locator(".dk-chat")).not.toContainText("To answer:");
  await expect(page.locator("[title]")).toHaveCount(0);
});
