// Sessions and items on another machine: the machine's label beside the holder's name (on an item's row, its page and
// the Desk's sessions list; nothing for this Mac), and an item worked there says how that machine is doing in one quiet
// line: not answering for how long, what it waits for, when it last pushed.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";
import { statusOf } from "./desk.mjs";

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
  // B2 (#666): the label by the row's title, the machine's line its status line.
  await expect(row.locator(".dk2b-tt .mach")).toHaveText("remote-1");
  await expect(statusOf(page, "d5")).toHaveText("Machine not answering for 12 min · Its work is on the machine, not pushed · last pushed 5 min ago");
  // This Mac's: no label, no line.
  const mine = page.locator('.dk-it[data-desk-item="d2"]');
  await expect(mine.locator(".mach")).toHaveCount(0);
  await expect(statusOf(page, "d2")).not.toContainText("machine");
  // The sessions list (All N sessions, on the right in the list's place): the label on the session there, none on
  // the lead's card here.
  await deskAllSessions(page);
  await expect(page.locator('.dk-sessions [data-desk-chat-of="host"] .dk-s1 .mach')).toHaveText("remote-1");
  await expect(page.locator(".dk-sessions .dk-lcard .mach")).toHaveCount(0);
  await page.locator(".dk-sessions .dk2b-items").click();
  // Its page: the label by its holder's name, the machine's line under it.
  await row.click();
  const right = page.locator(".dk-right");
  await expect(right.locator(".dk-pmeta .mach").first()).toHaveText("remote-1");
  await expect(right.locator(".dk-pmeta.dk-rmt")).toHaveText("machine not answering for 12 min · Its work is on the machine, not pushed · last pushed 5 min ago");
  // The machine answers again and nothing waits: only when it last pushed.
  await cue.change((st) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").remote, { quiet_since: 0, wait: "" }));
  await expect(right.locator(".dk-pmeta.dk-rmt")).toHaveText("last pushed 5 min ago");
  // Quiet, and nothing of it was ever pushed (#598): its work is only there, and the item says so.
  await cue.change((st) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").remote, { quiet_since: Date.now() - 31 * 60000, pushed_at: 0, sha: "" }));
  await expect(right.locator(".dk-pmeta.dk-rmt")).toHaveText("machine not answering for 31 min · its work is on the machine, not pushed");
  // Its row, back in the list (‹ Items N: the item opens in the list's place).
  await right.locator(".dk2b-items").click();
  await expect(statusOf(page, "d5")).toHaveText("Machine not answering for 31 min · its work is on the machine, not pushed");
  await expect(page.locator(".dk-rmt[title], .dk-rmt [title], .dk-items [title]")).toHaveCount(0);
});

// Where an item's session starts (#597): the Desk's machine on an unstarted item's card, a "Starts on" choice on its
// page (only machines the Desk has a folder on), and the Desk's own setting on its settings screen.
const machines = { machine: "build-box", machines: [{ name: "build-box", label: "build-box", folder: "~/code/cue" }, { name: "gpu-1", label: "gpu-1", folder: "" }] };
const fresh = { id: "n597", project: "cue", kind: "bug", title: "Runs on the box", see: "", done_when: "", state: "new", holder: null, after: null, ask: null, files: [], created: 1, updated: 1, done_at: null, from: "you", machine: "build-box" };

