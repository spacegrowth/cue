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
  // Quiet, and nothing of it was ever pushed (#598): its work is only there, and the item says so.
  await cue.change((st) => Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").remote, { quiet_since: Date.now() - 31 * 60000, pushed_at: 0, sha: "" }));
  await expect(right.locator(".dk-pmeta.dk-rmt")).toHaveText("machine not answering for 31 min · its work is on the machine, not pushed");
  await expect(row.locator(".dk-rmt")).toHaveText("machine not answering for 31 min · its work is on the machine, not pushed");
  await expect(page.locator(".dk-rmt[title], .dk-rmt [title]")).toHaveCount(0);
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
  await expect(row.locator(".dk-who .mach")).toHaveText("build-box");
  await row.locator(".dk-t").click();
  const right = page.locator(".dk-right");
  const seg = right.locator(".dk-starts");
  await expect(seg.locator("[data-desk-start-on]")).toHaveText(["This Mac", "build-box"]);
  await expect(seg.locator('[data-desk-start-on="build-box"]')).toHaveAttribute("aria-checked", "true");
  await expect(right.locator(".dk-on-hint")).toContainText("Other machines need this repository's folder there");
  await expect(right.locator("[title]")).toHaveCount(0);
  // This Mac: chosen, and the card's machine goes.
  await seg.locator('[data-desk-start-on=""]').click();
  await expect(seg.locator('[data-desk-start-on=""]')).toHaveAttribute("aria-checked", "true");
  await expect(row.locator(".mach")).toHaveCount(0);
  // An item someone has: no choice, it stays where it is.
  await page.locator('.dk-it[data-desk-item="d5"] .dk-t').click();
  await expect(right.locator(".dk-pmeta").first()).toBeVisible();
  await expect(right.locator(".dk-starts")).toHaveCount(0);
  await row.locator(".dk-t").click();
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
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  const dlg = page.locator(".ld-dialog");
  const sec = dlg.locator('[data-ld-sec="machine"]');
  await expect(sec.locator(".ld-h")).toHaveText("Where new sessions start");
  await expect(sec.locator("[data-ld-mach]")).toHaveText(["This Mac", "build-box", "gpu-1"]);
  await expect(sec.locator('[data-ld-mach="build-box"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("~/code/cue");
  await expect(dlg.locator('[data-ld="save"]')).toBeDisabled();
  // A machine the Desk hasn't used: its folder to fill in, and Save comes on.
  await sec.locator('[data-ld-mach="gpu-1"]').click();
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("");
  await sec.locator("[data-ld-folder]").fill("/srv/cue");
  await expect(dlg.locator('[data-ld="save"]')).toBeEnabled();
  // Saved, it comes back as saved: reopened, gpu-1 with its folder.
  await dlg.locator('[data-ld="save"]').click();
  await expect(dlg).toHaveCount(0);
  await page.locator(".dk-strip .dk-ab [data-lands-change]").click();
  await expect(sec.locator('[data-ld-mach="gpu-1"]')).toHaveAttribute("aria-checked", "true");
  await expect(sec.locator("[data-ld-folder]")).toHaveValue("/srv/cue");
  // A folder that isn't a full path there isn't saved.
  await sec.locator("[data-ld-folder]").fill("code/cue");
  await dlg.locator('[data-ld="save"]').click();
  await expect(page.locator(".toast")).toContainText("full path");
  await sec.locator("[data-ld-folder]").fill("/srv/cue");
  // This Mac: no folder.
  await sec.locator('[data-ld-mach=""]').click();
  await expect(sec.locator("[data-ld-folder]")).toHaveCount(0);
  await expect(sec.locator(".ld-hint")).toContainText("asks you before");
  await expect(dlg.locator("[title]")).toHaveCount(0);
});

// The new Desk page's Desk settings drawer (#663): it had no "Where new sessions start", so a machine picked there
// went nowhere. The same section, saved by the same Save, and still picked when the drawer opens again.
test("the new Desk page's settings drawer has where new sessions start, and a machine picked there stays picked", async ({ page, cue }) => {
  await cue.open("desk");
  await page.evaluate(() => localStorage.setItem("cue.deskV2", "1"));
  await page.reload();
  await cue.change((st, ms) => { st.desk.projects.find((x) => x.project === "cue").machines = { ...ms, machine: "" }; }, machines);
  await openDesk(page);
  await page.locator("[data-desk2-set]").click();
  const dr = page.locator(".dk2-drawer");
  const sec = dr.locator('[data-ld-sec="machine"]');
  await expect(sec.locator(".ld-h")).toHaveText("Where new sessions start");
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
