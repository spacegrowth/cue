import { test, expect, openDesk } from "./cue.mjs";

for (const kind of ["task", "feature", "bug"]) {
  test(`${kind} send-back says where it is, in the one line every message has, till it's delivered`, async ({ cue, page }) => {
    await cue.open("desk");
    await cue.change((st, type) => { st.desk.projects[0].items.find((i) => i.id === "d2").kind = type; st.items = []; }, kind);
    await openDesk(page);
    await page.locator('[data-desk-item="d2"]').click();
    const box = page.locator(".dk-right .dkn-spm textarea");
    await box.fill("Fix the spacing");
    await box.press("Enter");
    await cue.called("desk_item_act", { id: "d2", act: "send_back" });
    // The item opens its holder's chat (#418; C, #748): your send-back shows in it once, as the message to its busy holder
    // waiting there, not sent yet, its line the same as any message's (#422) (not also as a line of its own): your words
    // alone, and that it goes in next once its holder is done (#419).
    const chat = page.locator('.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="lead"]');
    const msg = chat.locator(".cv-you", { hasText: "Fix the spacing" }).last();
    const line = msg.locator(".msg-line");
    await expect(line).toHaveClass(/ml-held/);
    // The board's words (#688), and the item's head over your words.
    await expect(line).toContainText("Not sent yet · Cue sends it when its turn ends · Edit · Send now · Cancel");
    await expect(msg.locator(".cv-you-text")).toHaveText(/Fix the spacing$/);
    await expect(msg.locator(".dk-about")).toContainText("#d2");
    await expect(chat.locator(".cv-you.msg-sending")).toHaveCount(0);
    // Handed to the running agent, awaiting its delivery witness: still queued.
    await cue.change((st) => {
      const s = st.sessions.find((s) => s.session_id === "lead");
      s.queued = s.held; s.thread.push({ ...s.held, pending: true }); s.held = null;
    });
    await expect(line).toHaveClass(/ml-queued/);
    await expect(line).toContainText(/Queued for .+ · Send now/);
    await cue.change((st) => {
      const s = st.sessions.find((s) => s.session_id === "lead");
      s.queued = null; s.thread.at(-1).pending = false;
    });
    // Delivered: still your own bubble with the item's head (#688, no longer Cue's card), its line saying so,
    // nothing queued about it.
    const sent = chat.locator(".cv-you.mg-you", { hasText: "Fix the spacing" }).last();
    await expect(sent.locator(".dk-about")).toContainText("#d2");
    await expect(chat.locator(".cue-msg", { hasText: "Fix the spacing" })).toHaveCount(0);
    const went = sent.locator(".msg-line");
    await expect(went).toHaveClass(/ml-delivered/);
    await expect(went).toHaveText(/^You · .+ ago$/);
    await expect(went).not.toContainText(/queued|not sent/i);
    await expect(chat.locator(".cv-you.msg-sending")).toHaveCount(0);
  });
}

