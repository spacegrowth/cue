// The Desk (ui/desk.js): a project's items on the left (Waiting on you first, then a group a holder, Done today
// folded), its Dispatch chat on the right with Decisions beside it; an item, a holder or a crew member opened on the
// right with ‹ back, its row lit where it is; the one box at the foot going to Dispatch or to the lead; and with All
// projects, every project's Desk as a card.
import { test, expect } from "./cue.mjs";

/** Project `p` picked in the header (All projects with ""). */
const pick = async (page, p) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator(p ? `.pj-pop [data-pj="pick"][data-project="${p}"]` : '.pj-pop [data-pj="pick"][data-project=""]').click();
};
/** The Desk open from the header, on project `p`. */
const openDesk = async (page, p = "cue") => {
  await pick(page, p);
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};
/** The list as it reads, top to bottom: group labels and holder heads as "# …", items by id. */
const listOrder = (page) => page.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) =>
  el.dataset.deskItem || (el.matches(".dk-holder") ? `# ${el.querySelector(".dk-nm").textContent}` : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : null)).filter(Boolean));

test("the Desk is in the header after Board, and lists Waiting on you, then a group a holder", async ({ page, cue }) => {
  await cue.open("desk");
  const views = await page.locator(".switch-view button").evaluateAll((els) => els.map((el) => el.dataset.view));
  expect(views.slice(0, 3)).toEqual(["inbox", "board", "desk"]);
  await openDesk(page);
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "d2", "# web-lead", "d4", "d3", "# host-lead", "d5", "# Not given out yet", "d6", "# Done today"]);
  await expect(page.locator(".dk-gh.you")).toContainText("2");
  await expect(page.locator('.dk-holder[data-desk-holder="lead"] .dk-s')).toHaveText("lead · crew of 2");
  await expect(page.locator('.dk-holder[data-desk-holder="host"] .dk-s')).toHaveText("on its own");
  // What each waits on, or is on now.
  await expect(page.locator('[data-desk-item="d4"] .dk-h')).toHaveText("next · after Search stays centred in the header (both change header.js)");
  await expect(page.locator('[data-desk-item="d3"] .dk-h')).toHaveText("Now · search-centre, step 2 of 3");
  await expect(page.locator('[data-desk-item="d1"] .dk-h')).toContainText("keep focus in the box");
  // Done today folded to a line; a click unfolds it.
  await expect(page.locator(".dk-folded")).toContainText("Header count as a tint · Inbox before Board");
  await page.locator(".dk-folded").click();
  await expect(page.locator('[data-desk-item="d7"]')).toBeVisible();
  // Dispatch's chat on the right.
  await expect(page.locator(".dk-tab.on")).toHaveText("Dispatch");
  await expect(page.locator(".dk-chat")).toContainText("host-lead is on it");
});

test("an item opens on the right with its row lit in place; ‹ Dispatch goes back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  const top = await page.locator('[data-desk-item="d4"]').boundingBox();
  await page.locator('[data-desk-item="d4"]').click();
  await expect(page.locator('[data-desk-item="d4"]')).toHaveClass(/\bon\b/);
  expect(await listOrder(page)).toEqual(before);
  expect((await page.locator('[data-desk-item="d4"]').boundingBox()).y).toBe(top.y);
  const pane = page.locator(".dk-right .dk-pane");
  await expect(pane.locator("h1")).toHaveText("Rail dot blinks for a reply you didn't send");
  await expect(pane.locator(".dk-state")).toContainText("Bug · next for web-lead · added by you 20m ago");
  await expect(pane.locator(".dk-chain")).toContainText("Search stays centred in the header");
  await expect(pane.locator(".dk-chain")).toContainText("This one");
  await expect(pane).toContainText("Gave it to web-lead after Search");
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Talk about this item with Dispatch…");
  // Put it before the one it waits on.
  await pane.locator('[data-desk-act="put_before"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "put_before" });
  // Back to Dispatch.
  await page.locator(".dk-back").click();
  await expect(page.locator(".dk-chat")).toBeVisible();
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
});

