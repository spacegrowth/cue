import { test, expect, openDesk } from "./cue.mjs";

// #525: on #496's page (to check, its holder idle), "which one you think is good for users.." went back at 06:38 and
// cue-496 answered it. At 06:50 a question went too; the hub sends only a session's last few messages, so the
// send-back fell out of them, and the item's chat drew it again as a dashed "Sending… · sent back" bubble.
test("a send-back that went stays sent once its holder's chat has moved past it (#525)", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    Object.assign(s, { state: "waiting", held: null, queued: null });
    st.items = [];
  });
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  const words = "which one you think is good for users.. I cna't tell there are a lot";
  await box.fill(words);
  await box.press("Enter");
  await cue.called("desk_item_act", { id: "d2", act: "send_back" });
  const chat = page.locator(".dk-right .dk-ichat");
  const sending = chat.locator(".cv-you.msg-sending");
  // Its holder had it, and answered: back to check.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead"), it = st.desk.projects[0].items.find((i) => i.id === "d2");
    const step = st.desk.projects[0].decisions.find((c) => c.item_id === "d2" && (c.why || "").startsWith("sent back"));
    step.delivery.went = "sent";
    s.thread.push({ role: "agent", text: "I'd pick B.", at_ms: Date.now() + 1000 });
    Object.assign(s, { state: "waiting" });
    it.state = "to_check";
  });
  await expect(chat).toContainText("I'd pick B.");
  await expect(sending).toHaveCount(0);
  // A question later, answered: the session's last few messages no longer hold the send-back.
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead"), at = Date.now() + 2000;
    s.thread = [
      { role: "agent", text: "I'd pick B.", at_ms: at - 1000 },
      { role: "you", text: "A question about Desk item #d2: why only one session working?", at_ms: at },
      { role: "agent", text: "Only the refactor is at work.", at_ms: at + 1000 },
    ];
  });
  await expect(chat).toContainText("Only the refactor is at work.");
  await expect(sending).toHaveCount(0);
  await expect(chat.locator(".ml-back")).toHaveCount(0);
});

// What the hub hasn't marked as gone yet still says Sending, however the session's messages look.
test("a send-back the hub is still sending stays Sending on the item's chat", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    Object.assign(s, { state: "waiting", held: null, queued: null });
    st.items = [];
  });
  await openDesk(page);
  await cue.override("desk_item_act", (args) => {
    const d = CueBridge.world.state.desk.projects[0];
    d.decisions.unshift({ id: "slow", project: d.project, item_id: args.id, by: "You", text: args.text, why: "sent back to web-lead", at: Date.now() / 1000, delivery: { session_id: "lead", text: `Desk item #d2 isn't done: ${args.text}`, at_ms: Date.now(), went: "sending" } });
    CueBridge.emit("state", CueBridge.world.snapshot());
    return null;
  });
  await page.locator('[data-desk-item="d2"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.fill("Still on its way");
  await box.press("Enter");
  await cue.called("desk_item_act", { id: "d2", act: "send_back" });
  const chat = page.locator(".dk-right .dk-ichat");
  await expect(chat.locator(".cv-you.msg-sending", { hasText: "Still on its way" })).toHaveCount(1);
});
