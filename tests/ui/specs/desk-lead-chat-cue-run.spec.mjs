// The lead chat after an install (#869): its last messages are all Cue's Desk updates and the lead's short
// acknowledgements, so its thread and the first page of Cue's log draw nothing; your own exchanges further back still show.
import { test, expect, openDesk } from "./cue.mjs";

const upd = (t) => ({ role: "you", cue: true, at_ms: t, text: `<desk project="cue" changes="only">\nOnly what changed since your last Desk block.\n</desk>` });
const ack = (t) => ({ role: "agent", at_ms: t, text: "Repeat, already handled." });

test("a lead chat whose newest messages are all Cue's still shows your exchanges from before them", async ({ page, cue }) => {
  await cue.open("desk");
  const t0 = Date.now() - 3600e3;
  // Cue's log: your exchange, then 80 Cue updates and acks after it.
  const log = [{ role: "you", at_ms: t0, text: "Why is the rail dot blinking?" }, { role: "agent", at_ms: t0 + 1000, text: "It blinks for a reply you didn't send." }];
  for (let k = 0; k < 80; k++) log.push(k % 2 ? ack(t0 + 10000 + k * 1000) : upd(t0 + 10000 + k * 1000));
  const thread = log.slice(-5);
  await cue.change((st, th) => { st.sessions.find((s) => s.session_id === "lead").thread = th; }, thread);
  await cue.override("session_log_page", ({ beforeMs, limit }, log) => log.filter((e) => e.at_ms < beforeMs).slice(-(limit || 30)), log);
  await cue.override("transcript_page", () => []);
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".cv-empty")).toHaveCount(0);
  await expect(chat).toContainText("Why is the rail dot blinking?");
});
