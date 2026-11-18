// Every item line shows its kind's icon before its number (#292): bug, feature and task alike, wherever the line is.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
/** The kind its icon draws, by matching its shape against the Desk's own icons. */
const kindOf = (loc) => loc.evaluate((svg) => ["task", "bug", "feature"].find((k) => {
  const t = document.createElement("div");
  t.innerHTML = deskKindIcon(k);
  return t.firstElementChild.innerHTML === svg.innerHTML;
}) ?? "");

for (const kind of ["task", "bug", "feature"]) {
  test(`a ${kind}'s lines show its icon: the lead's routing line, the hand-off card, Desk home (#292)`, async ({ page, cue }) => {
    await cue.open("desk");
    await openDesk(page);
    await page.evaluate((kind) => {
      const d = state.desk.projects.find((x) => x.project === "cue");
      for (const i of d.items) if (i.state !== "done") i.kind = kind;
      sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "Here it is.\n\nDONE: #d3" });
      sessionOf("host").thread.push({ at_ms: Date.now() - 60_000, role: "you", text: [
        "Done? Write DONE: #d5, or say what's left", "",
        `Desk item #d5 (${kind}): Hosted sessions can't open the canvas`,
        "Why you: you know the host side",
        "Say DONE: #d5 on a line of its own when it's done.",
      ].join("\n") });
      renderMain();
    }, kind);
    const word = kind[0].toUpperCase() + kind.slice(1);
    // The lead's line about it: "[icon] Task #d3 is done".
    const say = page.locator(".dk-chat .own-lines button.dk-say-id").first();
    await expect(say).toHaveText(`${word} #d3`);
    expect(await kindOf(say.locator("svg.dk-kic"))).toBe(kind);
    // The hand-off card in the session's chat.
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
    const hand = page.locator('.dk-right [data-desk-chat="host"] .dk-hand-n');
    await expect(hand).toHaveText(`${word} #d5`);
    expect(await kindOf(hand.locator("svg.dk-kic"))).toBe(kind);
    // Desk home's line under the Desk's name.
    await page.locator("[data-desk-switch]").click();
    await page.locator("[data-desk-home]").click();
    const about = page.locator(".dk-desks .dk-desk .dh-ab").first();
    await expect(about).toHaveText(new RegExp(`^(On|Next): ${word} #\\S+ .+`));
    expect(await kindOf(about.locator("svg.dk-kic"))).toBe(kind);
  });
}
