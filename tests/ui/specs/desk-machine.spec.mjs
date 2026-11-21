// Sessions and items on another machine: the machine's label beside the holder's name (on an item's row, its page and
// the Desk's sessions list; nothing for this Mac), and an item worked there says how that machine is doing in one quiet
// line: not answering for how long, what it waits for, when it last pushed.
import { test, expect, openDesk } from "./cue.mjs";

test("a holder on another machine shows the machine, and its item says how that machine is doing", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").machine = "remote-1";
    Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"), {
      machine: "remote-1",
      remote: { machine: "remote-1", folder: "~/code/cue", branch: "desk/d5", sha: "abc1234", pushed_at: Date.now() - 5 * 60000, quiet_since: Date.now() - 12 * 60000, wait: "Its work is on the machine, not pushed" },
    });
  });
  await openDesk(page);
  const row = page.locator('.dk-it[data-desk-item="d5"]');
  await expect(row.locator(".dk-who .mach")).toHaveText("remote-1");
  await expect(row.locator(".dk-rmt")).toHaveText("machine not answering for 12 min · Its work is on the machine, not pushed · last pushed 5 min ago");
  // This Mac's: no label, no line.
  const mine = page.locator('.dk-it[data-desk-item="d2"]');
  await expect(mine.locator(".mach")).toHaveCount(0);
  await expect(mine.locator(".dk-rmt")).toHaveCount(0);
  // The sessions list: the label on the session there, none on the lead's card here.
  await expect(page.locator('.dk-sessions [data-desk-chat-of="host"] .dk-s1 .mach')).toHaveText("remote-1");
  await expect(page.locator(".dk-sessions .dk-lcard .mach")).toHaveCount(0);
  // Its page: the label by its holder's name, the machine's line under it.
  await row.click();
  const right = page.locator(".dk-right");
  await expect(right.locator(".dk-pmeta .mach").first()).toHaveText("remote-1");
  await expect(right.locator(".dk-pmeta.dk-rmt")).toHaveText("machine not answering for 12 min · Its work is on the machine, not pushed · last pushed 5 min ago");
  // The machine answers again and nothing waits: only when it last pushed.
  await cue.change((st) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").remote, { quiet_since: 0, wait: "" }));
  await expect(right.locator(".dk-pmeta.dk-rmt")).toHaveText("last pushed 5 min ago");
});
