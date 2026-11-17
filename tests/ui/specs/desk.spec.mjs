// The Desk (ui/desk.js, ui/desk_lead.js): the project's sessions on the left, its lead's chat in the middle with
// Decisions beside it, its items on the right (Waiting on you first, In progress, Queued, Done
// today folded); an item, a holder or a
// crew member opened in the middle with ‹ back, its row lit where it is; the one box at the foot going to the lead;
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
const listOrder = (page) => page.locator(".dk-items .dk-scroll > *").evaluateAll((els) => els.map((el) =>
  el.dataset.deskItem || (el.matches(".dk-holder") ? `# ${el.querySelector(".dk-nm").textContent}` : el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : null)).filter(Boolean));

test("the Desk is in the header after Board, and lists Waiting on you, then a group a holder", async ({ page, cue }) => {
  await cue.open("desk");
  const views = await page.locator(".switch-view button").evaluateAll((els) => els.map((el) => el.dataset.view));
  expect(views.slice(0, 3)).toEqual(["inbox", "board", "desk"]);
  await openDesk(page);
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "d2", "# In progress", "d3", "d5", "# Queued", "d4", "d6", "# Done", "d7", "d8", "d9"]);
  await expect(page.locator(".dk-items .dk-gh.you")).toContainText("2");
  // Grouped by state only: no head a session (the sessions are on the left).
  await expect(page.locator(".dk-items .dk-holder")).toHaveCount(0);
  // One row each: state, #id, title, who holds it, age; only the ask has a second line, its question.
  const cells = (id) => page.locator(`[data-desk-item="${id}"]`).evaluate((el) => [".dk-tag", ".dk-id", ".dk-t", ".dk-who"].map((c) => el.querySelector(c)?.textContent));
  expect(await cells("d4")).toEqual(["", "Bug #d4", "Rail dot blinks for a reply you didn't send", "web-lead"]);
  expect(await cells("d3")).toEqual(["", "Feature #d3", "Search stays centred in the header", "web-lead → search-centre"]);
  expect(await cells("d2")).toEqual(["", "Feature #d2", "Quick phrases at the right of Send", "web-lead"]);
  await expect(page.locator(".dk-items .dk-h")).toHaveCount(1);
  await expect(page.locator('[data-desk-item="d1"] .dk-h')).toContainText("keep focus in the box");
  // Ready to check: Looks right on its row. Done today: each with the time it finished.
  await expect(page.locator(".dk-items .dk-ok")).toHaveCount(1);
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toHaveAttribute("aria-label", "Looks right");
  await expect(page.locator('[data-desk-item="d7"] .dk-age')).toHaveText(/^\d{1,2}(:\d\d)?(am|pm)$/);
  // The lead's chat on the right, named for it, no crew beside it (a Desk's lead has none).
  await expect(page.locator(".dk-tab.on")).toHaveText("web-lead");
  await expect(page.locator(".dk-tabs")).not.toContainText("crew");
  // Its mark (pick J): a tinted badge with the kind's icon before the name, the word "Desk lead" over the name.
  const mark = page.locator('.dk-tabs [data-desk-mark="desk"]');
  await expect(mark).toHaveText("Desk lead");
  expect(await mark.evaluate((e) => !!e.nextElementSibling?.querySelector(".dk-tab.on"))).toBe(true);
  const badge = page.locator(".dk-tabs .dk-titled > .dk-badge");
  await expect(badge.locator("svg.dk-kic")).toHaveCount(1);
  expect(await badge.evaluate((e) => getComputedStyle(e).backgroundColor)).not.toBe("rgba(0, 0, 0, 0)");
  await expect(page.locator(".dk-chat")).toContainText("host-lead is on it");
  // The Sessions head: ‹ to every Desk, the Desk's name, ✎ only on hover; no "Change" anywhere.
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  await expect(page.locator(".dk-sh .dk-ren")).toHaveCSS("opacity", "0");
  await page.locator(".dk-sh").hover();
  await expect(page.locator(".dk-sh .dk-ren")).toHaveCSS("opacity", "1");
  await expect(page.locator(".dk-board")).not.toContainText("Change");
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
  await expect(page.locator('.dk-right [data-desk-mark]')).toHaveText("Bug");   // a chat for an item: its kind
  await expect(pane.locator(".dk-state")).toContainText("#d4 · next for web-lead · added by you 20m ago");
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

test("written about an item at work, the box goes to the lead naming it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "About #d3 (Search stays centred in the header): The More menu is cut off" });
  // The item stays open, its box empty; the lead's chat has it when you go back.
  await expect(page.locator(".toast").last()).toContainText("Sent to web-lead");
  await expect(page.locator(".dk-pane h1")).toHaveText("Search stays centred in the header");
  await expect(page.locator(".dk-box textarea")).toHaveValue("");
  await page.locator(".dk-back").click();
  await expect(page.locator(".dk-chat")).toContainText("The More menu is cut off");
  // The lead has it (after the Desk's summary Cue put before it): no Sending left, and no summary shown.
  await expect(page.locator(".dk-chat .sending")).toHaveCount(0);
  await expect(page.locator(".dk-chat")).not.toContainText("<desk>");
  await expect(page.locator(".dk-chat")).not.toContainText("3 open: 2 wait on you");
  await expect(page.locator(".dk-chat")).toContainText("Fine. And the canvas one?");
  // Only a block of the summary's shape goes: words of yours that start with <desk> stay.
  const said = await page.evaluate(() => [deskSaid('<desk project="cue">\nx\n</desk>\n\nHi'), deskSaid("<desk> is a tag"), deskSaid("<desk>\nx\n</desk> then"), deskSaid("<desk>\nx\n</desk>\n\nHi"),
    // Only a picture sent: nothing after the block (the blank line too trimmed off), and none of it shows.
    deskSaid('<desk project="cue">\nx\n</desk>'), deskSaid('<desk project="cue">\nx\n</desk>\n\n'), deskSaid('<desk project="cue">\nx\n</desk> then')]);
  expect(said).toEqual(["Hi", "<desk> is a tag", "<desk>\nx\n</desk> then", "<desk>\nx\n</desk>\n\nHi", "", "", '<desk project="cue">\nx\n</desk> then']);
  // A picture on its own: just the picture in the chat, none of the summary.
  await page.route(/\/tmp\/a\.png$/, (r) => r.fulfill({ status: 200, contentType: "image/png", body: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64") }));
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\nItems:\n#9 task: a long line\n</desk>', images: ["/tmp/a.png"] }); renderMain(); });
  await expect(page.locator(".dk-chat .cv-you").last()).not.toContainText("Items:");
  await expect(page.locator(".dk-chat .cv-you").last().locator(".thumb img")).toHaveCount(1);
  // Typed so, it shows in the chat as typed.
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: "<desk>\nmy own\n</desk>\n\nHi" }); renderMain(); });
  await expect(page.locator(".dk-chat .cv-you").last()).toContainText("my own");
  await expect(page.locator(".dk-chat .cv-you").last()).toContainText("<desk>");
});

test("the lead at work shows it in its chat: the working line and its steps, live, as any session's chat", async ({ page, cue }) => {
  await cue.override("session_steps", ({ sessionId }, at) => (sessionId !== "lead" ? { version: 1, turns: [] } : { version: 1, turns: [{ at_ms: at - 30000, prompt: "", more: 0, items: [
    { t: "step", id: "lr1", kind: "read", verb: "Read", subject: "header.js", mono: true, result: "40 lines", bad: false, done: true, at_ms: at - 20000 },
  ] }] }), await page.evaluate(() => now()));
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead");
    Object.assign(s, { state: "working", doing: "Reading header.js…", doing_ms: Date.now() - 5000, since_ms: Date.now() - 30000 });
  });
  const live = page.locator(".dk-chat .cv-status");
  await expect(live).toContainText("Reading header.js");
  await expect(live.locator(".dot-live")).toHaveCount(1);
  // Its steps come in while it works (followSteps follows the Desk's lead).
  await expect(page.locator(".dk-chat .stx-line", { hasText: "header.js" })).toBeVisible();
  // Done: the working line goes.
  await cue.change((st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "idle", doing: "" }); });
  await expect(page.locator(".dk-chat .dot-live")).toHaveCount(0);
});

test("a send from an item that doesn't go keeps the item open, and your words in its box", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  await cue.override("desk_send", () => { throw "offline"; });
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  await expect(page.locator(".toast")).toContainText("offline");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("Search stays centred in the header");
  await expect(page.locator(".dk-box textarea")).toHaveValue("The More menu is cut off");
});

test("the lead's own ASK stands in the box's place: its choices, Type answer…; answered, the box is back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Typed to the lead before it asked: kept, and shown held under its card.
  await page.locator(".dk-box textarea").fill("And the footer too");
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").state = "deciding";
    st.items.push({ ...st.items[0], id: "n-lead-ask", agent_id: "", agent_name: "", desk: true, hosted: false,
      tool_input: { questions: [{ question: "Add a rename right on the Desk's line?", header: "Desk", multiSelect: false, options: [{ label: "Yes, add it there", description: "" }, { label: "No, the chat's is enough", description: "" }] }] } });
  });
  const ask = page.locator(".dk-right .dk-ask");
  await expect(ask).toContainText("Add a rename right on the Desk's line?");
  await expect(page.locator(".dk-right .dk-box")).toHaveCount(0);
  await expect(ask.locator(".foot.held textarea")).toHaveValue("And the footer too");
  // Your own words: Type answer… opens a box for them.
  await ask.locator('[data-act="redirect"]').click();
  await expect(ask.locator("textarea")).toBeVisible();
  await page.keyboard.press("Escape");
  await ask.locator("[data-pick]", { hasText: "Yes, add it there" }).click();
  const call = await cue.called("respond");
  expect(call.args.id).toBe("n-lead-ask");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").state = "working"; });
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Tell web-lead: a bug, a feature, or a question…");
  await expect(page.locator(".dk-box textarea")).toHaveValue("And the footer too");
});