test("written about an open item, the box goes to Dispatch naming it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "About #d2 (Quick phrases at the right of Send): The More menu is cut off" });
  await expect(page.locator(".dk-chat")).toContainText("The More menu is cut off");
  // Dispatch has it (after the Desk's summary Cue put before it): no Sending left, and no summary shown.
  await expect(page.locator(".dk-chat .sending")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).not.toContainText("<desk>");
  await expect(page.locator(".dk-chat")).not.toContainText("3 open: 2 wait on you");
  await expect(page.locator(".dk-chat")).toContainText("Fine. And the canvas one?");
  // Only a block of the summary's shape goes: words of yours that start with <desk> stay.
  const said = await page.evaluate(() => [deskSaid('<desk project="cue">\nx\n</desk>\n\nHi'), deskSaid("<desk> is a tag"), deskSaid("<desk>\nx\n</desk> then"), deskSaid("<desk>\nx\n</desk>\n\nHi")]);
  expect(said).toEqual(["Hi", "<desk> is a tag", "<desk>\nx\n</desk> then", "<desk>\nx\n</desk>\n\nHi"]);
  // Typed so, it shows in the chat as typed.
  await page.evaluate(() => { sessionOf("disp-cue").thread.push({ at_ms: Date.now(), role: "you", text: "<desk>\nmy own\n</desk>\n\nHi" }); renderMain(); });
  await expect(page.locator(".dk-chat .cv-you").last()).toContainText("my own");
  await expect(page.locator(".dk-chat .cv-you").last()).toContainText("<desk>");
});

test("a send from an item that doesn't go keeps the item open, and your words in its box", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  await cue.override("desk_send", () => { throw "offline"; });
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await expect(page.locator(".toast")).toContainText("offline");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("Quick phrases at the right of Send");
  await expect(page.locator(".dk-box textarea")).toHaveValue("The More menu is cut off");
});

test("a lead opens with its crew; a crew member with where it is, and the box goes to the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-holder[data-desk-holder="lead"]').click();
  await expect(page.locator(".dk-holder.on")).toHaveAttribute("data-desk-holder", "lead");
  const pane = page.locator(".dk-right .dk-pane");
  await expect(pane.locator("h1")).toHaveText("web-lead");
  await expect(pane.locator(".dk-crew .dk-mb")).toHaveCount(2);
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Talk to web-lead…");
  await pane.locator('[data-desk-crew="id-search"]').click();
  await expect(pane.locator("h1")).toHaveText("search-centre");
  await expect(pane.locator(".dk-files")).toHaveText("ui/header.js · ui/styles/header.css");
  await expect(page.locator('[data-desk-item="d3"]')).toHaveClass(/\bon\b/);
  await expect(page.locator(".dk-holder.on")).toHaveAttribute("data-desk-holder", "lead");
  await expect(pane.locator('[data-mt-act="stop"]')).toBeVisible();
  await page.locator(".dk-box textarea").fill("Leave the Inbox button alone");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "About search-centre: Leave the Inbox button alone" });
  // ‹ web-lead goes back to the lead.
  await page.locator(".dk-back").click();
  await expect(pane.locator("h1")).toHaveText("web-lead");
});

test("Decisions: who decided, what and why; Undo this asks", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-tab="decisions"]').click();
  await expect(page.locator(".dk-tab.on")).toContainText("Decisions3");
  const decs = page.locator(".dk-right .dk-dec");
  await expect(decs).toHaveCount(3);
  await expect(decs.first()).toContainText("host-lead decided, 3m ago");
  await expect(decs.first()).toContainText("Hosted sessions don't read the settings file at start.");
  await decs.first().locator('[data-desk-undo="c1"]').click();
  await cue.called("desk_undo", { decisionId: "c1" });
  await expect(page.locator(".toast")).toHaveText("Asked its lead to undo it");
  await expect(decs.first()).toContainText("Undo asked");
});

test("All projects: a Desk a project; a click opens that project's", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  const cards = page.locator(".dk-desks .dk-desk");
  await expect(cards).toHaveCount(2);
  await expect(cards.first()).toHaveAttribute("data-desk-project", "cue");
  await expect(cards.first().locator(".dk-b")).toHaveText("2 waiting on you · 3 moving");
  await expect(cards.first().locator(".dk-cols")).toContainText("Done today · 3");
  await expect(cards.nth(1)).toContainText("Launch page draft");
  await cards.nth(1).click();
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
});

test("New item: kind, title, Done when; added, it opens", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator("[data-desk-new]").click();
  await page.locator('[data-desk-kind="feature"]').click();
  await page.locator('[data-text="desk-new-title"]').fill("Dark mode for the Desk");
  await page.locator('[data-text="desk-new-done"]').fill("It follows the system");
  await page.locator("[data-desk-add]").click();
  await cue.called("desk_add", { project: "cue", kind: "feature", title: "Dark mode for the Desk", doneWhen: "It follows the system" });
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("Dark mode for the Desk");
});

