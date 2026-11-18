import { test, expect } from "./cue.mjs";

for (const kind of ["task", "feature", "bug"]) {
  test(`${kind} send-back stays queued through transport and changes once delivered`, async ({ cue, page }) => {
    await cue.open("desk");
    await cue.change((st, type) => { st.desk.projects[0].items.find((i) => i.id === "d2").kind = type; st.items = []; }, kind);
    await page.locator('[data-pj="toggle"]').click();
    await page.locator('.pj-pop [data-project="cue"]').click();
    await page.locator('.switch-view [data-view="desk"]').click();
    await page.locator('[data-desk-item="d2"]').click();
    const box = page.locator(".dk-right .dk-box textarea");
    await box.fill("Fix the spacing");
    await box.press("Enter");
    await cue.called("desk_item_act", { id: "d2", act: "send_back" });
    const tag = page.locator(".dk-right .dk-ttag.back").last();
    await expect(tag).toHaveText("Sent back (queued)");
    // Handed to the running agent, awaiting its delivery witness.
    await cue.change((st) => {
      const s = st.sessions.find((s) => s.session_id === "lead");
      s.queued = s.held; s.thread.push({ ...s.held, pending: true }); s.held = null;
    });
    await expect(tag).toHaveText("Sent back (queued)");
    await cue.change((st) => {
      const s = st.sessions.find((s) => s.session_id === "lead");
      s.queued = null; s.thread.at(-1).pending = false;
    });
    await expect(tag).toHaveText("Sent back");
  });
}

test("a later identical send-back doesn't relabel a previously delivered one", async ({ cue, page }) => {
  await cue.open("desk");
  const labels = await page.evaluate(() => {
    const d = state.desk.projects[0], item = d.items.find((i) => i.id === "d2"), s = sessionOf("lead"), text = "Desk item #d2 needs another check";
    const at = Date.now();
    const first = { id: "back1", at: at / 1000, delivery: { session_id: "lead", text, at_ms: at } };
    const next = { id: "back2", at: (at + 1000) / 1000, delivery: { session_id: "lead", text, at_ms: at + 1000 } };
    d.decisions.push({ ...first, item_id: item.id }, { ...next, item_id: item.id });
    s.thread = [{ role: "you", text, at_ms: at + 1, pending: false }];
    s.held = { text, at_ms: at + 1000 };
    return [deskBackLabel(first, item), deskBackLabel(next, item)];
  });
  expect(labels).toEqual(["Sent back", "Sent back (queued)"]);
});

test("an identical optimistic send-back stays visible while waiting for its saved step", async ({ cue, page }) => {
  await cue.open("desk");
  const tags = await page.evaluate(() => {
    const d = state.desk.projects[0], item = d.items.find((i) => i.id === "d2"), at = Date.now();
    d.decisions.push({ id: "old-back", item_id: item.id, by: "You", text: "Check again", why: "sent back to web-lead", at: (at - 10000) / 1000 });
    deskPending.set(item.id, [{ text: "Check again", why: "back", at }]);
    return deskThread(item, d).filter((row) => row.back).map((row) => row.tag);
  });
  expect(tags).toEqual(["Sent back", "Sent back (queued)"]);
});

test("what you write to a busy holder from its item page says Queued until it takes it", async ({ cue, page }) => {
  await cue.open("desk");
  const tags = await page.evaluate(() => {
    const d = state.desk.projects[0], item = d.items.find((i) => i.id === "d2"), at = Date.now();
    item.holder = { ...(item.holder || {}), name: "web-lead", sid: "lead" };
    const s = sessionOf("lead"), said = `About #${item.id} (${item.title}): sending again`;
    d.decisions.push({ id: "w1", item_id: item.id, by: "You", text: "sending again", why: "to web-lead", at: at / 1000 });
    const tag = () => deskThread(item, d).find((r) => r.text === undefined && r.you && r.html.includes("sending again"))?.tag;
    s.held = { text: said, at_ms: at - 5 };
    const waiting = tag();
    s.held = null; s.thread = [...(s.thread || []), { role: "you", text: said, at_ms: at + 5 }];
    return [waiting, tag()];
  });
  expect(tags).toEqual(["Queued", ""]);
});

test("what you write to a busy Desk lead from an item page says Queued until it takes it", async ({ cue, page }) => {
  await cue.open("desk");
  const tags = await page.evaluate(() => {
    const d = state.desk.projects[0], item = d.items.find((i) => i.id === "d2"), at = Date.now();
    d.lead = { ...(d.lead || {}), sid: "lead", name: "desk-lead" };
    const s = sessionOf("lead"), said = `<desk project="cue">…</desk>\n\nAbout #${item.id} (${item.title}): to see if queued`;
    d.decisions.push({ id: "w2", item_id: item.id, by: "You", text: "to see if queued", why: "to desk-lead", at: at / 1000 });
    const tag = () => deskThread(item, d).find((r) => r.you && r.html.includes("to see if queued"))?.tag;
    s.queued = { text: said, at_ms: at - 5 };
    const waiting = tag();
    s.queued = null; s.thread = [...(s.thread || []), { role: "you", text: said, at_ms: at + 5 }];
    return [waiting, tag()];
  });
  expect(tags).toEqual(["Queued", ""]);
});