test("a lead opens with its crew; a crew member with where it is, and the box goes to the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  const pane = page.locator(".dk-right .dk-pane");
  await expect(pane.locator("h1")).toHaveText("web-lead");
  await expect(pane.locator(".dk-crew .dk-mb")).toHaveCount(2);
  await expect(page.locator(".dk-box textarea")).toHaveAttribute("placeholder", "Talk to web-lead…");
  await pane.locator('[data-desk-crew="id-search"]').click();
  await expect(pane.locator("h1")).toHaveText("search-centre");
  await expect(pane.locator(".dk-files")).toHaveText("ui/header.js · ui/styles/header.css");
  await expect(page.locator('[data-desk-item="d3"]')).toHaveClass(/\bon\b/);
  await expect(pane.locator('[data-mt-act="stop"]')).toBeVisible();
  await page.locator(".dk-box textarea").fill("Leave the Inbox button alone");
  await page.locator(".dk-box textarea").press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "About search-centre: Leave the Inbox button alone" });
  // ‹ web-lead goes back to the lead.
  await page.locator(".dk-back").click();
  await expect(pane.locator("h1")).toHaveText("web-lead");
});

test("Decisions: only judgment calls, who decided, what and why; Undo (on hover) asks", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-tab="decisions"]').click();
  // Two calls; "Gave it to web-lead after Search" is a step, not in the list.
  await expect(page.locator(".dk-tab.on")).toContainText("Decisions2");
  const decs = page.locator(".dk-right .dk-dec");
  await expect(decs).toHaveCount(2);
  await expect(page.locator(".dk-right")).not.toContainText("Gave it to web-lead after Search");
  await expect(decs.first().locator(".dk-age")).toHaveText("3m ago");
  // A table row: who, what was decided, why (muted), when; Undo a quiet icon at its end, there on hover.
  await expect(decs.first().locator(".dk-lby")).toHaveText("host-lead");
  expect(await decs.first().evaluate((el) => [...el.children].map((x) => x.className))).toEqual(["dk-lby", "dk-d", "dk-w", "dk-age", "dk-lu"]);
  await decs.first().hover();
  await expect(decs.first()).toContainText("Hosted sessions don't read the settings file at start.");
  await decs.first().locator('[data-desk-undo="c1"]').click();
  await cue.called("desk_undo", { decisionId: "c1" });
  await expect(page.locator(".toast")).toHaveText("Asked its lead to undo it");
  await expect(decs.first()).toContainText("Undo asked");
});

test("a step (a route, a give) is a quiet line on its item, not a decision", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const step = page.locator(".dk-step");
  await expect(step).toHaveCount(1);
  await expect(step).toContainText("Gave it to web-lead after Search, not a new session");
  await expect(step.locator('[data-desk-undo="c3"]')).toHaveAttribute("aria-label", "Undo");
  await expect(step.locator('[data-desk-undo="c3"] svg')).toHaveCount(1);
  await expect(step.locator('[data-desk-undo="c3"]')).toHaveCSS("opacity", "0");
  await step.hover();
  await expect(step.locator('[data-desk-undo="c3"]')).toHaveCSS("opacity", "1");
  await expect(page.locator(".dk-sec h3", { hasText: "Decisions" })).toHaveCount(0);
  // The same step said again in a row is one row, "×3", not three.
  await cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"), c = d.decisions.find((x) => x.id === "c3"); d.decisions.push({ ...c, id: "c3b" }, { ...c, id: "c3c" }); });
  await expect(step).toHaveCount(1);
  await expect(step.locator(".dk-x")).toHaveText("×3");
});

test("an item's Latest shows as written: **bold** in bold, and a link that opens on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").latest = "**Done.** The mocks: https://claude.ai/artifact/abc123"; });
  await page.locator('[data-desk-item="d2"]').click();
  const sec = page.locator(".dk-sec", { has: page.locator("h3", { hasText: "Latest" }) });
  await expect(sec.locator("strong")).toHaveText("Done.");
  await expect(sec).not.toContainText("**");
  await sec.locator('a[data-href="https://claude.ai/artifact/abc123"]').click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/abc123" });
});

test("an item page's back link is one line above its title", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  const back = await page.locator(".dk-right .dk-back").boundingBox(), h1 = await page.locator(".dk-right .dk-pane h1").boundingBox();
  expect(back.height).toBeLessThan(32);
  expect(back.width).toBeGreaterThan(60);
  expect(back.y + back.height).toBeLessThanOrEqual(h1.y + 1);
});

test("a design item shows one \"Designs\" link to its canvas; a newer link replaces it; none without one", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d4"]').click();
  await expect(page.locator(".dk-right .dk-dsg")).toHaveCount(0);
  const set = (u) => cue.change((s, u) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4").designs = u; }, u);
  await set("https://claude.ai/artifact/AAA");
  const link = page.locator(".dk-right .dk-dsg a");
  await expect(link).toHaveCount(1);
  await expect(link).toHaveText("Designs");
  await set("https://claude.ai/artifact/BBB");
  await link.click();
  await cue.called("open_link", { url: "https://claude.ai/artifact/BBB" });
});

test("the Desk tab opens a Desk, never the list: the one last open, else Desk home's first", async ({ page, cue }) => {
  await cue.open("desk");
  const tab = page.locator('.switch-view [data-view="desk"]'), board = page.locator('.switch-view [data-view="board"]');
  await tab.click();
  await expect(page.locator(".dk-desks")).toHaveCount(0);
  await expect(page.locator("[data-desk-home]")).toHaveText("‹ Desks");
  // Desk home, then into website; away to the Board with All projects, and the Desk tab is back on website.
  await page.locator("[data-desk-home]").click();
  await page.locator('.dk-desk[data-desk-project="website"]').click();
  await page.locator("[data-desk-home]").click();
  await board.click();
  await tab.click();
  await expect(page.locator(".dk-desks")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
});

test("Desk home: one row a Desk with what it's about, its people, open tasks, bugs and features, and who's on it; a click opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator("[data-desk-home]").click();
  await expect(page.locator(".dk-hh")).toHaveText("Desks · 2");
  await expect(page.locator(".dh-hd span")).toHaveText(["Desk", "Project · people", "Tasks", "Bugs", "Features", "On it", ""]);
  const rows = page.locator(".dk-desks .dk-desk");
  await expect(rows).toHaveCount(2);
  const cue1 = rows.first();
  await expect(cue1).toHaveAttribute("data-desk-project", "cue");
  await expect(cue1.locator(".dh-nm")).toHaveText("cue");
  await expect(cue1.locator(".dh-ab")).toHaveText(/^(On|Next): (Bug|Feature|Task) #\S+ .+/);
  await expect(cue1.locator(".dh-pj")).toHaveText("cue");
  await expect(cue1.locator(".dh-cell").nth(1).locator(".dh-sm")).toHaveText(/^\d+ sessions? · (\d+ crew|no crew)$/);
  // A kind's cell: how many are open, then how many are working and how many wait on your check.
  const cells = cue1.locator(".dh-cell").filter({ has: page.locator(".dh-big") });
  await expect(cells).toHaveCount(3);
  await expect(cells.nth(0).locator(".dk-kic")).toHaveCount(1);
  expect(await cells.nth(0).locator(".dh-big").innerHTML()).toContain('d="m8.5 12.2');   // the task mark under Tasks
  for (let k = 0; k < 3; k++) await expect(cells.nth(k)).toHaveText(/^(–|\d+ open(\d+ working|none moving)?( · )?(\d+ to check)?)$/);
  await expect(cue1.locator(".dh-on b")).toHaveText("web-lead");
  await expect(page.locator(".dh-tbl [title]")).toHaveCount(0);
  await rows.nth(1).click();
  await expect(page.locator(".dk-items .dk-it").first()).toHaveAttribute("data-desk-item", "w1");
  // ‹ Desks in the Sessions head: back to Desk home, then into another.
  await expect(page.locator("[data-desk-home]")).toHaveText("‹ Desks");
  await page.locator("[data-desk-home]").click();
  await expect(rows).toHaveCount(2);
  await rows.first().click();
  await expect(page.locator(".dk-sh b")).toHaveText("cue");
  await expect(page.locator(".dk-sh")).not.toContainText("Change");
});

test("+ New is a menu: New session as before, New Desk asks a project and a name, starts its lead and opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator("[data-desk-home]").click();
  await cue.change((st) => { st.history = [...(st.history || []), { cwd: "/work/notes", created_ms: Date.now() }, { cwd: "/work/blog", created_ms: Date.now() }]; });
  const chip = page.locator("[data-new-menu]");
  await chip.click();
  await expect(page.locator(".nw-pop button")).toHaveText(["New session⌘N", "New Desk"]);
  // Esc closes the menu; New session opens + New's form as it always did.
  await page.keyboard.press("Escape");
  await expect(page.locator(".nw-pop")).toHaveCount(0);
  await chip.click();
  await page.locator('.nw-pop [data-lv="new"]').click();
  await expect(page.locator(".nw-pop")).toHaveCount(0);
  await expect(page.locator('[data-text="new-name"]')).toBeVisible();
  await expect(page.locator('[data-lv="desk-start"]')).toHaveCount(0);
  await page.keyboard.press("Escape");
  // New Desk: only a project (one with no Desk yet) and a name, which follows the project.
  await chip.click();
  await page.locator(".nw-pop [data-desk-new-desk]").click();
  const sheet = page.locator(".nd");
  await expect(sheet.locator(".nd-l")).toHaveText(["Project", "Name"]);
  await expect(sheet.locator("#nd-name")).toBeFocused();
  // Project is Cue's own dropdown, no system menu: the name first, its folder muted beside it.
  await expect(sheet.locator("select")).toHaveCount(0);
  const pick = sheet.locator("#nd-proj"), opts = sheet.locator(".nd-drop [role=option]");
  await pick.click();
  await expect(opts).toHaveCount(2);
  expect((await opts.locator(".nd-pn").allInnerTexts()).sort()).toEqual(["blog", "notes"]);
  await opts.filter({ hasText: "/work/notes" }).click();
  await expect(sheet.locator(".nd-drop")).toHaveCount(0);
  await expect(pick.locator(".nd-pn")).toHaveText("notes");
  await expect(pick.locator(".nd-pp")).toHaveText("/work/notes");
  await expect(sheet.locator("#nd-name")).toHaveValue("notes");
  expect(await pick.locator(".nd-pp").evaluate((el) => getComputedStyle(el).color)).not.toBe(await pick.locator(".nd-pn").evaluate((el) => getComputedStyle(el).color));
  // Keys: ↓ on it opens its list, Esc closes just the list, the sheet stays.
  await pick.focus();
  await page.keyboard.press("ArrowDown");
  await expect(opts).toHaveCount(2);
  await page.keyboard.press("Escape");
  await expect(sheet.locator(".nd-drop")).toHaveCount(0);
  await expect(sheet).toHaveCount(1);
  await expect(pick).toBeFocused();
  // Esc closes it with nothing started.
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0);
  // From Desk home's foot too; the name typed is the Desk's.
  await page.locator(".dh-add").click();
  await pick.click();
  await opts.filter({ hasText: "/work/notes" }).click();
  await sheet.locator("#nd-name").fill("Notes app");
  await sheet.locator("#nd-name").press("Enter");
  await cue.called("desk_start", { folder: "/work/notes" });
  await expect(sheet).toHaveCount(0);
  await expect(page.locator(".dk-sh b")).toHaveText("Notes app");
  // Its project has a Desk now: New Desk has nothing left to offer.
  await page.locator("[data-desk-home]").click();
  await expect(page.locator(".dk-desks .dk-desk")).toHaveCount(3);
  await page.locator(".dh-add").click();
  await pick.click();
  await expect(opts).toHaveCount(1);
  await expect(opts).toContainText("blog");
  await sheet.locator(".btn[data-nd-close]").click();
  await expect(sheet).toHaveCount(0);
});

