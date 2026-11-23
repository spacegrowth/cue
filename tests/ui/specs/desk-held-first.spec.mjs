// Steps held for your OK (#749): a step Cue holds for your yes (Ask me before: a push to main, a release, a delete),
// whether held while you were away or asked by an item's session, sits at the very top of Waiting on you, above
// everything else, oldest first, with No and Allow right there. On today's Desk page and the new one.
import { test, expect, openDesk } from "./cue.mjs";

const T = 1_800_000_000_000;

/** Item 227's session asks a one-way step (a push to main, asked at `at`); the Desk holds two steps from while you
 *  were away, one asked before it and one after. */
async function setup(cue, page) {
  await cue.open("desk");
  await cue.change((st, T) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    Object.assign(d.items.find((item) => item.holder?.sid === "host"), { id: "227", kind: "feature", title: "Give the Desk its own icon" });
    st.items.push({ id: "perm", session_id: "host", project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "git push origin main" }, one_way: true, suggestions: [], created_ms: T, thread: [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }] });
    d.held = [{ id: "held-new", sid: "site", who: "site", what: "release", command: "scripts/release.sh 1.2.3", at: T + 5000 },
      { id: "held-old", sid: "site", who: "site", what: "delete files", command: "rm -rf build", at: T - 5000 }];
  }, T);
  await openDesk(page);
}

/** The Items list's rows, top to bottom: a held step by its id, an item by its number. */
const rows = (page) => page.locator(".dk-items [data-held-row], .dk-items [data-desk-item]").evaluateAll((els) => els.map((e) => e.dataset.heldRow || e.dataset.deskItem));

test("on today's Desk page, steps held for your OK lead Waiting on you, oldest first, with No and Allow (#749)", async ({ page, cue }) => {
  await setup(cue, page);
  await expect.poll(async () => (await rows(page)).slice(0, 3)).toEqual(["held-old", "227", "held-new"]);
  // The head says Waiting on you right above them, nothing in between.
  const first = await page.locator('.dk-items .dk2b-gh[data-desk2b-group="you"]').evaluate((h) => h.nextElementSibling?.querySelector("[data-held-row]")?.dataset.heldRow);
  expect(first).toBe("held-old");
  const held = page.locator('.dk-items [data-held-row="held-old"]');
  await expect(held.locator(".row-btns button")).toHaveText(["No", "Allow"]);
  await expect(held.locator("[title]")).toHaveCount(0);
  // Answered: gone, and the next oldest is on top.
  await held.locator('[data-allow="1"]').click();
  await cue.called("desk_held_answer", { id: "held-old", allow: true });
  await expect.poll(async () => (await rows(page)).slice(0, 2)).toEqual(["227", "held-new"]);
});

test("on the new Desk page, steps held for your OK lead Waiting on you, oldest first (#749)", async ({ page, cue }) => {
  await page.addInitScript(() => localStorage.setItem("cue.deskV2", "1"));
  await setup(cue, page);
  await expect(page.locator(".dk-board.dk-v2")).toHaveCount(1);
  await expect.poll(async () => (await rows(page)).slice(0, 3)).toEqual(["held-old", "227", "held-new"]);
  await expect(page.locator('.dk-items [data-held-row="held-old"] .row-btns button')).toHaveText(["No", "Allow"]);
});
