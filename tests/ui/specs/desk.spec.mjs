// The Desk (ui/desk.js, ui/desk_lead.js): a project's items on the left (Waiting on you first, Not given out yet, then
// a group a holder, Done today folded), its lead's chat on the right with Decisions beside it; an item, a holder or a
// crew member opened on the right with ‹ back, its row lit where it is; the one box at the foot going to the lead;
// who runs the Desk (picked, changed, ended); Give to…; the quick-add line; and with All projects, every project's
// Desk as a card.
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
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "d2", "# Not given out yet", "d6", "# web-lead", "d4", "d3", "# host-lead", "d5", "# Done today"]);
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
  // The lead's chat on the right, named for it, its crew beside; Run by it over the items.
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  await expect(page.locator(".dk-crewline")).toHaveText("crew: search-centre, focus-fix");
  await expect(page.locator(".dk-chat")).toContainText("host-lead is on it");
  await expect(page.locator(".dk-runby")).toHaveText("Run by web-lead · Change");
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Tell web-lead: a bug, a feature, or a question…");
  await expect(page.locator("[data-desk-new]")).toHaveCount(0);
});

test("an item opens on the right with its row lit in place; ‹ web-lead goes back", async ({ page, cue }) => {
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
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Talk about this item with web-lead…");
  // Put it before the one it waits on.
  await pane.locator('[data-desk-act="put_before"]').click();
  await cue.called("desk_item_act", { id: "d4", act: "put_before" });
  // Back to the lead.
  await expect(page.locator(".dk-back")).toHaveText("‹ web-lead");
  await page.locator(".dk-back").click();
  await expect(page.locator(".dk-chat")).toBeVisible();
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
});

test("written about an open item, the box goes to the lead naming it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "About #d2 (Quick phrases at the right of Send): The More menu is cut off" });
  await expect(page.locator(".dk-chat")).toContainText("The More menu is cut off");
  // The lead has it (after the Desk's summary Cue put before it): no Sending left, and no summary shown.
  await expect(page.locator(".dk-chat .sending")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).not.toContainText("<desk>");
  await expect(page.locator(".dk-chat")).not.toContainText("3 open: 2 wait on you");
  await expect(page.locator(".dk-chat")).toContainText("Fine. And the canvas one?");
  // Only a block of the summary's shape goes: words of yours that start with <desk> stay.
  const said = await page.evaluate(() => [deskSaid('<desk project="cue">\nx\n</desk>\n\nHi'), deskSaid("<desk> is a tag"), deskSaid("<desk>\nx\n</desk> then"), deskSaid("<desk>\nx\n</desk>\n\nHi")]);
  expect(said).toEqual(["Hi", "<desk> is a tag", "<desk>\nx\n</desk> then", "<desk>\nx\n</desk>\n\nHi"]);
  // Typed so, it shows in the chat as typed.
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: "<desk>\nmy own\n</desk>\n\nHi" }); renderMain(); });
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
  await page.evaluate(() => localStorage.setItem("cue.deskKind", "task"));
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