test("the Desk's name renames from its Sessions head", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-sh").hover();
  await page.locator(".dk-sh .dk-ren").click();
  const box = page.locator(".dk-sh .pj-name-in");
  await box.fill("Cue app");
  await box.press("Enter");
  await expect(page.locator(".dk-sh b")).toHaveText("Cue app");
});

test("a crew member's session opens its lead's crew page on the Desk; Esc and ‹ Back to the Desk come back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const toCrewPage = async () => {
    await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
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
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
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
  expect(await page.evaluate((i) => deskItemState(i), it)).toMatch(/^<span class="dk-id">#\w*<\/span> · next · added by you/);
  expect(await page.evaluate((i) => deskTag(i), it)).toEqual(["", ""]);
  // One whose hand-off failed is tagged stuck instead.
  expect(await page.evaluate((i) => deskTag({ ...i, route_failed: "web-lead is gone" }), it)).toEqual(["stuck", "ask"]);
});

test("an item that couldn't be given out says why; a dropped one is gone", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-it", { hasText: "Old idea, dropped" })).toHaveCount(0);
  // Nor is it counted on its project's row on Desk home.
  const card = await page.evaluate(() => {
    state.desk.projects.push({ project: "zz", lead: null, decisions: [], items: [{ id: "z1", project: "zz", kind: "task", title: "Gone", state: "dropped", created: 1, updated: 1 }] });
    return deskAll().split('data-desk-project="zz"')[1];
  });
  expect(card).toContain("Nothing on it yet.");
  expect(card).not.toContain("Gone");
  expect(card).not.toContain(" open");
  await expect(page.locator('[data-desk-item="d6"] .dk-tag')).toHaveText("stuck");
  await expect(page.locator('[data-desk-item="d6"] .dk-who')).toHaveText("Give to…");
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
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("web-lead");
  await page.locator('.dk-right [data-desk-crew="id-search"]').focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("search-centre");
  await expect(page.locator('.dk-right [data-desk-mark]')).toHaveText("Crew");
});

test("Send back opens the item's own \"Why send it back?\" box: it keeps what you wrote when it doesn't go, and closes when it does", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  // The item's own box, typed in first, isn't where the reason comes from.
  await page.locator(".dk-box textarea").fill("for the chat");
  await page.locator('.dk-right [data-desk-act="send_back"]').click();
  const why = page.locator(".dk-right .dk-sb");
  await expect(why.locator(".dk-sb-l")).toHaveText("Why send it back?");
  const box = why.locator("textarea");
  await expect(box).toBeFocused();
  // A box as the Desk's own: the mic and a picture too.
  await expect(why.locator('[data-mic="back:d2"]')).toHaveCount(1);
  await expect(why.locator('[data-attach="back:d2"]')).toHaveCount(1);
  await page.screenshot({ path: `${process.env.SHOT_DIR || "/tmp"}/send-back-open.png` });
  await box.fill("Still loses focus");
  await cue.override("desk_item_act", () => { throw "offline"; });
  await why.locator('[data-desk-act="back_go"]').click();
  await expect(page.locator(".toast")).toContainText("offline");
  await expect(box).toHaveValue("Still loses focus");
  await cue.override("desk_item_act", () => null);
  // A picture goes with the words.
  await page.evaluate(() => { draft("back:d2").images.push({ name: "shot.png", mime: "image/png", data: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=" }); renderMain(); });
  await expect(why.locator(".chips")).toHaveCount(1);
  await box.press("Enter");
  await expect.poll(async () => (await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back").map((c) => [c.args.text, c.args.images.map((i) => i.name)]))
    .toEqual([["Still loses focus", []], ["Still loses focus", ["shot.png"]]]);
  await expect(page.locator(".dk-right .dk-sb")).toHaveCount(0);
  await expect(page.locator(".dk-box textarea")).toHaveValue("for the chat");
  expect(await cue.calls("desk_send")).toEqual([]);
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
  // No box to anyone, nothing runs it, till someone does.
  await expect(page.locator(".dk-box")).toHaveCount(0);
  await expect(page.locator(".dk-leadspot")).toHaveCount(0);
  await card.locator(".btn.primary").click();
  await cue.called("desk_set_lead", { project: "website", sid: "site" });
  await expect(page.locator(".dk-tab.on")).toHaveText("site-lead");
  await expect(page.locator(".dk-leadspot")).toContainText("site-lead");
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

test("⋯ by Runs this Desk: the project's other sessions, each with Run it; Keep or Esc goes back; Run it hands it over", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-leadspot [data-desk-change]").click();
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
  await page.locator(".dk-leadspot [data-desk-change]").click();
  await page.locator("body").press("Escape");
  await expect(page.locator(".dk-pick")).toHaveCount(0);
  await page.locator(".dk-leadspot [data-desk-change]").click();
  await card.locator('.dk-who [data-desk-lead="host"]').click();
  await cue.called("desk_set_lead", { project: "cue", sid: "host" });
  await expect(page.locator(".dk-leadspot")).toContainText("host-lead");
});

test("✎ beside a session's name (its row, its chat's header) renames the session: Enter keeps it, Esc doesn't, the row doesn't open", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator(".dk-sessions .dk-scroll .dk-srow:not(.crew)").first();
  const pen = row.locator(".dk-sren"), box = row.locator(".rename-in");
  await expect(pen).toHaveAttribute("aria-label", "Rename this session");
  await expect(pen).toHaveCSS("opacity", "0");
  await row.hover();
  await expect(pen).toHaveCSS("opacity", "1");
  const before = await row.locator(".dk-nm").textContent();
  await pen.click();
  await expect(box).toBeFocused();
  await expect(box).toHaveValue(before);
  await box.click();
  await expect(page.locator(".dk-sch")).toHaveCount(0);
  await box.fill("not this");
  await box.press("Escape");
  await expect(box).toHaveCount(0);
  await expect(row.locator(".dk-nm")).toHaveText(before);
  await row.hover();
  await pen.click();
  await box.fill("search-fix");
  await box.press("Enter");
  await expect(row.locator(".dk-nm")).toHaveText("search-fix");
  // Its chat's header has the same ✎.
  await row.click();
  const head = page.locator(".dk-sch");
  await expect(head.locator("b")).toHaveText("search-fix");
  await head.hover();
  await head.locator(".dk-sren").click();
  await head.locator(".rename-in").fill("search-fix-2");
  await head.locator(".rename-in").press("Enter");
  await expect(head.locator("b")).toHaveText("search-fix-2");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
});

test("✎ by the Desk's name: Esc keeps the name as it was; a click away keeps what you typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const head = page.locator(".dk-sh"), box = head.locator(".pj-name-in");
  await expect(head.locator(".dk-ren")).toHaveAttribute("aria-label", "Rename this Desk");
  await head.hover();
  await head.locator(".dk-ren").click();
  await expect(box).toBeFocused();
  await box.fill("not this");
  await box.press("Escape");
  await expect(box).toHaveCount(0);
  await expect(head.locator("b")).toHaveText("cue");
  await head.hover();
  await head.locator(".dk-ren").click();
  await box.fill("Cue app");
  await page.locator(".dk-items .dk-ch > b").click();
  await expect(box).toHaveCount(0);
  await expect(head.locator("b")).toHaveText("Cue app");
  expect(await page.locator(".dk-board [title]").count()).toBe(0);
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
  const below = await page.locator('[data-desk-item="d4"]').boundingBox();
  await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
  const menu = page.locator('[data-desk-item="d6"] .dk-givemenu');
  await expect(menu.locator("button")).toHaveText([/web-lead\s*lead, crew of 2/, /host-lead\s*session/, "New session"]);
  expect(await listOrder(page)).toEqual(before);
  expect((await page.locator('[data-desk-item="d4"]').boundingBox()).y).toBe(below.y);
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
  await page.locator('[data-desk-item="d6"] .dk-t').click();
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

test("the quick-add line keeps its height as you type (a one-line field doesn't grow or shrink)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const line = page.locator('.dk-qa [data-qa="line"]');
  const h = async () => (await line.boundingBox()).height;
  const before = await h();
  expect(before).toBe(32);
  await line.pressSequentially("Logo blurs");
  expect(await h()).toBe(before);
  await line.fill("");
  await line.pressSequentially("x");
  expect(await h()).toBe(before);
});

test("the quick-add line: Enter adds an item of no kind (the lead says which), the line keeps the cursor; no New item, no kind switch", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator("[data-desk-new]")).toHaveCount(0);
  await expect(page.locator(".dk-qa button", { hasText: /^(Bug|Feature|Task)$/ })).toHaveCount(0);
  const line = page.locator('.dk-qa [data-qa="line"]');
  await expect(line).toHaveAttribute("placeholder", "Add an item…");
  await line.fill("Dark mode for the Desk");
  await line.press("Enter");
  await cue.called("desk_add", { project: "cue", kind: "", title: "Dark mode for the Desk" });
  await expect(line).toHaveValue("");
  await expect(line).toBeFocused();
  // No kind yet: the pane shows no kind before where it is.
  const row = page.locator(".dk-items .dk-it", { hasText: "Dark mode for the Desk" });
  await row.click();
  await expect(page.locator(".dk-pane .dk-state")).toHaveText(/^#\w+ · not given out yet · added by you/);
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
  await cue.called("desk_add", { project: "cue", kind: "", title: "Search keeps its place", doneWhen: "It reopens where it was" });
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

test("a picture dropped on the quick-add line (or picked with its button) goes with the item; its pane shows it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const qa = page.locator(".dk-qa");
  await expect(qa.locator('[data-attach^="desk-qa:"]')).toHaveCount(1);
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([137, 80, 78, 71])], "shot.png", { type: "image/png" }));
    const el = document.querySelector('.dk-qa [data-qa="line"]');
    for (const type of ["dragover", "drop"]) el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
  });
  await expect(qa.locator(".chip")).toHaveCount(1);
  const line = qa.locator('[data-qa="line"]');
  await line.fill("Logo blurs on the Desk");
  await line.press("Enter");
  const call = await cue.called("desk_add", { project: "cue", kind: "", title: "Logo blurs on the Desk" });
  expect(call.args.images.map((i) => [i.name, i.mime])).toEqual([["shot.png", "image/png"]]);
  await expect(qa.locator(".chip"), "the picture went with it").toHaveCount(0);
  await expect(line).toBeFocused();
  // The item, with its picture kept: the pane shows it under Pictures.
  const id = await page.evaluate(() => deskOf("cue").items[0].id);
  await cue.change((st, id) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === id).images = ["data:image/png;base64,iVBORw0KGgo="]; }, id);
  await page.locator(`[data-desk-item="${id}"]`).click();
  await expect(page.locator(".dk-pane .thumbs .thumb")).toHaveCount(1);
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