test("an item nobody has yet shows where it will start, and you can choose another place on its page", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, [ms, it]) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.machines = ms;
    d.items.push(it);
  }, [machines, fresh]);
  await openDesk(page);
  const row = page.locator('.dk-it[data-desk-item="n597"]');
  await expect(row.locator(".dk2b-tt .mach")).toHaveText("build-box");
  await row.locator(".dk2b-tt").click();
  const right = page.locator(".dk-right");
  const seg = right.locator(".dk-starts");
  await expect(seg.locator("[data-desk-start-on]")).toHaveText(["This Mac", "build-box"]);
  await expect(seg.locator('[data-desk-start-on="build-box"]')).toHaveAttribute("aria-checked", "true");
  await expect(right.locator(".dk-on-hint")).toContainText("Other machines need this repository's folder there");
  await expect(right.locator("[title]")).toHaveCount(0);
  // This Mac: chosen, and the card's machine goes.
  await seg.locator('[data-desk-start-on=""]').click();
  await expect(seg.locator('[data-desk-start-on=""]')).toHaveAttribute("aria-checked", "true");
  // Back in the list (the item opens in its place): the card's machine has gone.
  await right.locator(".dk2b-items").click();
  await expect(row.locator(".mach")).toHaveCount(0);
  // An item someone has: no choice, it stays where it is.
  await page.locator('.dk-it[data-desk-item="d5"] .dk2b-tt').click();
  await expect(right.locator(".dk-pmeta").first()).toBeVisible();
  await expect(right.locator(".dk-starts")).toHaveCount(0);
  await right.locator(".dk2b-items").click();
  await row.locator(".dk2b-tt").click();
  await expect(page.locator(".dk-right .dk-pg")).toContainText("Runs on the box");
  await expect(seg).toBeVisible();
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    if (process.env.SHOT_DIR) await right.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-starts-on-${c}.png` });
  }
});

test("the Desk's settings say where its new sessions start, with the folder there for a machine", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = ms; }, machines);
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const sec = dlg.locator('[data-ld-sec="machine"]');
  await expect(sec.locator(".ld-h")).toHaveText("Machines");
  await expect(sec.locator("[data-ld-mach]")).toHaveText(["This Mac", "build-box", "gpu-1"]);
  await expect(sec.locator('[data-ld-mach="build-box"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("~/code/cue");
  await expect(dlg.locator('[data-ld="save"]')).toBeDisabled();
  // A machine the Desk hasn't used, in build-box's place: its folder to fill in, and Save comes on.
  await sec.locator('[data-ld-mach="gpu-1"]').click();
  await sec.locator('[data-ld-mach="build-box"]').click();
  await expect(sec.locator('[data-ld-mach="build-box"]')).toHaveAttribute("aria-checked", "false");
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("");
  await sec.locator("[data-ld-folder]").fill("/srv/cue");
  await expect(dlg.locator('[data-ld="save"]')).toBeEnabled();
  // Saved, it comes back as saved: reopened, gpu-1 with its folder.
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  await expect(sec.locator('[data-ld-mach="gpu-1"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("/srv/cue");
  // A folder that isn't a full path there isn't saved.
  await sec.locator("[data-ld-folder]").fill("code/cue");
  await dlg.locator('[data-ld="save"]').click();
  await expect(page.locator(".toast")).toContainText("full path");
  await sec.locator("[data-ld-folder]").fill("/srv/cue");
  // This Mac alone: no folder. The last one ticked stays ticked.
  await sec.locator('[data-ld-mach=""]').click();
  await sec.locator('[data-ld-mach="gpu-1"]').click();
  await expect(sec.locator("[data-ld-folder]")).toHaveCount(0);
  await sec.locator('[data-ld-mach=""]').click();
  await expect(sec.locator('[data-ld-mach=""]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator(".ld-hint", { hasText: "asks you before" })).toHaveCount(1);
  await expect(dlg.locator("[title]")).toHaveCount(0);
});

// The new Desk page's Desk settings drawer (#663): it had no "Where new sessions start", so a machine picked there
// went nowhere. The same section, saved by the same Save, and still picked when the drawer opens again.
test("the new Desk page's settings drawer has where new sessions start, and a machine picked there stays picked", async ({ page, cue }) => {
  await cue.open("desk");
  await page.evaluate(() => localStorage.setItem("cue.deskV2", "1"));
  await page.reload();
  await page.locator('#app [data-view="board"]').waitFor();
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = { ...ms, machine: "" }; }, machines);
  await openDesk(page);
  await page.locator("[data-desk2-set]").click();
  const dr = page.locator(".dk2-drawer");
  const sec = dr.locator('[data-ld-sec="machine"]');
  await expect(sec.locator(".ld-h")).toHaveText("Machines");
  await expect(sec.locator('[data-ld-mach=""]')).toHaveAttribute("aria-checked", "true");
  await sec.locator('[data-ld-mach="gpu-1"]').click();
  await sec.locator("[data-ld-folder]").fill("/srv/cue");
  await dr.locator('[data-ld="save"]').click();
  await expect(dr).toHaveCount(0);
  await page.locator("[data-desk2-set]").click();
  await expect(sec.locator('[data-ld-mach="gpu-1"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("/srv/cue");
  // The model row too, as on today's dialog.
  await expect(dr.locator("[data-ld-model]")).toHaveCount(1);
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    if (process.env.SHOT_DIR) await sec.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-v2-machine-${c}.png` });
  }
  await expect(dr.locator("[title]")).toHaveCount(0);
});

