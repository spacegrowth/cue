// A mocks item waiting for you to check lists its options, A, B… or M1, M2…, each with what it is and a Pick (#445).
// A pick closes the mocks item and makes its build item for the lead to route; its chat is for comments on the mocks.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const items = (page) => page.evaluate(() => CueBridge.world.state.desk.projects.find((x) => x.project === "cue").items);

test("a mocks-only item lists its options M1, M2 with a Pick each; a pick closes it and makes its build item (#445)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Mocks only (#408, #436): no DESIGNS: line, the canvas is the link its DONE made.
  const text = "Two ways on the canvas: https://claude.ai/artifact/MOCKS\n- **M1**: a strip under the header\n- **M2** — a card in the list\nPick one: M1 or M2.";
  await patchItem(cue, "d5", { state: "to_check", title: "Mocks: working items", designs: "", made: ["https://claude.ai/artifact/MOCKS"], finish: { step: "checked", at: Date.now(), mocks: true }, said: [{ at: Date.now(), by: "host-lead", text }] });
  await page.locator('[data-desk-item="d5"] .dk-t').click();
  const head = page.locator(".dk-iside .dk-pnow");
  const opts = head.locator(".dk-opts li");
  await expect(opts).toHaveCount(2);
  await expect(opts.locator(".dk-optk")).toHaveText(["M1", "M2"]);
  await expect(opts.locator(".dk-optw")).toHaveText(["a strip under the header", "a card in the list"]);
  await expect(opts.locator('[data-desk-act="pick"]')).toHaveText(["Pick", "Pick"]);
  // Its line says where the mocks are and what to pick, not the whole reply.
  await expect(head.locator(".dk-pickl")).toHaveText("Mocks · open the canvas · Pick one: M1 or M2.");
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  // Its chat is for comments on the mocks; the pick is the button.
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", /^Comments on the mocks/);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    // The theme redraws the page: again once it's settled.
    if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-iside").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-445-${theme}.png`, timeout: 2000 })).toPass();
  }
  await opts.nth(1).locator('[data-desk-act="pick"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "pick", text: "M2" });
  await expect(head.locator(".dk-pword")).toHaveText("Done");
  await expect(head.locator(".dk-opts")).toHaveCount(0);
  const n = (await items(page)).find((i) => i.id === "d5").built_as;
  await expect(head.locator(".dk-built")).toHaveText(`Picked M2 → being built as #${n}`);
  expect((await items(page)).find((i) => i.id === n).title).toBe("Build: working items (option M2)");
});

test("options listed one to a line are picked from even with no pick line; one option alone isn't a list (#445)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d5", { state: "to_check", designs: "https://claude.ai/artifact/MOCKS", said: [{ at: Date.now(), by: "host-lead", text: "On the canvas:\nA: rows\nB) cards\nC — a table" }] });
  await page.locator('[data-desk-item="d5"] .dk-t').click();
  const opts = page.locator(".dk-iside .dk-pnow .dk-opts li");
  await expect(opts.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
  await expect(opts.locator(".dk-optw")).toHaveText(["rows", "cards", "a table"]);
  await patchItem(cue, "d5", { said: [{ at: Date.now(), by: "host-lead", text: "A: the one way, on the canvas." }] });
  await expect(page.locator(".dk-iside .dk-pnow .dk-opts")).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-box textarea")).not.toHaveAttribute("placeholder", /^Comments on the mocks/);
});