test("⋯ by Runs this Desk with an item open: the open pane closes and the picker shows", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await page.locator(".dk-right .dk-drop").click();
  await page.locator(".dk-leadspot [data-desk-change]").click();
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
  await page.locator(".dk-leadspot [data-desk-change]").click();
  await page.screenshot({ path: `${dir}/cue-desk-change.png` });
  await pick(page, "website");
  await page.locator(".dk-pick").waitFor();
  await page.screenshot({ path: `${dir}/cue-desk-pick1.png` });
});

test("the lead's own Desk lines are drawn as lines in its chat, code blocks left as they are; no Actions row, no Send to…", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: [
      "Here's the plan.",
      "",
      "ITEM: bug | Box loses focus | done when: typing keeps focus",
      "- **ROUTE:** #d7k2 -> web-lead | after: #a1 | why: it knows the box",
      "DONE: #d3",
      "CONTEXT: host-lead -> keep | why: more to come",
      "",
      "```",
      "ITEM: task | an example | done when: never",
      "```",
    ].join("\n") });
    renderMain();
  });
  const said = page.locator(".dk-chat .cv-agent").last();
  await expect(said.locator(".own-lines .dk-say")).toHaveCount(3);
  await expect(said.locator(".own-lines .dk-route")).toHaveCount(1);
  await expect(said.locator(".dk-say").nth(0).locator("svg.dk-kic")).toHaveCount(1);
  await expect(said.locator(".dk-say").nth(0)).toContainText("Box loses focus");
  await expect(said.locator(".dk-say").nth(0).locator(".dk-say-s")).toHaveText(" · done when typing keeps focus");
  // A hand-off on one line (its kind's icon, number → who), the why open under it; "Why ⌃" folds it.
  const route = said.locator(".dk-route");
  await expect(route).toContainText("#d7k2→web-lead");
  await expect(route.locator(".dk-rt-why")).toHaveText("Why ⌃");
  await expect(route.locator(".dk-rt-why-s")).toHaveText("after #a1 · it knows the box");
  expect(await route.locator(".dk-rt-row").evaluate((el) => el.getBoundingClientRect().height)).toBeLessThan(40);
  // Among other lines it has no box of its own inside the block's.
  await expect(route.locator(".dk-rt-row")).toHaveCSS("border-top-width", "0px");
  await route.locator(".dk-rt-why").click();
  await expect(route.locator(".dk-rt-why")).toHaveText("Why ⌄");
  await expect(route.locator(".dk-rt-why-s")).toHaveCount(0);
  // It stays folded through a redraw, and opens again.
  await page.evaluate(() => renderMain());
  await expect(said.locator(".dk-rt-why-s")).toHaveCount(0);
  await said.locator(".dk-rt-why").click();
  await expect(said.locator(".dk-rt-why-s")).toHaveCount(1);
  await expect(said.locator(".dk-say").nth(1)).toContainText(/^Done Feature #d3 · \S/);
  await expect(said.locator(".dk-say").nth(2)).toContainText("host-lead → keep");
  await expect(said.locator(".cv-text")).toContainText("Here's the plan.");
  await expect(said.locator("pre")).toContainText("ITEM: task | an example");
  await expect(said).not.toContainText("ROUTE:");
  await expect(page.locator(".dk-chat .ac-sugs")).toHaveCount(0);
  await expect(page.locator(".dk-chat .cv-fwd")).toHaveCount(0);
});

test("a reply with little to read has no Listen or Read: a lone ROUTE line, a short answer; a longer one has both", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const say = (text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);
  const last = page.locator(".dk-chat .cv-agent").last();
  await say("ROUTE: #d2 -> web-lead | why: it's free and warm, and the owner wants mocks first. Make 2-3 quick mocks on the Desk picks canvas of how Looks right could show less: a quiet tick that turns into Looks right on hover, one \"Check 3\" button, and so on. Post the link here.");
  await expect(last.locator(".dk-route")).toHaveCount(1);
  // Alone, the hand-off is the only box: the block around it has none.
  await expect(last.locator(".own-lines")).toHaveCSS("border-top-width", "0px");
  await expect(last.locator("[data-listen], [data-read]")).toHaveCount(0);
  await say("Sent it to web-lead.");
  await expect(last.locator("[data-listen]")).toHaveCount(0);
  await say("Sent it to web-lead, who has the header code warm from the last change; it can start right away and should be done within the hour, then you check it.");
  await expect(last.locator("[data-listen]")).toHaveCount(1);
  await expect(last.locator("[data-read]")).toHaveCount(1);
});

test("Cue's New item note shows as a small card an item, without its Route words for the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\n' + [
      "New item #d2 (feature): Quick phrases | done when: they sit by Send | pictures: `a.png` `b.png` | from web-lead",
      "New item #d9: Something new",
      "Route each now: ROUTE it to a session, or -> new. Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.",
    ].join("\n") });
    renderMain();
  });
  const news = page.locator(".dk-chat .dk-news").last();
  await expect(news.locator(".dk-new")).toHaveCount(2);
  await expect(news.locator(".dk-new").nth(0)).toHaveText("New item Feature #d2 · Quick phrases · done when they sit by Send · 2 pictures · from web-lead");
  await expect(news.locator(".dk-new").nth(1)).toHaveText("New item #d9 · Something new");
  await expect(page.locator(".dk-chat")).not.toContainText("Route each now");
  await news.locator('[data-desk-chk-item="d2"]').click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toHaveCount(1);
});

test("Cue's note on a lead's DONE it couldn't act on shows as a small card an item, without its words for the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\n' + [
      "#d2 not closed: it waits on your check",
      "#zz9 not closed: this Desk has no item #zz9",
      "Only the person closes an item that waits on their check (Looks right). Don't say DONE for these again.",
    ].join("\n") });
    renderMain();
  });
  const notes = page.locator(".dk-chat .dk-news").last();
  await expect(notes.locator(".dk-new")).toHaveCount(2);
  await expect(notes.locator(".dk-new").nth(0)).toHaveText("Not closed Feature #d2 · Quick phrases at the right of Send · it waits on your check");
  await expect(notes.locator(".dk-new").nth(1)).toHaveText("Not closed #zz9 · this Desk has no item #zz9");
  await expect(page.locator(".dk-chat")).not.toContainText("Don't say DONE");
  await notes.locator('[data-desk-chk-item="d2"]').click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toHaveCount(1);
});

