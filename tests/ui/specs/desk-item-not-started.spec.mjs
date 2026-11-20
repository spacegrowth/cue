// An item given out but not started yet (#476): its page's chat says so plainly and why (the item before it, or a
// free working folder), with Start now, never "Waiting on you" of its idle holder unless something really waits on
// you; once it starts, the holder's own line as before.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};

test("a queued item after another says it hasn't started and after which, not Waiting on you, with Start now", async ({ page, cue }) => {
  await cue.open("desk");
  // Its holder idle with no turns and nothing open: its own line would say "Waiting on you for your reply".
  await cue.change((st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", thread: [], since_ms: Date.now() - 60000 }); st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending")); });
  await openItem(page, "d4");
  const ichat = page.locator(".dk-right .dk-ichat"), line = ichat.locator(".dk-notyet");
  await expect(line).toHaveText(/^Hasn't started yet\. It starts after #d3 Search stays centred, which search-centre is on now\.\s*Start now$/);
  await expect(ichat).not.toContainText("Waiting on you");
  await expect(ichat.locator('[data-key="where"]')).toHaveCount(0);
  // A real ask of its holder's still shows: something does wait on you.
  await cue.change((st) => {
    st.items.push({ id: "q-476", session_id: "lead", kind: "question", status: "pending", created_ms: Date.now(), title: "Question", body: "Keep the dot red?", questions: [{ question: "Keep the dot red?", options: [] }] });
  });
  await expect(ichat.locator('[data-key="where"] .cv-what')).toContainText("Waiting on you for your answer");
  await expect(line).toHaveCount(1);
  // Start now starts it, as the panel's does.
  await line.locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "start_now" });
  // Started: the holder's own line, no "hasn't started".
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "q-476"); st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4").state = "working"; Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "working", doing: "Reading rail.js…", doing_ms: Date.now() }); });
  await expect(line).toHaveCount(0);
  await expect(ichat.locator('[data-key="where"]')).toContainText("Reading rail.js");
});

test("a queued item waiting for a free working folder says so, with Start now", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", thread: [], since_ms: Date.now() - 60000 }); st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending"));
    Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4"), { after: null, tree_wait: true, copy_users: ["d3", "d5"] });
  });
  await openItem(page, "d4");
  const ichat = page.locator(".dk-right .dk-ichat");
  await expect(ichat.locator(".dk-notyet")).toHaveText(/^Hasn't started yet\. Waiting for a free working folder \(in use by #d3, #d5\)\. It starts by itself when one is free\.\s*Start now$/);
  await expect(ichat).not.toContainText("Waiting on you");
});
