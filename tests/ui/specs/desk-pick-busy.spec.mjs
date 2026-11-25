// #667: a mocks item its holder still has (at work, or finishing) lists its options with a Pick; a pick closes it
// there and then (its options in the bar over its chat, #748), and the lead's chat shows one line: "You picked B2 on
// #d5; #… builds it".
// Light and dark, no hover text.
import { test, expect, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const items = (page) => page.evaluate(() => CueBridge.world.state.desk.projects.find((x) => x.project === "cue").items);
const BLOCK = `<desk id="cue" name="cue" project="cue">\nItems: none.\n</desk>\n\n`;

for (const scheme of ["light", "dark"]) {
  for (const state of ["working", "finishing"]) {
    test(`a pick on mocks its holder is still ${state} on closes it, with one line saying so (${scheme}) (#667)`, async ({ page, cue }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await cue.open("desk");
      await openDesk(page);
      const text = "Three on the canvas.\nPick one for working items: B1, B2 or B3.";
      await patchItem(cue, "d5", { state, title: "Mocks: working items", designs: "https://claude.ai/artifact/MOCKS", next: [{ option: "any", kind: "feature", title: "Build: working items", see: "", done_when: "", by: "new" }], said: [{ at: Date.now(), by: "host-lead", text }], ...(state === "finishing" ? { finish: { step: "commit", at: Date.now() } } : {}) });
      await page.locator('.dkn-items [data-desk-item="d5"] .dkn-tt').click();
      const head = page.locator(".dk-right .dkn-ib");
      await expect(head.locator(".dkn-ibopts .dk-optk")).toHaveText(["B1", "B2", "B3"]);
      await head.locator('.dkn-ibopts [data-desk-act="pick"]').nth(1).click();
      await cue.called("desk_item_act", { id: "d5", act: "pick", text: "B2" });
      await expect(head.locator(".dkn-ibopts")).toHaveCount(0);
      expect((await items(page)).find((i) => i.id === "d5").built_as).toBeTruthy();
      await expect(page.locator(".dk-right [title]")).toHaveCount(0);
      if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-667-${state}-${scheme}.png`, timeout: 2000 })).toPass();
    });
  }

  test(`the lead's chat shows a pick as one line (${scheme}) (#667)`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1500, height: 950 });
    await cue.open("desk");
    await openDesk(page);
    const row = `Robin picked option B2 on #d5 (Mocks: working items): "B2". #d5 is done; #d6 builds it. Cue filed #d6 Build: working items to follow it: route it now.\n\nNew item #d6 (feature): Build: working items (option B2)\n\n(Desk changes 8–9)`;
    await cue.change((st, { msgs }) => { st.sessions.find((x) => x.session_id === "lead").thread.push(...msgs); }, { msgs: [{ role: "you", text: BLOCK + row, at_ms: NOW - 20000 }] });
    const line = page.locator(".dk-chat .dk-cue");
    await expect(line).toHaveCount(1);
    await expect(line).toContainText("You picked B2 on #d5; #d6 builds it");
    await expect(line).not.toContainText("#d6 new");
    await expect(page.locator(".dk-chat [title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.locator(".dk-chat").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-667-lead-${scheme}.png` });
  });
}