test("an item's pane: its words select and copy, its buttons don't", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const pane = page.locator(".dk-right .dk-pane");
  const sel = (loc) => loc.evaluate((el) => getComputedStyle(el).webkitUserSelect || getComputedStyle(el).userSelect);
  expect(await sel(pane.locator("h1"))).toBe("text");
  expect(await sel(pane.locator(".dk-state"))).toBe("text");
  expect(await sel(pane.locator("button").first())).toBe("none");
  const h1 = pane.locator("h1"), box = await h1.boundingBox();
  await page.mouse.move(box.x + 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  expect((await page.evaluate(() => String(getSelection()))).length).toBeGreaterThan(3);
});

test("a working item offers Restart only once its holder has stopped (a crash cut its turn short)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').click();
  const pane = page.locator(".dk-right .dk-pane");
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await page.evaluate(() => { sessionOf("host").state = "waiting"; renderMain(); });
  await pane.locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "start_now" });
  // A crew member's item: Restart only when that teammate isn't at work.
  await page.locator('[data-desk-item="d3"]').click();
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await page.evaluate(() => { sessionOf("lead").team.find((m) => m.agent_id === "id-search").state = "done"; renderMain(); });
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Restart");
});

test("an item back to check stands out until you open it; its pane links to what was made", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    const d2 = state.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2");
    d2.made = ["https://claude.ai/artifact/abc123", "/tmp/mocks/list.html"];
    d2.files = ["ui/send.js"];
    renderMain();
  });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row).toHaveClass(/landed/);
  await expect(row.locator(".dk-tag")).toHaveClass(/landed/);
  await row.locator(".dk-t").click();
  await expect(page.locator(".dk-pane .dk-sec", { hasText: "What it made" }).locator(".dk-made")).toHaveText(["Open the page ↗", "list.html ↗"]);
  await expect(row).not.toHaveClass(/landed/);
  await expect(row.locator(".dk-tag")).toHaveClass(/check/);
});

test("a to-check item's page lists its Files changed, each with its counts; a click opens that file over the Desk, Esc closes it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/chat/compose.js", "ui/styles/chat.css"]; });
  await page.locator('.dk-items [data-desk-item="d2"] .dk-t').click();
  // The files Looks right will commit, as the hub reads them.
  await cue.called("desk_item_diff", { id: "d2" });
  const sec = page.locator(".dk-pane .dk-sec", { hasText: "Files changed" });
  await expect(sec.locator(".dk-fc-p")).toHaveText(["ui/chat/compose.js", "ui/styles/chat.css"]);
  await expect(sec.locator(".dk-fc").first().locator(".dv-n")).toHaveText(["+2", "−1"]);
  // Read once for the page, not again on each redraw.
  await page.evaluate(() => { renderMain(); renderMain(); });
  await page.waitForTimeout(200);
  expect((await cue.calls("desk_item_diff")).length).toBe(1);
  await sec.locator(".dk-fc").nth(1).click();
  // Over the Desk: the item's files on the left, the one clicked picked, its diff on the right. The Desk stays put.
  const ov = page.locator(".dov");
  await expect(ov.locator(".dov-title")).toContainText("Feature #d2");
  await expect(ov.locator(".dov-f .dov-p")).toHaveText(["ui/chat/compose.js", "ui/styles/chat.css"]);
  await expect(ov.locator(".dov-f.on .dov-p")).toHaveText("ui/styles/chat.css");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("ui/styles/chat.css");
  await expect(ov.locator(".dov-lines .a").first()).toHaveText("+new ui/styles/chat.css");
  await expect(page.locator(".active-pane .dv")).toHaveCount(0);
  // ↑ steps to the file above; a click on a file picks it.
  await page.keyboard.press("ArrowUp");
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("ui/chat/compose.js");
  await ov.locator(".dov-f").nth(1).click();
  await expect(ov.locator(".dov-fh .dov-p")).toHaveText("ui/styles/chat.css");
  // Esc closes only it: the item's page is still open under it.
  await page.keyboard.press("Escape");
  await expect(page.locator(".dov")).toHaveCount(0);
  await expect(sec.locator(".dk-fc")).toHaveCount(2);
  // A click outside it closes it too.
  await sec.locator(".dk-fc").first().click();
  await expect(page.locator(".dov")).toHaveCount(1);
  await page.mouse.click(10, 10);
  await expect(page.locator(".dov")).toHaveCount(0);
  await expect(page.locator(".dov-scrim [title]")).toHaveCount(0);
});

test("written about an item ready to check, it's sent back with your words: off Waiting on you, to its session", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  await page.locator(".dk-box textarea").fill("The More menu is cut off");
  await page.locator(".dk-box textarea").press("Enter");
  // As Send back: to its holder, not the lead.
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The More menu is cut off" });
  expect(await cue.calls("desk_send")).toEqual([]);
  await expect(page.locator(".toast").last()).toContainText("Sent back");
  await expect.poll(() => listOrder(page)).toEqual(["# Waiting on you", "d1", "# In progress", "d3", "d2", "d5", "# Queued", "d4", "d6", "# Done", "d7", "d8", "d9"]);
  // The box emptied, the item's page still open.
  await expect(page.locator(".dk-box textarea")).toHaveValue("");
  await expect(page.locator(".dk-pane")).toHaveCount(1);
  // One that doesn't go: still waiting on you, your words kept in its box.
  await page.locator('[data-desk-item="d1"]').click();
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").state = "to_check"; });
  await cue.override("desk_item_act", () => { throw "its session has ended"; });
  await page.locator(".dk-box textarea").fill("Still loses focus");
  await page.locator(".dk-box textarea").press("Enter");
  await expect(page.locator(".toast").last()).toContainText("its session has ended");
  await expect(page.locator(".dk-box textarea")).toHaveValue("Still loses focus");
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("Reply box loses focus after Send");
});

/** The Desk run by host instead, so "lead" (with its crew) is a session in the list like any other. */
const crewNotLead = (cue) => cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = { sid: "host", name: "host-lead", state: "live" }; });