// Several machines ticked (#661): the Desk's new items spread over them. Each ticked machine has its folder, and both
// are still ticked when the settings open again.
test("the Desk's settings can tick two machines, each with its folder, and both stay ticked", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = ms; }, machines);
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const sec = dlg.locator('[data-ld-sec="machine"]');
  await expect(sec.locator("[data-ld-mach]").first()).toHaveAttribute("role", "checkbox");
  await sec.locator('[data-ld-mach="gpu-1"]').click();
  await expect(sec.locator('[data-ld-mach="build-box"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator('[data-ld-mach="gpu-1"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator(".ld-folder-name")).toHaveText(["build-box", "gpu-1"]);
  await expect(sec.locator('[data-ld-folder="build-box"]')).toHaveValue("~/code/cue");
  await expect(sec.locator(".ld-hint", { hasText: "fewest" })).toHaveCount(1);
  // gpu-1 has no folder yet: Save says so and nothing is saved.
  await dlg.locator('[data-ld="save"]').click();
  await expect(page.locator(".toast")).toContainText("give the folder");
  await sec.locator('[data-ld-folder="gpu-1"]').fill("/srv/cue");
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  await expect(sec.locator('[data-ld-mach="build-box"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator('[data-ld-mach="gpu-1"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator('[data-ld-mach=""]')).toHaveAttribute("aria-checked", "false");
  await expect(sec.locator('[data-ld-folder="gpu-1"]')).toHaveValue("/srv/cue");
  await expect(dlg.locator('[data-ld="save"]')).toBeDisabled();
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    if (process.env.SHOT_DIR) await sec.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-two-machines-${c}.png` });
  }
  await expect(dlg.locator("[title]")).toHaveCount(0);
});

// How many items at once on each machine (#711): a − n + on each ticked machine's row, 1 to 9, saved with the rest and
// there when the settings open again.
test("Desk settings open on Machines, where every machine has how many items may work there at once", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = { ...ms, machine: "", picked: [""] }; }, machines);
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  // First on the screen, named Machines.
  await expect(dlg.locator(".ld-h").first()).toHaveText("Machines");
  const sec = dlg.locator('[data-ld-sec="machine"]');
  await expect(sec.locator(".ld-sub")).toContainText("how many of its items each machine works on at once");
  // Only This Mac ticked: every machine still has its number, This Mac's too (the Desk's Items at once, now only
  // here: no Desk-wide row under How work lands).
  await expect(sec.locator(".ld-mach-name")).toHaveText(["This Mac", "build-box", "gpu-1"]);
  const n = sec.locator(".ld-mach-n .ld-n");
  await expect(n).toHaveText(["3", "3", "3"]);
  await expect(sec.locator(".ld-mach-at")).toHaveText(["at once", "at once", "at once"]);
  await expect(dlg.locator(".ld-row", { hasText: "Items at once" })).toHaveCount(0);
  await expect(dlg.locator('[data-ld-sec="lands"] [data-ld-model]')).toHaveCount(1);
  await expect(sec.locator(".ld-hint", { hasText: "The most of this Desk's items working on each machine together" })).toHaveCount(1);
  for (let k = 0; k < 2; k++) await sec.locator('[data-ld-mach-n="build-box"][data-ld-by="-1"]').click();
  await expect(n).toHaveText(["3", "1", "3"]);
  await expect(sec.locator('[data-ld-mach-n="build-box"][data-ld-by="-1"]')).toBeDisabled();
  await expect(dlg.locator('[data-ld="save"]')).toBeEnabled();
  // Two places ticked: the hint says more go to the other one.
  await sec.locator('[data-ld-mach="build-box"]').click();
  await expect(sec.locator(".ld-hint", { hasText: "go to another place you ticked" })).toHaveCount(1);
  await sec.locator('[data-ld-mach-n="gpu-1"][data-ld-by="1"]').click();
  await sec.locator('[aria-label="More at once on This Mac"]').click();
  await expect(n).toHaveText(["4", "1", "4"]);
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  await expect(n).toHaveText(["4", "1", "4"]);
  await expect(dlg.locator('[data-ld="save"]')).toBeDisabled();
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    if (process.env.SHOT_DIR) await sec.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-machine-at-once-${c}.png` });
  }
  await expect(dlg.locator("[title]")).toHaveCount(0);
});

// An item waiting for room on its machine (#711): its row and page say so.
test("an item waiting for room on a full machine says so on its row and page", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, [ms, it]) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.machines = ms;
    d.items.push({ ...it, id: "w711", title: "Waits for the box", state: "queued", machine_wait: "build-box" });
  }, [machines, fresh]);
  await openDesk(page);
  const row = page.locator('.dk-it[data-desk-item="w711"]');
  // B2's row (#666): its status line under the title.
  await expect(row.locator(".dk2b-ts")).toHaveText("Waits for room on build-box");
  await row.click();
  await expect(page.locator(".dk-right")).toContainText("Starts when build-box has room");
});
