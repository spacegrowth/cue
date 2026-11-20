// A message from another Desk's session in the lead's chat (#472, option C): a band across the whole chat between
// hairlines, a FROM ANOTHER DESK tag, that Desk's chip, the session and its item there, the message, and a line that the
// lead answers it. One from the lead's own Desk keeps the usual "From …" bubble.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";


for (const scheme of ["light", "dark"]) {
  test(`a message from another Desk's session spans the lead's chat as a band; one from its own Desk doesn't (${scheme}) (#472)`, async ({ page, cue }) => {
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
    const chat = page.locator(".dk-chat"), band = chat.locator(".dk-xband");
    await expect(band).toHaveCount(2);
    const b = band.first();
    await expect(b.locator(".dk-xb-tag")).toHaveText("FROM ANOTHER DESK");
    await expect(b.locator(".dk-xb-chip")).toHaveText("Docs site");
    await expect(b.locator(".dk-xb-chip .pdot")).toHaveCount(1);
    await expect(b.locator(".dk-xb-chip svg")).toHaveCount(1);
    await expect(b.locator(".dk-xb-who")).toHaveText(/ · #w118$/);
    await expect(b.locator(".dk-xb-age")).toHaveText("2m ago");
    await expect(b.locator(".dk-xb-t")).toHaveText(said);
    await expect(b.locator(".dk-xb-f")).toHaveText("The lead answers it below; you don't need to.");
    // Across the whole chat, between hairlines, no edge stripe.
    const [bb, cb] = [await b.boundingBox(), await chat.boundingBox()];
    expect(Math.abs(bb.x - cb.x)).toBeLessThan(1.5);
    expect(Math.abs(bb.width - cb.width)).toBeLessThan(1.5);
    const css = await b.evaluate((e) => { const c = getComputedStyle(e); return [c.borderTopWidth, c.borderBottomWidth, c.borderLeftWidth, c.borderTopColor, c.backgroundColor]; });
    expect(css.slice(0, 3)).toEqual(["1px", "1px", "0px"]);
    expect(css[4]).not.toBe(await chat.evaluate((e) => getComputedStyle(e.closest(".dk-card, .chat-card, .card") || e).backgroundColor));
    // The one from the lead's own Desk keeps the usual bubble.
    await expect(chat.locator(".cv-peer")).toHaveCount(1);
    await expect(chat.locator(".cv-peer")).toContainText("From this Desk's own session.");
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
    const band = page.locator(".dk-chat .dk-xband");
    await expect(band).toHaveCount(1);
    await expect(band.locator(".dk-xb-t")).toHaveText("An older one from the Docs site.");
    await expect(band.locator(".dk-xb-chip")).toHaveText("Docs site");
  });
}