test("three columns: the project's sessions on the left, the lead's chat in the middle, the items on the right", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const cols = await page.locator(".dk-board > .dk-card").evaluateAll((els) => els.map((el) => [...el.classList].find((c) => ["dk-sessions", "dk-right", "dk-items"].includes(c))));
  expect(cols).toEqual(["dk-sessions", "dk-right", "dk-items"]);
  const xs = await page.locator(".dk-board > .dk-card").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().x));
  expect(xs[0] < xs[1] && xs[1] < xs[2]).toBe(true);
  // A session in the project that holds nothing: Not on the Desk.
  await cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
  // The Desk's lead in its own spot at the top, with no crew, not even old ones; the list under it: Working first
  // (host-lead), then Not on the Desk.
  await cue.change((st) => { const l = st.sessions.find((x) => x.session_id === "lead"); l.team.push({ ...l.team[0], agent_id: "id-old", call: "id-old", name: "old", state: "committed" }); });
  const left = page.locator(".dk-sessions");
  await expect(left.locator('.dk-leadspot [data-desk-lead-chat="lead"]')).toHaveCount(1);
  await expect(left.locator('.dk-leadspot [data-desk-lead-chat="lead"] .dk-pill')).toHaveCount(0);
  // Each row marked in its top-right corner with what it is (Type marks pick 8).
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-rmark')).toHaveText("Desk lead");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-rmark')).toHaveText("Session");
  // The right list: each item's kind as its badge (pick J) at the row's left.
  const items = page.locator(".dk-items");
  await expect(items.locator('[data-desk-item="d1"] > .dk-badge > svg.dk-kic')).toHaveCount(1);
  expect(await items.locator(".dk-it").evaluateAll((els) => els.every((el) => el.querySelector(".dk-kic")))).toBe(true);
  const ic = await items.locator('[data-desk-item="d1"] .dk-kic').evaluate((el) => [getComputedStyle(el).color, getComputedStyle(document.documentElement).getPropertyValue("--accent")]);
  expect(ic[0]).not.toBe("");
  await expect(left.locator(".dk-srow.crew")).toHaveCount(0);
  await expect(left.locator("[data-desk-done-fold]")).toHaveCount(0);
  await expect(left.locator(".dk-ch")).toContainText("2 sessions · 1 working");
  const rows = await left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-gh") ? `# ${el.textContent.trim()}`
    : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}${el.querySelector(".dk-pill") ? ` [${el.querySelector(".dk-pill").textContent}]` : ""}`));
  expect(rows).toEqual(["# Working 1", "host", "# Not on the Desk 1", "spare"]);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-s2')).toContainText("#d5");
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-s2 .dk-w')).toHaveText("nothing on the Desk");
  await expect(left.locator('[data-desk-chat-of="spare"] [data-desk-member]')).toHaveText("Add to Desk");
  await expect(left.locator('[data-desk-chat-of="host"] [data-desk-member]')).toHaveCount(0);
  // The lead's chat shows: its row is lit, alone.
  await expect(left.locator(".dk-srow.on")).toHaveCount(1);
  await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
  // Another session: its chat in the middle, its row lit in place; Esc goes back to the lead's.
  await left.locator('[data-desk-chat-of="host"]').click();
  await expect(page.locator('.dk-right [data-desk-chat="host"]')).toHaveCount(1);
  await expect(page.locator(".dk-right .dk-ch [data-desk-mark]")).toHaveText(/^(Lead|Bug)$/);
  const bg = (l) => l.evaluate((e) => getComputedStyle(e).backgroundColor);
  const hostBg = await bg(page.locator(".dk-right [data-desk-mark]"));
  // A session with no crew and nothing on the Desk: Session, in the same one color as every other mark.
  await left.locator('[data-desk-chat-of="spare"]').click();
  await expect(page.locator(".dk-right .dk-ch [data-desk-mark]")).toHaveText("Session");
  expect(await bg(page.locator(".dk-right [data-desk-mark]"))).toBe(hostBg);
  await left.locator('[data-desk-chat-of="host"]').click();
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-chat-of", "host");
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", /^Talk to /);
  await page.keyboard.press("Escape");
  await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
  // Another session's crew sit under it; a crew row opens its pane, lit.
  await crewNotLead(cue);
  await expect(left.locator('[data-desk-crew="id-focus"] .dk-st')).toHaveText("waiting on you");
  await left.locator('[data-desk-crew="id-search"]').click();
  await expect(page.locator(".dk-right .dk-pane h1")).toHaveText("search-centre");
  await expect(left.locator(".dk-srow.on")).toHaveAttribute("data-desk-crew", "id-search");
  // + New session opens + New, in the project's folder.
  await left.locator("[data-desk-new-session]").click();
  await expect(page.locator('[data-text="new-name"]')).toBeVisible();
});

test("each session row says its size and cache: \"142k · cache 12m\", \"cold\", amber in its last 5 minutes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // No numbers yet: no line.
  await expect(page.locator(".dk-sessions .dk-size")).toHaveCount(0);
  await cue.change((st) => {
    const t = Date.now();
    st.insight = { lead: { tokens: 142000, until_ms: t + 12 * 60000 - 2000 }, host: { tokens: 88000, until_ms: t + 3 * 60000 } };
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] });
    st.insight.spare = { tokens: 51000, until_ms: t - 60000 };
  });
  const left = page.locator(".dk-sessions");
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).toHaveText("142k · cache 12m");
  await expect(left.locator('[data-desk-lead-chat="lead"] .dk-size')).not.toHaveClass(/hot/);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveText("88k · cache 3m");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-size')).toHaveClass(/hot/);
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-size')).toHaveText("51k · cold");
  // Each with its state and what it's on.
  await expect(left.locator('[data-desk-chat-of="host"] .dk-st')).toHaveText("working");
  await expect(left.locator('[data-desk-chat-of="host"] .dk-s2')).toContainText("#d5");
  await expect(left.locator('[data-desk-chat-of="spare"] .dk-st')).toHaveText("idle");
});

/** The animation the app's own styles give `el` (the test page turns every animation off, so not what it computes). */
const blinkOf = (loc) => loc.evaluate((el) => [...document.styleSheets].flatMap((sh) => { try { return [...sh.cssRules]; } catch { return []; } })
  .filter((r) => r.selectorText && !r.selectorText.startsWith("*") && el.matches(r.selectorText) && r.style?.animationName).map((r) => r.style.animationName).pop() || "none");
test("a working session's dot blinks and its current step shows under its title, with its time, live", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const h = st.sessions.find((x) => x.session_id === "host");
    h.doing = "Editing ui/desk.js"; h.doing_ms = Date.now() - 4 * 60000 - 5000;
  });
  const host = page.locator('.dk-sessions [data-desk-chat-of="host"]');
  await expect(host.locator(".dk-now")).toHaveText("Editing ui/desk.js · 4m");
  // The app's blink, never a spinner.
  expect(await blinkOf(host.locator(".dk-st > i"))).toBe("pulse");
  // A new step: the line follows.
  await cue.change((st) => { const h = st.sessions.find((x) => x.session_id === "host"); h.doing = "Running tests"; h.doing_ms = Date.now(); });
  await expect(host.locator(".dk-now")).toHaveText(/^Running tests · /);
  // Stopped: no blink, no step.
  await cue.change((st) => { const h = st.sessions.find((x) => x.session_id === "host"); h.state = "idle"; h.doing = ""; });
  await expect(host.locator(".dk-now")).toHaveCount(0);
  expect(await blinkOf(host.locator(".dk-st > i"))).toBe("none");
  // A teammate at work: its step too.
  await crewNotLead(cue);
  const mate = page.locator('.dk-sessions [data-desk-crew="id-search"]');
  expect(await blinkOf(mate.locator(".dk-st > i"))).toBe("pulse");
});

test("done teammates fold under their lead as \"N done · show\", opened in place; done but not committed stays out, tagged", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const lead = st.sessions.find((x) => x.session_id === "lead"), t = Date.now(), base = lead.team[0];
    const mate = (agent_id, name, state) => ({ ...base, agent_id, call: agent_id, name, state, what: `${name}'s work`, step: "", ended_ms: t - 60000 });
    lead.team.push({ ...mate("id-tidy", "tidy-copy", "done"), uncommitted: true }, mate("id-a", "old-a", "committed"), mate("id-b", "old-b", "committed"), mate("id-c", "old-c", "aside"));
  });
  await crewNotLead(cue);
  const left = page.locator(".dk-sessions");
  const crewNames = () => left.locator(".dk-srow.crew").evaluateAll((els) => els.map((el) => el.dataset.deskCrew));
  // Folded: the three finished ones are one line; the one not committed stays, tagged.
  const fold = left.locator('[data-desk-done-fold="lead"]');
  await expect(fold).toHaveText("3 done · show");
  expect(await crewNames()).toEqual(["id-search", "id-focus", "id-tidy"]);
  await expect(left.locator('[data-desk-crew="id-tidy"] .dk-pill.loose')).toHaveText("not committed");
  await expect(left.locator('[data-desk-crew="id-search"] .dk-pill.loose')).toHaveCount(0);
  // Opened in place, under the line; nothing above it moves.
  const above = await left.locator('[data-desk-crew="id-tidy"]').boundingBox();
  await fold.click();
  await expect(fold).toHaveText("3 done · hide");
  await expect(fold).toHaveAttribute("aria-expanded", "true");
  expect(await crewNames()).toEqual(["id-search", "id-focus", "id-tidy", "id-a", "id-b", "id-c"]);
  await expect(left.locator('[data-desk-crew="id-c"] .dk-st')).toHaveText("set aside");
  await expect(left.locator(".dk-srow.past")).toHaveCount(3);
  expect((await left.locator('[data-desk-crew="id-tidy"]').boundingBox()).y).toBe(above.y);
  expect((await left.locator('[data-desk-crew="id-a"]').boundingBox()).y).toBeGreaterThan(above.y);
  await fold.click();
  await expect(left.locator(".dk-srow.past")).toHaveCount(0);
  // Git says its files are committed (Cue marks it): nothing of it left in play, it folds too.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").team.find((t) => t.agent_id === "id-tidy").uncommitted = false; });
  await expect(fold).toHaveText("4 done · show");
  expect(await crewNames()).toEqual(["id-search", "id-focus"]);
  // Committed: still in the fold.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").team.find((t) => t.agent_id === "id-tidy").state = "committed"; });
  await expect(fold).toHaveText("4 done · show");
  // Opened, it stays open across a restart.
  await fold.click();
  await expect(fold).toHaveText("4 done · hide");
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { const lead = st.sessions.find((x) => x.session_id === "lead"); lead.team.push({ ...lead.team[0], agent_id: "id-a", call: "id-a", name: "old-a", state: "committed" }); });
  await crewNotLead(cue);
  await expect(left.locator('[data-desk-done-fold="lead"]')).toHaveAttribute("aria-expanded", "true");
  await expect(left.locator('[data-desk-crew="id-a"]')).toBeVisible();
});

test("selecting a session lights the item it holds where it is, reading \"working · <session> · 4m\"", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const items = page.locator(".dk-items");
  const before = await listOrder(page);
  const ys = () => items.locator("[data-desk-item]").evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().y)));
  const top = await ys();
  await expect(items.locator(".dk-it.on")).toHaveCount(0);
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  // host-lead's item, lit alone, in its own row; nothing moved.
  await expect(items.locator(".dk-it.on")).toHaveCount(1);
  const d5 = items.locator('[data-desk-item="d5"]');
  await expect(d5).toHaveClass(/\bon\b/);
  await expect(d5.locator(".dk-who")).toHaveText(/^working · host-lead · \d+m$/);
  expect(await listOrder(page)).toEqual(before);
  expect(await ys()).toEqual(top);
  // The lead's row lights the lead's item instead; back to nothing lit once something else is shown.
  await page.locator('.dk-sessions [data-desk-lead-chat="lead"]').click();
  await expect(d5).not.toHaveClass(/\bon\b/);
  await expect(d5.locator(".dk-who")).toHaveText("host-lead");
  expect(await ys()).toEqual(top);
});

test("the lead's box takes pictures and has the mic, as a session's chat box does", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-box");
  const key = await box.locator("textarea").getAttribute("data-text");
  await expect(box).toHaveAttribute("data-drop", key);
  await expect(box.locator("[data-mic]")).toHaveAttribute("data-mic", key);
  await expect(box.locator("[data-attach]")).toHaveAttribute("data-attach", key);
  // A picture pasted (as the paste handler adds it): shown on the box, and sent with the words.
  const png = "data:image/png;base64,iVBORw0KGgo=";
  await page.evaluate(([k, data]) => { draft(k).images.push({ name: "pasted image", mime: "image/png", data }); renderMain(); }, [key, png]);
  await expect(box.locator(".chips .chip")).toHaveCount(1);
  await box.locator("textarea").fill("This is cut off");
  await box.locator("textarea").press("Enter");
  await cue.called("desk_send", { project: "cue", text: "This is cut off", images: [{ name: "pasted image", mime: "image/png", data: png }] });
  await expect(page.locator(".dk-box .chips .chip")).toHaveCount(0);
});

test("working sessions show first in a Working group; a session moves when its state changes, never on a click", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [] }));
  const left = page.locator(".dk-sessions");
  const order = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}`
    : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}`));
  const before = ["# Working", "host", "# Not on the Desk", "spare"];
  await expect.poll(order).toEqual(before);
  // A click lights a row where it is.
  await left.locator('[data-desk-chat-of="spare"]').click();
  await expect(left.locator('[data-desk-chat-of="spare"]')).toHaveClass(/\bon\b/);
  expect(await order()).toEqual(before);
  // spare starts work: it moves up to Working, after host; host stops: it leaves Working for On the Desk.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "spare").state = "working"; });
  await expect.poll(order).toEqual(["# Working", "host", "spare"]);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").state = "idle"; });
  await expect.poll(order).toEqual(["# Working", "spare", "# On the Desk", "host"]);
});

