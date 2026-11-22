// Every item line shows its kind's icon before its number (#292): bug, feature and task alike, wherever the line is.
import { test, expect, openDesk } from "./cue.mjs";

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
    // The lead's line about it, and the hand-off card in the session's chat: the Desk's own (#688), in Activity.
    await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
    await expect(page.locator('.dk-right [data-desk-chat="host"] .dk-cnt')).toHaveCount(1);
    await page.evaluate(() => deskOpenActivity());
    // "[icon] Task #d3 is done".
    const say = page.locator(".dk-moves .own-lines button.mg-id").first();
    await expect(say).toHaveText(`${word} #d3`);
    expect(await kindOf(say.locator("svg.dk-kic"))).toBe(kind);
    const hand = page.locator(".dk-moves .mg-id[data-desk-hand]");
    await expect(hand).toHaveText(`${word} #d5`);
    expect(await kindOf(hand.locator("svg.dk-kic"))).toBe(kind);
    // Desk home's line under the Desk's name.
    await page.locator('.top [data-hv-menu="desks"]').click();
    await page.locator(".top [data-hv-desk-home]").click();
    const about = page.locator(".dk-desks .dk-desk .dh-ab").first();
    await expect(about).toHaveText(new RegExp(`^(On|Next): ${word} #\\S+ .+`));
    expect(await kindOf(about.locator("svg.dk-kic"))).toBe(kind);
  });
}