test("what's open on the right, and Change, close when the header picks another project", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-holder[data-desk-holder="lead"]').click();
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("web-lead");
  await pick(page, "website");
  await expect(page.locator(".dk-right .dk-pane")).toHaveCount(0);
  // website has no lead yet: who runs it.
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
  await pick(page, "cue");
  await page.locator("[data-desk-change]").first().click();
  await expect(page.locator(".dk-pick")).toBeVisible();
  await pick(page, "website");
  await pick(page, "cue");
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
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

test("an item that couldn't be given out says why; a dropped one is gone", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator('[data-desk-item="d9"]')).toHaveCount(0);
  // Nor is it counted on its project's card in All projects.
  const card = await page.evaluate(() => {
    state.desk.projects.push({ project: "zz", lead: null, decisions: [], items: [{ id: "z1", project: "zz", kind: "task", title: "Gone", state: "dropped", created: 1, updated: 1 }] });
    return deskAll();
  });
  expect(card).toContain("Quiet today</span>");
  expect(card).not.toContain("item open");
  await expect(page.locator('[data-desk-item="d6"] .dk-h')).toHaveText("Couldn't give it out: No session in this project could take it. · Give to…");
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

// ---------- who runs the Desk ----------

test("no lead yet, one session: 'Let <name> run it', or start a new lead; picked, its chat opens", async ({ page, cue }) => {
  await cue.open("desk");
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  const card = page.locator(".dk-right .dk-pick");
  await expect(card.locator("h2")).toHaveText("Who runs this Desk?");
  await expect(card.locator(".btn.primary")).toHaveText("Let site-lead run it");
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new lead");
  // No box to anyone, no Run by, till someone runs it.
  await expect(page.locator(".dk-box")).toHaveCount(0);
  await expect(page.locator(".dk-runby")).toHaveCount(0);
  await card.locator(".btn.primary").click();
  await cue.called("desk_set_lead", { project: "website", sid: "site" });
  await expect(page.locator(".dk-tab.on")).toHaveText("site-lead");
  await expect(page.locator(".dk-runby")).toHaveText("Run by site-lead · Change");
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Tell site-lead: a bug, a feature, or a question…");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("no sessions in the project: one button, Start a lead; with no folder known, + New's folder step opens", async ({ page, cue }) => {
  await cue.open("desk");
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  // Its one session gone (the header's picker still has the project picked).
  await cue.change((st) => { st.sessions = st.sessions.filter((s) => s.project !== "website"); });
  const card = page.locator(".dk-right .dk-pick");
  await expect(card.locator("button")).toHaveCount(1);
  await expect(card.locator("button")).toHaveText("Start a lead");
  await card.locator("button").click();
  await cue.called("desk_set_lead", { project: "website" });
  await expect(page.locator(".toast")).toContainText("No folder known");
  expect(await page.evaluate(() => liveSpot)).toBe(true);
});

test("Start a new lead with a folder known: a '<project>-lead' runs it", async ({ page, cue }) => {
  await cue.open("desk");
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-pick .dk-fresh").click();
  await cue.called("desk_set_lead", { project: "website" });
  await expect(page.locator(".dk-tab.on")).toHaveText("website-lead");
  await expect(page.locator(".dk-chat")).toBeVisible();
});

test("Change: the project's other sessions, each with Run it; Keep or Esc goes back; Run it hands it over", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-runby [data-desk-change]").click();
  const card = page.locator(".dk-right .dk-pick");
  // web-lead runs it now: not listed, Keep web-lead instead.
  const rows = await card.locator(".dk-who").evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent]));
  expect(rows).toEqual([["host-lead", "session"]]);
  await expect(card.locator(".dk-who .btn")).toHaveText("Run it");
  await expect(card.locator(".dk-fresh")).toHaveText("Start a new lead");
  await expect(card.locator(".dk-keep")).toHaveText("Keep web-lead");
  await card.locator(".dk-keep").click();
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  await page.locator(".dk-runby [data-desk-change]").click();
  await page.locator("body").press("Escape");
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await page.locator(".dk-runby [data-desk-change]").click();
  await card.locator('.dk-who [data-desk-lead="host"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "host" });
  await expect(page.locator(".dk-runby")).toHaveText("Run by host-lead · Change");
});

test("several sessions and no lead: a row each, leads first (dot), plain ones hollow", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = null; });
  await openDesk(page);
  const who = page.locator(".dk-right .dk-who");
  expect(await who.evaluateAll((els) => els.map((el) => [el.querySelector(".dk-lt").textContent, el.querySelector(".dk-ls").textContent, el.querySelector("i").className]))).toEqual([
    ["web-lead", "lead, crew of 2", ""], ["host-lead", "session", "idle"],
  ]);
  await expect(page.locator(".dk-keep")).toHaveCount(0);
});

test("a lead that has ended: one line, Resume it or Pick another; no box", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead.state = "ended"; });
  await openDesk(page);
  const line = page.locator(".dk-right .dk-endlead");
  await expect(line.locator("p")).toHaveText("web-lead has ended. Items stay here.");
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  await line.locator('[data-desk-lead="lead"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "lead" });
  await expect(page.locator(".dk-chat")).toBeVisible();
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead.state = "ended"; });
  await page.locator(".dk-endlead [data-desk-change]").click();
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
});

// ---------- Give to… ----------

test("Give to… on a row: a menu of the project's sessions over the list (rows stay put); a pick gives it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const before = await listOrder(page);
  const below = await page.locator('.dk-holder[data-desk-holder="lead"]').boundingBox();
  await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
  const menu = page.locator('[data-desk-item="d6"] .dk-givemenu');
  await expect(menu.locator("button")).toHaveText([/web-lead\s*lead, crew of 2/, /host-lead\s*session/, "New session"]);
  expect(await listOrder(page)).toEqual(before);
  expect((await page.locator('.dk-holder[data-desk-holder="lead"]').boundingBox()).y).toBe(below.y);
  // Its row didn't open.
  await expect(page.locator(".dk-right .dk-pane")).toHaveCount(0);
  await menu.locator('[data-desk-give-to="host"]').click();
  await cue.called("desk_give", { id: "d6", sid: "host" });
  await expect(page.locator(".dk-givemenu")).toHaveCount(0);
  await expect(page.locator('[data-desk-item="d6"] [data-desk-give]')).toHaveCount(0);
});

test("Give to… in the open item; Esc closes the menu, not the item; New session starts one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  const pane = page.locator(".dk-right .dk-pane");
  await pane.locator("[data-desk-give]").click();
  await expect(pane.locator(".dk-givemenu")).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(pane.locator(".dk-givemenu")).toHaveCount(0);
  await expect(pane.locator("h1")).toHaveText("Tidy the Settings copy");
  await pane.locator("[data-desk-give]").click();
  await pane.locator('.dk-givemenu [data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d6", act: "start_now" });
});

// ---------- the quick-add line ----------