test("an idle session whose crew is at work is listed under Working, its working crew first, never below idle sessions", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    st.sessions.find((x) => x.session_id === "host").state = "idle";
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", thread: [],
      team: [{ agent_id: "m1", name: "tidy", state: "done", uncommitted: true }, { agent_id: "m2", name: "busy-one", state: "working" }] });
  });
  const left = page.locator(".dk-sessions");
  const order = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}`
    : el.matches(".dk-sfold") ? null : `${el.matches(".crew") ? "  " : ""}${el.dataset.deskLeadChat || el.dataset.deskChatOf || el.dataset.deskCrew}`).filter(Boolean));
  await expect.poll(order).toEqual(["# Working", "spare", "  m2", "  m1", "# On the Desk", "host"]);
  await expect(left.locator(".dk-ch .dk-sub")).toContainText("1 working");
});

test("Looks right commits the item's files at once: no sheet; the row and a toast say \"Committed <hash> · N files\" with Undo, which takes it back", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"]; });
  await openDesk(page);
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".sheet")).toHaveCount(0);
  await expect(page.locator(".toast")).toContainText("Committed abc1234 · 2 files");
  const line = page.locator(".dk-cmt").first();
  await expect(line).toContainText("Committed abc1234 · 2 files");
  await expect(line.locator('[data-desk-act="undo_commit"]')).toHaveText("Undo · back to check");
  await line.locator('[data-desk-act="undo_commit"]').click();
  await cue.called("desk_commit_undo", { id: "d2" });
  await expect(page.locator(".toast").last()).toHaveText("Back to check · commit taken back, every change still in place");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();

  // Another commit came after it: still back to check, its commit kept, said so; no code changed either way.
  await cue.change((s) => { s.desk_commit_after = true; });
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await page.locator('.dk-cmt [data-desk-act="undo_commit"]').first().click();
  await expect(page.locator(".toast").last()).toHaveText("Back to check · its commit stays (another came after it); no code changed");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();
});

test("✓ on a row: it dims while Undo can take it back, then folds away (height to nothing) before it goes", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await row.locator(".dk-ok").click();
  await expect(row).toHaveClass(/\bsoon\b/);
  await expect(row.locator(".dk-i1")).toHaveCSS("opacity", "0.45");
  // The fold lasts a moment: watched as it happens, its height then.
  const h = await page.evaluate(() => new Promise((done) => {
    const seen = () => { const el = document.querySelector('.dk-items .dk-it.dk-leave[data-desk-item="d2"]'); if (el) { obs.disconnect(); done(parseFloat(el.style.getPropertyValue("--h"))); } };
    const obs = new MutationObserver(seen);
    obs.observe(document.body, { subtree: true, childList: true, attributes: true });
    seen();
  }));
  expect(h).toBeGreaterThan(20);
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator('.dk-items .dk-it.dk-leave')).toHaveCount(0);
});

test("Looks right on an item whose file another session is editing: done at once, \"Commit waiting: <file> (<session> is editing it)\", then \"Committed <hash>\" with Undo once it's made; with no files just done", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => {
    s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"];
    s.desk_busy = [{ path: "ui/styles/send.css", by: "tidier" }];
  });
  await openDesk(page);
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await expect(page.locator(".toast")).toContainText("Done · commit waiting: ui/styles/send.css (tidier is editing it)");
  const line = page.locator(".dk-cmt").first();
  await expect(line).toHaveText("Commit waiting: ui/styles/send.css (tidier is editing it) · Undo · back to check");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toHaveCount(0);

  // Cue makes the commit once the tidier is done with the file: the same line says so, with Undo.
  await cue.change((s) => { const d = s.desk.projects.find((d) => d.project === "cue"); for (const i of [...d.items, ...(d.checks || [])].filter((i) => i.id === "d2")) Object.assign(i, { commit: "abc1234", commit_wait: null }); });
  await expect(line).toContainText("Committed abc1234");
  await expect(line.locator('[data-desk-act="undo_commit"]')).toBeVisible();

  // Why it couldn't be made, when a word stopped it.
  await cue.change((s) => { const d = s.desk.projects.find((d) => d.project === "cue"); for (const i of [...d.items, ...(d.checks || [])].filter((i) => i.id === "d2")) Object.assign(i, { commit: "", commit_wait: { files: [], busy: [], error: 'Not committed: "x" is in ui/send.js' } }); });
  await expect(line).toContainText('Commit not made: Not committed: "x" is in ui/send.js');

  await cue.change((s) => { const i = s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2"); Object.assign(i, { state: "to_check", commit: "", commit_wait: null, files: [] }); });
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await expect(page.locator(".toast").last()).toContainText("Done");
  await expect(page.locator('[data-desk-item="d2"] .dk-cm')).toHaveCount(0);
  // Done with nothing committed still has its Undo: back to check, no code changed.
  await page.locator('[data-desk-item="d2"]').first().click();
  const plain = page.locator('.dk-cmt:has-text("Looked right")').first();
  await expect(plain).toHaveText("Looked right · Undo · back to check");
  await plain.locator('[data-desk-act="undo_commit"]').click();
  await cue.called("desk_commit_undo", { id: "d2" });
  await expect(page.locator(".toast").last()).toHaveText("Back to check · no code changed");
});

/** A session of cue's that isn't on its Desk ("spare", holding nothing). */
const spare = (cue) => cue.change((st) => st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", thread: [] }));
/** The left card's groups and rows, as "# group" and session ids. */
const leftRows = (page) => page.locator(".dk-sessions .dk-scroll > *").evaluateAll((els) => els.map((el) => el.matches(".dk-gh") ? `# ${el.firstChild.textContent.trim()}` : el.dataset.deskChatOf || el.dataset.deskCrew).filter(Boolean));

test("Add to Desk on a session that isn't on it puts it on the Desk's own list, in place", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await spare(cue);
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host", "# Not on the Desk", "spare"]);
  await page.locator('.dk-sessions [data-desk-chat-of="spare"] [data-desk-member]').click();
  expect((await cue.called("desk_member")).args).toEqual({ project: "cue", sid: "spare", on: true });
  await expect.poll(() => leftRows(page)).toEqual(["# Working", "host", "# On the Desk", "spare"]);
  await expect(page.locator(".dk-sessions [data-desk-member]")).toHaveCount(0);
  // The click was the button's, not the row's: the lead's chat still shows.
  await expect(page.locator('.dk-sessions [data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
});

test("a session's ⋯ menu adds it to its project's Desk, or takes it off", async ({ page, cue }) => {
  await cue.open("board-waiting");
  await page.locator('.nrow[data-big="n1"]').click();
  const project = await page.evaluate(() => CueBridge.world.state.sessions.find((x) => x.session_id === "relay").project);
  expect(project).toBeTruthy();
  const item = page.locator('.more-menu [data-desk-member-set="relay"]');
  await page.locator('.active-pane [data-more="relay"]').click();
  await expect(item).toContainText("Add to Desk");
  await item.click();
  expect((await cue.called("desk_member")).args).toEqual({ project, sid: "relay", on: true });
  await expect(page.locator(".more-menu")).toHaveCount(0);
  await page.locator('.active-pane [data-more="relay"]').click();
  await expect(item).toContainText("Remove from Desk");
  await item.click();
  expect((await cue.calls("desk_member")).at(-1).args).toEqual({ project, sid: "relay", on: false });
  await page.locator('.active-pane [data-more="relay"]').click();
  await expect(item).toContainText("Add to Desk");
});

test("after a restart the lead's chat has its past (Cue's log beyond its last few) and what you half typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator(".dk-box textarea").fill("half a thought about the");
  await page.waitForTimeout(500);   // drafts are saved a moment after typing (window.js saveDrafts)
  // Quit and reopen: a new window, Cue's state from scratch; drafts from cue.db.
  await cue.open("desk");
  await openDesk(page);
  await expect(page.locator(".dk-box textarea")).toHaveValue("half a thought about the");
  const chat = page.locator(".dk-chat");
  await expect(chat).toContainText("Earlier message 75");
  await expect(chat.locator(".cv-older button[data-act='load-older']")).toHaveText("Earlier messages");
  await chat.locator(".cv-older button[data-act='load-older']").click();
  await expect(chat).toContainText("Earlier message 45");
});

test("↑ in the lead's box brings back what you sent it (without the Desk's summary), ↓ goes forward to what you typed", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  expect(await page.evaluate(() => deskLead(deskOf(projFilter)).sid)).toBe("lead");
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "lead"), t = Date.now();
    s.thread = [...(s.thread || []), { role: "you", text: '<desk project="cue">\nItems: none\n</desk>\n\nfirst ask', at_ms: t - 2000, images: [] },
      { role: "agent", text: "ok", at_ms: t - 1500, images: [] }, { role: "you", text: "second ask", at_ms: t - 1000, images: [] }];
  });
  const box = page.locator(".dk-box textarea");
  await box.fill("draft");
  await box.press("ArrowUp");
  await expect(box).toHaveValue("second ask");
  await box.press("ArrowUp");
  await expect(box).toHaveValue("first ask");
  await box.press("ArrowDown");
  await box.press("ArrowDown");
  await expect(box).toHaveValue("draft");
});