test("a crew member's session opens its lead's crew page on the Desk; Esc and ‹ Back to the Desk come back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const toCrewPage = async () => {
    await page.locator('.dk-holder[data-desk-holder="lead"]').click();
    await page.locator('.dk-right [data-desk-crew="id-search"]').click();
    await page.locator('.dk-right [data-desk-crew-open="id-search"]').click();
    await expect(page.locator(".crew-page")).toBeVisible();
    await expect(page.locator(".crew-page .cp-back")).toHaveText("‹ Back to the Desk");
    await expect(page.locator('.switch-view [data-view="desk"]')).toHaveClass(/\bon\b/);
  };
  await toCrewPage();
  await page.locator(".crew-page .cp-back").click();
  await expect(page.locator(".crew-page")).toHaveCount(0);
  await expect(page.locator(".dk-board")).toBeVisible();
  // Esc: the page's own steps first (the member back to its lead), then out to the Desk.
  await toCrewPage();
  await page.locator("body").press("Escape");
  await expect(page.locator(".crew-page")).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(page.locator(".crew-page")).toHaveCount(0);
  await expect(page.locator(".dk-board")).toBeVisible();
});

test("what's open on the right, and New item, close when the header picks another project", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-holder[data-desk-holder="lead"]').click();
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("web-lead");
  await page.locator("[data-desk-new]").click();
  await expect(page.locator('[data-text="desk-new-title"]')).toBeVisible();
  await pick(page, "website");
  await expect(page.locator(".dk-right .dk-pane")).toHaveCount(0);
  await expect(page.locator('[data-text="desk-new-title"]')).toHaveCount(0);
  await expect(page.locator(".dk-tab.on")).toHaveText("Dispatch");
});

test("Drop this item asks once more before it drops", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const drop = page.locator(".dk-right .dk-drop");
  await drop.click();
  await expect(drop).toHaveText("Drop it");
  expect(await cue.calls("desk_item_act")).toEqual([]);
  // Any other click lets it go.
  await page.locator(".dk-right h1").click();
  await expect(drop).toHaveText("Drop this item");
  await drop.click();
  await drop.click();
  await cue.called("desk_item_act", { id: "d6", act: "drop" });
});

test("an item no one holds yet, or waiting on nothing, reads plain: next", async ({ page, cue }) => {
  await cue.open("desk");
  const it = { kind: "bug", state: "queued", holder: null, after: null, from: "you", created: Date.now() / 1000 };
  expect(await page.evaluate((i) => deskItemState(i), it)).toMatch(/^Bug · next · added by you/);
  expect(await page.evaluate((i) => deskItemLine(i), it)).toBe("next");
  // One whose hand-off failed says so instead.
  expect(await page.evaluate((i) => deskItemLine({ ...i, route_failed: "web-lead is gone" }), it)).toBe("Couldn't give it out: web-lead is gone");
});

test("an item Dispatch couldn't give out says why; a dropped one is gone", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator('[data-desk-item="d9"]')).toHaveCount(0);
  // Nor is it counted on its project's card in All projects.
  const card = await page.evaluate(() => {
    state.desk.projects.push({ project: "zz", dispatch: null, decisions: [], items: [{ id: "z1", project: "zz", kind: "task", title: "Gone", state: "dropped", created: 1, updated: 1 }] });
    return deskAll();
  });
  expect(card).toContain("Quiet today</span>");
  expect(card).not.toContain("item open");
  await expect(page.locator('[data-desk-item="d6"] .dk-h')).toHaveText("Couldn't give it out: No session in this project could take it.");
  await page.locator('[data-desk-item="d6"]').click();
  await expect(page.locator(".dk-right .dk-pane")).toContainText("Couldn't give it out");
  await expect(page.locator(".dk-right .dk-pane")).toContainText("No session in this project could take it.");
});

test("rows open from the keyboard: Enter or Space, and the focus stays on the row", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("Rail dot blinks for a reply you didn't send");
  await expect(page.locator('[data-desk-item="d4"]')).toBeFocused();
  await page.locator('.dk-holder[data-desk-holder="lead"]').focus();
  await page.keyboard.press(" ");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("web-lead");
  await page.locator('.dk-right [data-desk-crew="id-search"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("search-centre");
});

test("Send back keeps what you wrote when it doesn't go, and clears it when it does", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const box = page.locator(".dk-box textarea");
  await box.fill("Still loses focus");
  await cue.override("desk_item_act", () => { throw "offline"; });
  await page.locator('.dk-right [data-desk-act="send_back"]').click();
  await expect(page.locator(".toast")).toContainText("offline");
  await expect(box).toHaveValue("Still loses focus");
  await cue.override("desk_item_act", () => null);
  await page.locator('.dk-right [data-desk-act="send_back"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "Still loses focus" });
  await expect(box).toHaveValue("");
});