test("the quick-add line: Enter adds of the kind picked (remembered), the line keeps the cursor; no New item", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator("[data-desk-new]")).toHaveCount(0);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await expect(line).toHaveAttribute("placeholder", "Add an item…");
  await expect(page.locator(".dk-qk.on")).toHaveText("Task");
  await page.locator('[data-desk-kind="bug"]').click();
  await expect(line).toBeFocused();
  await line.fill("Dark mode for the Desk");
  await line.press("Enter");
  await cue.called("desk_add", { project: "cue", kind: "bug", title: "Dark mode for the Desk" });
  await expect(line).toHaveValue("");
  await expect(line).toBeFocused();
  await expect(page.locator(".dk-items")).toContainText("Dark mode for the Desk");
  // The kind stays picked when the Desk is drawn again.
  expect(await page.evaluate(() => deskQaKind())).toBe("bug");
  await pick(page, "website");
  await pick(page, "cue");
  await expect(page.locator(".dk-qk.on")).toHaveText("Bug");
});

test("Done when is optional: ⇧Enter or + Done when opens it, Enter adds both; Esc closes an empty one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]'), done = page.locator('.dk-qa [data-qa="done"]');
  await expect(done).toHaveCount(0);
  await line.fill("Search keeps its place");
  await line.press("Shift+Enter");
  await expect(done).toBeFocused();
  await done.press("Escape");
  await expect(done).toHaveCount(0);
  await expect(line).toHaveValue("Search keeps its place");
  await page.locator("[data-desk-qa-done]").click();
  await done.fill("It reopens where it was");
  await done.press("Enter");
  await cue.called("desk_add", { project: "cue", kind: "task", title: "Search keeps its place", doneWhen: "It reopens where it was" });
  await expect(done).toHaveCount(0);
  await expect(line).toHaveValue("");
});

test("several lines pasted: an item a line (bullets dropped); one line pastes as text", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dk-qa [data-qa="line"]').focus();
  const paste = (text) => page.evaluate((t) => {
    const el = document.querySelector('.dk-qa [data-qa="line"]'), dt = new DataTransfer();
    dt.setData("text/plain", t);
    return el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
  }, text);
  expect(await paste("- First thing\n\n2. Second thing\n* Third thing")).toBe(false);
  await expect.poll(async () => (await cue.calls("desk_add")).map((c) => c.args.title)).toEqual(["First thing", "Second thing", "Third thing"]);
  expect((await cue.calls("desk_add")).every((c) => !("doneWhen" in c.args))).toBe(true);
  expect(await paste("Just one")).toBe(true);
  expect((await cue.calls("desk_add")).length).toBe(3);
});

test("what's half-typed in the quick-add line stays on its own project's Desk", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.fill("Only for cue");
  await pick(page, "website");
  await expect(line).toHaveValue("");
  await pick(page, "cue");
  await expect(line).toHaveValue("Only for cue");
});

test("pasted lines that don't go: the first back in the line, and how many failed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_add", () => { throw "offline"; });
  await page.evaluate(() => deskQaAdd(["One", "Two", "Three"]));
  await expect(page.locator(".toast")).toContainText("3 not added: offline");
  await expect(page.locator('.dk-qa [data-qa="line"]')).toHaveValue("One");
});

test("one item with Done when that doesn't go: both come back, Done when open", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_add", () => { throw "offline"; });
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.fill("Search keeps its place");
  await line.press("Shift+Enter");
  await page.locator('.dk-qa [data-qa="done"]').fill("It reopens where it was");
  await page.locator('.dk-qa [data-qa="done"]').press("Enter");
  await expect(page.locator(".toast")).toContainText("Not added: offline");
  await expect(line).toHaveValue("Search keeps its place");
  await expect(page.locator('.dk-qa [data-qa="done"]')).toHaveValue("It reopens where it was");
});

test("Change with an item open: the open pane closes and the picker shows", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await page.locator(".dk-right .dk-drop").click();
  await page.locator(".dk-runby [data-desk-change]").click();
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
  await expect(page.locator(".dk-right .dk-pane")).toHaveCount(0);
  await expect(page.locator(".dk-it.on")).toHaveCount(0);
  expect(await page.evaluate(() => deskDropArm)).toBe(null);
});

test("a session's name is escaped in Let <name> run it", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "site").name = "<b>&site"; });
  await pick(page, "website");
  await page.locator('.switch-view [data-view="desk"]').click();
  await expect(page.locator(".dk-pick .btn.primary")).toHaveText("Let <b>&site run it");
});

test("⌘N on a project's Desk focuses the quick-add line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-tab.on").click();
  await page.keyboard.press("Meta+n");
  await expect(page.locator('.dk-qa [data-qa="line"]')).toBeFocused();
});

test("the shots: the lead's Desk, Give to…, Change, the picker", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const dir = process.env.TMPDIR || "/tmp";
  await page.screenshot({ path: `${dir}/cue-desk-lead.png` });
  await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
  await page.screenshot({ path: `${dir}/cue-desk-give.png` });
  await page.locator("body").press("Escape");
  await page.locator(".dk-runby [data-desk-change]").click();
  await page.screenshot({ path: `${dir}/cue-desk-change.png` });
  await pick(page, "website");
  await page.locator(".dk-pick").waitFor();
  await page.screenshot({ path: `${dir}/cue-desk-pick1.png` });
});