test("a DONE is a card in the lead's chat: item, who, what to test, files, installed or not; its words copy, its links open, its item and who did it open; Send back sends it back", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const card = page.locator(".dk-chat .dk-chk");
  await expect(card).toHaveCount(1);
  await expect(card.locator(".dk-chk-t")).toHaveText("Feature #d2 · Quick phrases at the right of Send");
  await expect(card.locator(".dk-chk-w")).toContainText("Done by web-lead");
  // What to test: a short checklist, a point a line.
  await expect(card.locator(".dkc-dw li")).toHaveText(["Quick phrases sit at the right of Send", "More opens the rest"]);
  await expect(card.locator(".dk-chk-w")).toContainText("ago · 2 files");
  await expect(card.locator(".dk-chk-files")).toHaveCount(0);
  // Its words can be selected (to copy), and a link in them opens.
  expect(await card.locator(".dkc-dw").evaluate((el) => getComputedStyle(el).webkitUserSelect || getComputedStyle(el).userSelect)).toBe("text");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").checks[0].done_when = "See https://example.com/spec for it"; });
  await card.locator('.dkc-dw a[data-href="https://example.com/spec"]').click();
  await cue.called("open_link", { url: "https://example.com/spec" });

  await expect(card.locator(".dk-chk-in")).toHaveText("Not installed yet");
  // Installed after it was done: says so, with when.
  await page.evaluate(() => { state.desk.installed_ms = 1793984280000 + 60000; renderMain(); });
  await expect(card.locator(".dk-chk-in")).toContainText("In the installed Cue · installed ");

  // Send back opens a box of its own on the card; with nothing typed its Send asks why, and sends nothing.
  const lead = page.locator(".dk-right .dk-box textarea");
  await lead.fill("a word for the lead");
  await card.locator('[data-desk-act="send_back"]').click();
  await expect(card.locator(".dk-sb-l")).toHaveText("Why send it back?");
  await card.locator('[data-desk-act="back_go"]').click();
  await expect(page.locator(".toast")).toContainText("Say why it goes back first");
  expect((await cue.calls("desk_item_act")).filter((c) => c.args.act === "send_back")).toEqual([]);
  // Its words go to the item's holder only; the lead's box is left as it was, and nothing goes to the lead.
  await card.locator(".dk-sb textarea").fill("The arrows still skip one");
  await card.locator('[data-desk-act="back_go"]').click();
  await cue.called("desk_item_act", { id: "d2", act: "send_back", text: "The arrows still skip one" });
  await expect(lead).toHaveValue("a word for the lead");
  expect(await cue.calls("desk_send")).toEqual([]);
  // The card keeps why it went back.
  await expect(card.locator(".dk-chk-back")).toHaveText("The arrows still skip one");
  await expect(card.locator(".dkc-l").last()).toHaveText("Sent back");
  await expect(card.locator('[data-desk-act]')).toHaveCount(0);

  // Who did it opens its chat; the item's name opens the item.
  await card.locator('.dk-chk-w [data-desk-chat-of]').click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, sessionOf(deskOpen.sid)?.name])).toEqual(["chat", "web-lead"]);
  await page.evaluate(() => deskShow(null));
  await card.locator('.dk-chk-t [data-desk-chk-item="d2"]').click();
  await expect(page.locator('.dk-it.on[data-desk-item="d2"]')).toBeVisible();
});

test("finished items: the 10 newest, then 10 more as the list's end comes into view; nothing jumps, and 500 stay quick", async ({ page, cue }) => {
  await cue.open("desk");
  // 500 finished long ago, kept only in Cue's database (not in the Desk's state): the list loads them as you scroll.
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.older = Array.from({ length: 500 }, (_, k) => ({ id: `o${String(k).padStart(3, "0")}`, project: "cue", kind: "task", title: `Old ${k}`, state: "done", holder: null, created: 1000 + k, updated: 1000 + k, done_at: 1000 + k, from: "you" }));
  });
  await openDesk(page);
  await page.evaluate(() => { deskDonePages.clear(); renderMain(); });   // as the Desk opens with them there
  const list = page.locator(".dk-items .dk-scroll");
  const doneIds = async () => { const o = await listOrder(page); return o.slice(o.indexOf("# Done") + 1); };
  const count = (n) => expect.poll(async () => (await doneIds()).length).toBe(n);
  await count(10);
  await expect(list.locator('[data-desk-done-more="cue"]')).toHaveCount(1);
  // The newest first: today's three, then the newest of the old.
  expect((await doneIds()).slice(0, 5)).toEqual(["d7", "d8", "d9", "o499", "o498"]);
  // To the end: 10 more, and where you were reading stays where it was.
  const anchor = list.locator('[data-desk-item="o495"]');
  const top0 = await anchor.evaluate((e) => e.getBoundingClientRect().top);
  await list.evaluate((e) => { e.scrollTop = e.scrollHeight; });
  await count(20);
  const scrolled = await list.evaluate((e) => e.scrollTop);
  expect(await anchor.evaluate((e) => e.getBoundingClientRect().top) + scrolled).toBeCloseTo(top0, 0);
  await list.evaluate((e) => { e.scrollTop = e.scrollHeight; });
  await count(30);
  // Every one loaded: the end goes, and redrawing 500 stays quick.
  await expect.poll(async () => { await list.evaluate((e) => { e.scrollTop = e.scrollHeight; }); return (await doneIds()).length; }, { timeout: 20000, intervals: [50] }).toBe(503);
  await expect(list.locator('[data-desk-done-more]')).toHaveCount(0);
  const ms = await page.evaluate(() => { const t = performance.now(); for (let n = 0; n < 10; n++) deskList("cue"); return (performance.now() - t) / 10; });
  expect(ms).toBeLessThan(50);
});

test("your words about an item show its kind's icon, kind and number over them in the lead's chat; a click opens it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: "About #d1 (Reply box loses focus after Send): keep focus (in the box): please" }); renderMain(); });
  const you = page.locator(".dk-chat .cv-you").last();
  await expect(you.locator(".dk-about svg.dk-kic")).toHaveCount(1);
  await expect(you.locator(".dk-about")).toHaveText("Bug #d1 · Reply box loses focus after Send");
  await expect(you.locator(".cv-you-text")).toHaveText("Bug #d1 · Reply box loses focus after Sendkeep focus (in the box): please");
  await you.locator(".dk-about").click();
  await expect(page.locator(".dk-pane h1")).toHaveText("Reply box loses focus after Send");
});

test("Looks right waits a moment with \"Done · Undo\" on the item and in a toast; Undo then leaves it to check, nothing sent", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await expect(page.locator('[data-desk-item="d2"] .dk-soon')).toHaveText("Done · Undo · back to check");
  await expect(page.locator(".toast")).toHaveText("Done · Undo · back to check");
  await page.locator(".toast .toast-undo").click();
  await expect(page.locator(".toast")).toHaveText("Still to check");
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();
  await page.waitForTimeout(3300);
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  // Undo on the item does the same; left alone, it goes.
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  await page.locator('[data-desk-item="d2"] .dk-soon [data-desk-act="looks_undo"]').click();
  await expect(page.locator('[data-desk-item="d2"] .dk-ok')).toBeVisible();
  await page.locator('[data-desk-item="d2"] .dk-ok').click();
  expect(await cue.calls("desk_looks_right")).toEqual([]);
  await cue.called("desk_looks_right", { id: "d2" });
});

test("Waiting on you stays at 0, not in red; an open item takes the solid highlight with its words in the highlight's ink", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const you = page.locator(".dk-items .dk-gh.you");
  const deny = await page.evaluate(() => { const t = document.createElement("i"); t.style.color = "var(--deny)"; document.body.append(t); const c = getComputedStyle(t).color; t.remove(); return c; });
  expect(await you.evaluate((e) => getComputedStyle(e).color)).toBe(deny);
  await cue.change((st) => { for (const i of st.desk.projects.find((d) => d.project === "cue").items) if (["needs_you", "to_check"].includes(i.state)) i.state = "done"; });
  await expect(you).toHaveText(/Waiting on you\s*0/);
  expect(await you.evaluate((e) => getComputedStyle(e).color)).not.toBe(deny);
  const row = page.locator('[data-desk-item="d3"]');
  await row.click();
  await expect(row).toHaveClass(/\bon\b/);
  const [bg, sel, ink, idInk] = await row.evaluate((e) => { const c = (v) => { const t = document.createElement("i"); t.style.color = v; document.body.append(t); const r = getComputedStyle(t).color; t.remove(); return r; }; return [getComputedStyle(e).backgroundColor, c("var(--sel)"), c("var(--on-sel)"), getComputedStyle(e.querySelector(".dk-id")).color]; });
  expect(bg).toBe(sel);
  expect(idInk).toBe(ink);
});

test("an open item's box keeps the cursor: through redraws, and after Enter sends it, as you go on typing", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d3"]').click();
  const box = page.locator(".dk-right .dk-box textarea");
  await box.click();
  await page.keyboard.type("half a thought");
  const at = await box.boundingBox();
  for (let n = 0; n < 3; n++) await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d3").title += "."; });
  await page.keyboard.type(" more");
  expect(await page.locator(".dk-card.dk-right").evaluate((e) => getComputedStyle(e).overflow)).toBe("clip");   // can't be scrolled to show the box
  expect(await box.boundingBox()).toEqual(at);
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("half a thought more");
  // Enter sends it; the send takes a moment, and you go on typing meanwhile: it all stays in this item's box.
  await cue.override("desk_send", () => new Promise((r) => { window.sendGoes = () => r({ sid: "lead" }); }));
  await page.keyboard.press("Enter");
  await expect(box).toHaveValue("");
  await page.keyboard.type("and then");
  await page.evaluate(() => window.sendGoes());
  await expect(page.locator(".toast").last()).toContainText("Sent to");
  await page.keyboard.type(" more");
  await expect(page.locator(".dk-pane h1")).toContainText("Search stays centred");
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("and then more");
  expect(await box.boundingBox()).toEqual(at);
});
