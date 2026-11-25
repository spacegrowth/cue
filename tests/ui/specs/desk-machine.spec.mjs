// Sessions and items on another machine: the machine's label beside the holder's name (on an item's row and
// the Desk's sessions list; nothing for this Mac), and an item worked there says how that machine is doing in one quiet
// line: not answering for how long, what it waits for, when it last pushed.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";
import { statusOf, fullOf } from "./desk.mjs";

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
  const row = page.locator('.dkn-row[data-desk-item="d5"]');
  // The label at the end of the line under the row's title, the machine's line its Now.
  await expect(row.locator(".dkn-sub .mach")).toHaveText("remote-1");
  await expect(fullOf(page, "d5")).toHaveText("Machine not answering for 12 min · Its work is on the machine, not pushed · last pushed 5 min ago");
  // This Mac's: no label, no line.
  const mine = page.locator('.dkn-row[data-desk-item="d2"]');
  await expect(mine.locator(".mach")).toHaveCount(0);
  await expect(statusOf(page, "d2")).not.toContainText("machine");
  // The sessions list (All N sessions, in the chat's place): the label on the session there, none on the lead's card.
  await deskAllSessions(page);
  await expect(page.locator('.dk-sessions [data-desk-chat-of="host"] .dk-s1 .mach')).toHaveText("remote-1");
  await expect(page.locator(".dk-sessions .dk-lcard .mach")).toHaveCount(0);
  // The machine answers again and nothing waits (its row's Now goes back to its step; when it last pushed was on the
  // old item page, lost with it).
  await cue.change((st) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").remote, { quiet_since: 0, wait: "" }));
  await expect(statusOf(page, "d5")).not.toContainText("not answering");
  // Quiet, and nothing of it was ever pushed (#598): its work is only there, and the item says so.
  await cue.change((st) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").remote, { quiet_since: Date.now() - 31 * 60000, pushed_at: 0, sha: "" }));
  await expect(fullOf(page, "d5")).toHaveText("Machine not answering for 31 min · its work is on the machine, not pushed");
  await expect(page.locator(".dkn-items [title]")).toHaveCount(0);
});

// Where an item's session starts (#597): the Desk's own setting on its settings screen.
const machines = { machine: "build-box", machines: [{ name: "build-box", label: "build-box", folder: "~/code/cue" }, { name: "gpu-1", label: "gpu-1", folder: "" }] };
const fresh = { id: "n597", project: "cue", kind: "bug", title: "Runs on the box", see: "", done_when: "", state: "new", holder: null, after: null, ask: null, files: [], created: 1, updated: 1, done_at: null, from: "you", machine: "build-box" };

test("the Desk's settings say where its new sessions start, with the folder there for a machine", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = ms; }, machines);
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const sec = dlg.locator('[data-ld-sec="machine"]');
  // On Where it works, under where an item works (#913).
  await expect(dlg.locator('[data-ld-tab="where"]')).toHaveAttribute("aria-selected", "true");
  await expect(sec.locator(".ld-label")).toHaveText(["Starts on", "At once"]);
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
test("Desk settings open on Where it works, where every machine has how many items may work there at once", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = { ...ms, machine: "", picked: [""] }; }, machines);
  await openDesk(page);
  await deskAllSessions(page);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  // On the first tab, Where it works (#913): where they start, and how many at once on each.
  await expect(dlg.locator('[data-ld-tab="where"]')).toHaveAttribute("aria-selected", "true");
  const sec = dlg.locator('[data-ld-sec="machine"]');
  await expect(sec.locator(".ld-label")).toHaveText(["Starts on", "At once"]);
  // Only This Mac ticked: every machine still has its number, This Mac's too (the Desk's Items at once, now only
  // here: no Desk-wide row under How work lands).
  await expect(sec.locator(".ld-mach-name")).toHaveText(["This Mac", "build-box", "gpu-1"]);
  const n = sec.locator(".ld-mach-n .ld-n");
  await expect(n).toHaveText(["3", "3", "3"]);
  await expect(sec.locator(".ld-mach-at")).toHaveText(["at once", "at once", "at once"]);
  await expect(dlg.locator(".ld-label", { hasText: "At once" })).toHaveCount(1);
  await expect(dlg.locator('[data-ld-sec="model"] [data-ld-model]')).toHaveCount(1);
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
test("an item waiting for room on a full machine says so on its row", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st, [ms, it]) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.machines = ms;
    d.items.push({ ...it, id: "w711", title: "Waits for the box", state: "queued", machine_wait: "build-box" });
  }, [machines, fresh]);
  await openDesk(page);
  // Its row in Up next: the line under its title.
  // Up next says in full why it waits (desk-live, 8fa85063b8).
  await expect(statusOf(page, "w711")).toHaveText("Waits for room on build-box: it already runs all the items it can");
});

// Up next has no machine on a row nobody has yet (#776): where it starts is picked as it leaves the queue, so the row
// says only why it waits. At a laptop window and at the smallest.
for (const [name, size] of [["a laptop window", { width: 1440, height: 900 }], ["the smallest window", { width: 900, height: 600 }]]) {
  test(`at ${name}, an Up next row nobody has shows why it waits and no machine`, async ({ page, cue }) => {
    await page.setViewportSize(size);
    await cue.open("desk");
    await cue.change((st, [ms, it]) => {
      const d = st.desk.projects.find((x) => x.project === "cue");
      d.machines = { ...ms, machines: [...ms.machines, { name: "", label: "This Mac", folder: "" }] };
      // As an older Cue might send it: a machine on it too. The row still shows none.
      d.items.push({ ...it, id: "w776", title: "Waits for any free place", state: "queued", room_wait: true, machine_wait: "build-box or This Mac", machine: "build-box" });
      d.items.push({ ...it, id: "n776", title: "Not given out yet", state: "new" });
    }, [machines, fresh]);
    await openDesk(page);
    const row = page.locator('.dk-it[data-desk-item="w776"]');
    await row.scrollIntoViewIfNeeded();
    await expect(row).toBeVisible();
    await expect(row).toContainText("Waits for room on build-box or This Mac");
    await expect(row.locator(".mach")).toHaveCount(0);
    await expect(page.locator('.dk-it[data-desk-item="n776"] .mach')).toHaveCount(0);
    // Its text fits its row: nothing runs past the right edge.
    const [r, box] = await Promise.all([row.boundingBox(), page.locator(".dkn-items").first().boundingBox()]);
    expect(r.x + r.width).toBeLessThanOrEqual(box.x + box.width + 1);
    if (process.env.SHOT_DIR) await row.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-up-next-${size.width}.png` });
    await expect(page.locator(".dkn-items [title], .dk-right [title]")).toHaveCount(0);
  });
}
