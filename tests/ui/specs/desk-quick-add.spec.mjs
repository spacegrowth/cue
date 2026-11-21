// The Desk: the quick-add line, the lead's own lines and Cue's notes as its chat draws them, an item's pane,
// paused and parked items, and items back to check.
import { test, expect, pick, openDesk } from "./cue.mjs";
import { listOrder } from "./desk.mjs";

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
  await expect(page.locator(".dk-pg .dk-state")).toHaveText(/^added by you · /);
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-pword")).toHaveText("Not given out");
});

test("Add an item is one box and the picture button: no You'll see box; the lead writes it (#400)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const qa = page.locator(".dk-qa"), line = qa.locator('[data-qa="line"]');
  await expect(qa.locator("input")).toHaveCount(1);
  await expect(qa.locator('[data-qa="done"], [data-desk-qa-done]')).toHaveCount(0);
  await expect(qa).not.toContainText("You'll see");
  await expect(qa.locator("[data-attach]")).toBeVisible();
  await line.fill("Search keeps its place");
  await line.press("Shift+Enter");
  const c = await cue.called("desk_add", { project: "cue", kind: "", title: "Search keeps its place" });
  expect(c.args.see).toBeUndefined();
  await expect(line).toHaveValue("");
  await expect(qa.locator("input")).toHaveCount(1);
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
  expect((await cue.calls("desk_add")).every((c) => !("see" in c.args) && !("doneWhen" in c.args))).toBe(true);
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
  await cue.change((st, id) => { Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === id), { images: ["data:image/png;base64,iVBORw0KGgo="], holder: null }); }, id);
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

test("one item that doesn't go comes back in the line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.override("desk_add", () => { throw "offline"; });
  const line = page.locator('.dk-qa [data-qa="line"]');
  await line.fill("Search keeps its place");
  await line.press("Enter");
  await expect(page.locator(".toast")).toContainText("Not added: offline");
  await expect(line).toHaveValue("Search keeps its place");
});

test("⋯ in the head strip with an item open: the open pane closes and the picker shows", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d6"]').click();
  await page.locator(".dk-panel[data-side-for] .dk-drop").click();
  await page.locator(".dk-strip [data-desk-change]").click();
  await expect(page.locator(".dk-pick h2")).toHaveText("Who runs this Desk?");
  await expect(page.locator(".dk-right .dk-ipane")).toHaveCount(0);
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
  await page.locator(".dk-strip [data-desk-change]").click();
  await page.screenshot({ path: `${dir}/cue-desk-change.png` });
  await pick(page, "website");
  await page.locator(".dk-pick").waitFor();
  await page.screenshot({ path: `${dir}/cue-desk-pick1.png` });
});

test("the lead's own Desk lines are drawn as lines in its chat, code blocks left as they are; no Send to…", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: [
      "Here's the plan.",
      "",
      "ITEM: bug | Box loses focus | you'll see: typing keeps focus | done when: focus.spec passes",
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
  // Each line one short quiet line at the left (#157): no rules, no box, no chip.
  const ln = said.locator(".own-lines .dk-ln");
  await expect(ln).toHaveCount(4);
  await expect(ln.nth(0)).toHaveText("Added a bug: Box loses focus");
  await expect(ln.nth(0).locator("svg.dk-kic")).toHaveCount(0);
  await expect(said.locator(".own-lines")).toHaveCSS("border-top-width", "0px");
  expect(await said.locator(".own-lines .dk-cl").count()).toBe(0);
  // A hand-off that waits: "#d7k2 waits on #a1", a plain sentence at the left, one line, its why shut; no arrow, no chevron.
  const route = ln.nth(1);
  await expect(route.locator(".mg-lh")).toHaveText("#d7k2 waits on #a1");
  await expect(said.locator(".own-lines")).not.toContainText("→");
  await expect(said.locator(".own-lines")).not.toContainText("⌄");
  const lay = await route.evaluate((el) => { const r = el.getBoundingClientRect(), t = el.querySelector(".mg-lh").getBoundingClientRect(), m = el.closest(".cv-text").getBoundingClientRect(); return { left: t.left - m.left, h: r.height }; });
  expect(lay.left).toBeLessThan(4);
  expect(lay.h).toBeLessThan(24);
  await expect(route.locator(".mg-full")).toHaveCount(0);
  await expect(route.locator(".mg-line")).toHaveAttribute("aria-expanded", "false");
  // A click opens its why under it, the family's .mg-full (a soft fill, no border, #501); it stays open through a redraw, and a click shuts it.
  await route.locator(".mg-line").dispatchEvent("click");
  await expect(route.locator(".mg-full")).toHaveText("Then to web-lead · it knows the box");
  await expect(route.locator(".mg-full")).not.toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await expect(route.locator(".mg-full")).toHaveCSS("border-top-width", "0px");
  await page.evaluate(() => renderMain());
  await expect(said.locator(".dk-ln").nth(1).locator(".mg-full")).toHaveCount(1);
  await said.locator(".dk-ln").nth(1).locator(".mg-line").dispatchEvent("click");
  await expect(said.locator(".mg-full")).toHaveCount(0);
  // Done: "Feature #d3 is done", its number a link; its title on a click, which opens the item.
  await expect(ln.nth(2).locator(".mg-lh")).toHaveText("Feature #d3 is done");
  await expect(ln.nth(2).locator("button.mg-id")).toHaveText("Feature #d3");
  // Its kind's icon before its number, as on every item line (#292).
  await expect(ln.nth(2).locator("button.mg-id svg.dk-kic")).toHaveCount(1);
  await ln.nth(2).locator(".mg-line").dispatchEvent("click");
  await expect(ln.nth(2).locator(".dk-ln-it")).toHaveCount(1);
  await expect(ln.nth(3).locator(".mg-lh")).toHaveText("Kept host-lead's context");
  await expect(said.locator(".cv-text")).toContainText("Here's the plan.");
  await expect(said.locator("pre")).toContainText("ITEM: task | an example");
  await expect(said).not.toContainText("ROUTE:");
  await expect(page.locator(".dk-chat .cv-fwd")).toHaveCount(0);
  // The number is a link: it opens its item, not the line's why; Enter on the line opens the why.
  await page.evaluate(() => deskLnOpen.clear());
  await page.evaluate(() => renderMain());
  await said.locator(".dk-ln").nth(1).locator(".mg-line").focus();
  await page.keyboard.press("Enter");
  await expect(said.locator(".dk-ln").nth(1).locator(".mg-full")).toHaveCount(1);
  await said.locator(".dk-ln").nth(2).locator("button.mg-id").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d3"]);
  await expect(page.locator(".dk-chat .dk-ln").nth(2).locator(".mg-full")).toHaveCount(0);
});

test("the dispatcher chat keeps the plain answer above its routing update", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "Yes. All approvals should appear under Waiting on you.\n\nROUTE: #d2 -> web-lead | why: warm" }); renderMain(); });
  const reply = page.locator(".dk-chat .cv-agent").last();
  await expect(reply.locator(".cv-text > p:first-child")).toHaveText("Yes. All approvals should appear under Waiting on you.");
  const l = reply.locator(".dk-ln");
  await expect(l.locator(".mg-lh")).toHaveText(/^Gave \w+ #d2 to web-lead$/);
  await expect(l.locator(".mg-who")).toHaveText("web-lead");
});

test("a hand-off to a new session names that session once Cue has started it, and #new is the item just above", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const say = (text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);
  const last = page.locator(".dk-chat .cv-agent").last();
  await say(["ITEM: bug | \"Looks right\" is too loud | done when: it's quiet", "ROUTE: #new -> new | why: no one has the header"].join("\n"));
  await expect(last.locator(".dk-ln").nth(1).locator(".mg-lh")).toHaveText("Gave the new item to a new session");
  // Cue makes the item and starts a session for it: the card says both.
  await page.evaluate(() => {
    const d = state.desk.projects.find((p) => p.project === "cue");
    d.items.push({ id: "q9", project: "cue", kind: "bug", title: "\"Looks right\" is too loud", state: "working", created: Date.now(), updated: Date.now(), holder: { kind: "session", sid: "s-new", name: "cue-117" } });
    renderMain();
  });
  await expect(last.locator(".dk-ln").nth(1).locator(".mg-lh")).toHaveText("Gave Bug #q9 to cue-117");
  await expect(last.locator(".dk-ln").nth(0).locator(".mg-lh")).toHaveText("Added Bug #q9: \"Looks right\" is too loud");
  // A named session stays as the lead named it.
  await say("ROUTE: #q9 -> web-lead | why: warm");
  await expect(last.locator(".dk-ln .mg-who")).toHaveText("web-lead");
});

test("a reply of only routing lines is short lines at the left: TITLE and ROUTE, its title and why on a click (#157)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "TITLE: #d2 | A longer title the lead gave it, long enough that it would wrap on a narrow chat\nROUTE: #d2 -> web-lead | kind: feature | after: #d3 | why: free and warm, and it knows the header code from the last change" }); renderMain(); });
  const last = page.locator(".dk-chat .cv-agent").last(), ln = last.locator(".dk-ln");
  await expect(ln).toHaveCount(2);
  await expect(ln.nth(0).locator(".mg-lh")).toHaveText(/^Renamed \w+ #d2$/);
  await expect(ln.nth(1).locator(".mg-lh")).toHaveText(/^\w+ #d2 waits on #d3$/);
  // Stacked tight: the second starts right under the first.
  const [a, b] = [await ln.nth(0).boundingBox(), await ln.nth(1).boundingBox()];
  expect(b.y - (a.y + a.height)).toBeLessThan(2);
  // One body line (#501: 13.5px/1.5) and the row's 1px above and below.
  expect(a.height).toBeLessThan(23);
  // Only the item numbers carry an icon, their kind's (#292); the lines themselves have none.
  await expect(last.locator(".dk-ln svg")).toHaveCount(2);
  await expect(last.locator(".dk-ln button.mg-id > svg.dk-kic")).toHaveCount(2);
  await expect(last.locator(".cv-text")).not.toContainText("TITLE:");
  for (const l of await ln.all()) expect(await l.evaluate((el) => el.getBoundingClientRect().height)).toBeLessThan(24);
  await ln.nth(1).locator(".mg-line").dispatchEvent("click");
  await expect(ln.nth(1).locator(".mg-full")).toContainText("free and warm");
  await expect(ln.nth(1).locator(".mg-full .dk-ln-it")).toHaveCount(1);
  await ln.nth(0).locator(".mg-line").dispatchEvent("click");
  await expect(ln.nth(0).locator(".mg-full")).toHaveText("A longer title the lead gave it, long enough that it would wrap on a narrow chat");
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await expect(() => last.screenshot({ path: `${dir}/cue-desk-lead-lines-light.png`, timeout: 1000 })).toPass(); // (a steps load can redraw the row mid-shot)
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(ln.nth(1).locator("button.mg-id").first()).not.toHaveCSS("color", "rgb(0, 0, 0)");
  await expect(() => last.screenshot({ path: `${dir}/cue-desk-lead-lines-dark.png`, timeout: 1000 })).toPass(); // (a steps load can redraw the row mid-shot)
  // Its title opens the item.
  await ln.nth(1).locator(".dk-ln-it").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "d2"]);
});

test("a reply with little to read has no Listen or Read: a lone ROUTE line, a short answer; a longer one has both", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const say = (text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);
  const last = page.locator(".dk-chat .cv-agent").last();
  await say("ROUTE: #d2 -> web-lead | why: it's free and warm, and the owner wants mocks first. Make 2-3 quick mocks on the Desk picks canvas of how Looks right could show less: a quiet tick that turns into Looks right on hover, one \"Check 3\" button, and so on. Post the link here.");
  await expect(last.locator(".dk-ln")).toHaveCount(1);
  // No box around it; its why shut until you click.
  await expect(last.locator(".own-lines")).toHaveCSS("border-top-width", "0px");
  await expect(last.locator(".mg-full")).toHaveCount(0);
  await expect(last.locator("[data-listen], [data-read]")).toHaveCount(0);
  await say("Sent it to web-lead.");
  await expect(last.locator("[data-listen]")).toHaveCount(0);
  await say("Sent it to web-lead, who has the header code warm from the last change; it can start right away and should be done within the hour, then you check it.");
  await expect(last.locator("[data-listen]")).toHaveCount(1);
  await expect(last.locator("[data-read]")).toHaveCount(1);
});

test("Cue's New item note shows as a small card an item, without its words for the lead", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n3 open\n</desk>\n\n' + [
      "New item #d2 (feature): Quick phrases | you'll see: they sit by Send | done when: compose.spec passes | from web-lead",
      "New item #d9: Something new",
      "Route each now: ROUTE it to a session, or -> new. Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.",
    ].join("\n") });
    renderMain();
  });
  // One card for them (#501): NEW ITEMS and its age in the head, a line an item.
  const news = page.locator('.dk-chat [data-mg="new"]').last();
  await expect(news.locator(".mg-tag")).toHaveText("NEW ITEMS");
  await expect(news.locator(".mg-h .mg-age")).toHaveText(/^\d+s$/);
  const cards = news.locator(".dk-ni");
  await expect(cards).toHaveCount(2);
  // Its kind, number and title on one line.
  await expect(cards.nth(0).locator(".mg-id")).toHaveText("Feature #d2");
  // The Desk's title for it now leads; the words it came with sit under it, lighter.
  await expect(cards.nth(0).locator(".dk-ni-t")).toHaveText("Quick phrases at the right of Send");
  await expect(cards.nth(0).locator(".dk-ni-w")).toHaveText("Quick phrases");
  // What you'll see, not its done when: that's the session's check (#382).
  await expect(cards.nth(0).locator(".dk-ni-s")).toHaveText("you'll see they sit by Send · from web-lead");
  await expect(cards.nth(0).locator("svg.dk-kic")).toHaveCount(1);
  // Said without a kind, but the Desk has it as a bug by now: the card shows what it is now.
  await expect(cards.nth(1).locator(".mg-id")).toHaveText("Bug #d9");
  // No "New item" line, and nothing under a title with no more to say.
  await expect(cards.nth(1).locator(".dk-ni-s")).toHaveCount(0);
  await expect(news).not.toContainText("New item");
  await expect(page.locator(".dk-chat")).not.toContainText("Route each now");
  await cards.nth(0).click();
  await expect(page.locator('.dk-panel[data-side-for="d2"]')).toHaveCount(1);
});

test("Cue's word on a stopped item is a card from Cue, not a message from You; its words for the lead stay hidden (#244)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // As desk/hub/stall/stall.rs stop_words and desk/hub/lead/run.rs start_failed say them.
  await page.evaluate(() => {
    const t = Date.now(), lead = sessionOf("lead");
    lead.thread.push({ at_ms: t, role: "you", text: "#d2 (Quick phrases) stopped, not committed: desk-ui ended its turn on it with no DONE, and its work isn't committed (ui/desk.js, ui/styles/desk.css), so it isn't in the user's app. Its last line: The row keeps its seat; tests next.\nSettle it: ROUTE #d2 to desk-ui again with what's left as the why (Cue commits its work when it says DONE: #d2), or route it on; only if the user must decide, ASK: #d2 | <question> | choices: … Until then it isn't shown as waiting on the user." });
    lead.thread.push({ at_ms: t, role: "you", text: "#d9 (Something new) stopped: web-lead ended its turn on it with no DONE and no question for the user. Its last line: Checking the box next.\nSettle it: ROUTE #d9 to web-lead again with what to do next as the why, or route it on; only if the user must decide, ASK: #d9 | <question> | choices: … Until then it isn't shown as waiting on the user." });
    lead.thread.push({ at_ms: t, role: "you", text: "Cue couldn't start a new session for #d9 (Something new): no folder for cue. It's still new; ROUTE it to a session, or to new to try again." });
    renderMain();
  });
  // Cards from Cue in the message family (#501): what happened as the tag, the item as the id, its title, one plain line.
  const cards = page.locator('.dk-chat [data-mg="stop"]');
  await expect(cards).toHaveCount(3);
  await expect(cards.nth(0).locator(".mg-who")).toHaveText(["Cue", "desk-ui"]);
  await expect(cards.nth(0).locator(".mg-tag")).toHaveText("STOPPED, NOT COMMITTED");
  await expect(cards.nth(0).locator(".mg-h .mg-id")).toHaveText("Feature #d2");
  await expect(cards.nth(0).locator(".mg-t").first()).toHaveText("Quick phrases at the right of Send");
  await expect(cards.nth(0).locator(".dk-mq")).toHaveText("desk-ui ended its turn with no DONE, and its work isn't in your app yet.");
  await expect(cards.nth(0).locator(".mg-full")).toHaveText("Its last line: The row keeps its seat; tests next.");
  await expect(cards.nth(0).locator(".mg-f .mg-btn")).toHaveText("Open #d2");
  // The files folded: a count, the paths only once opened.
  const fold = cards.nth(0).locator(".dk-mfiles");
  await expect(fold.locator("summary")).toHaveText("2 files not committed");
  await expect(fold.locator(".dk-mfl")).toBeHidden();
  await fold.locator("summary").click();
  await expect(fold.locator(".dk-mfl")).toHaveText("ui/desk.jsui/styles/desk.css");
  await expect(cards.nth(1).locator(".mg-tag")).toHaveText("STOPPED");
  await expect(cards.nth(1).locator(".dk-mq")).toHaveText("web-lead ended its turn with no DONE and no question for you.");
  await expect(cards.nth(1).locator(".dk-mfiles")).toHaveCount(0);
  await expect(cards.nth(2).locator(".mg-tag")).toHaveText("COULDN'T START");
  await expect(cards.nth(2).locator(".dk-mq")).toHaveText("Cue couldn't start a session for it: no folder for cue.");
  // Not from You, and none of Cue's words for the lead.
  const chat = page.locator(".dk-chat");
  await expect(chat).not.toContainText("Settle it");
  await expect(chat).not.toContainText("ROUTE it to a session");
  await expect(chat.locator('[data-mg="stop"] [title]')).toHaveCount(0);
  if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-stop-card.png` });
  await cards.nth(1).locator(".mg-h .mg-id").click();
  await expect(page.locator('.dk-panel[data-side-for="d9"]')).toHaveCount(1);
});

test("a session's TO LEAD: words are a card from that session about its item; Cue's NOTE hint stays hidden (#474)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // As desk/hub/turn/heard.rs to_lead_said says them.
  await page.evaluate(() => {
    const t = Date.now(), lead = sessionOf("lead");
    lead.thread.push({ at_ms: t, role: "you", text: "From desk-ui on #d2 (Quick phrases):\nShould the row wrap on narrow windows?\nAnswer it with NOTE: #d2 | <your words>." });
    lead.thread.push({ at_ms: t, role: "you", text: "From web-lead:\nI'm free for more work.\nIt holds no item here, so a NOTE can't reach it." });
    renderMain();
  });
  const cards = page.locator('.dk-chat [data-mg="from"]');
  await expect(cards).toHaveCount(2);
  await expect(cards.nth(0).locator(".mg-who")).toHaveText("desk-ui");
  await expect(cards.nth(0).locator(".mg-tag")).toHaveText("TO LEAD");
  await expect(cards.nth(0).locator(".mg-id")).toHaveText("Feature #d2");
  await expect(cards.nth(0).locator(".mg-t")).toHaveText(["Quick phrases at the right of Send", "Should the row wrap on narrow windows?"]);
  await expect(cards.nth(1).locator(".mg-who")).toHaveText("web-lead");
  await expect(cards.nth(1).locator(".mg-id")).toHaveCount(0);
  const chat = page.locator(".dk-chat");
  await expect(chat).not.toContainText("Answer it with NOTE");
  await expect(chat).not.toContainText("a NOTE can't reach it");
  await expect(chat.locator('[data-mg="from"] [title]')).toHaveCount(0);
  if (process.env.SHOT_DIR) await chat.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-to-lead-card.png` });
  await cards.nth(0).locator("[data-desk-chk-item]").click();
  await expect(page.locator('.dk-panel[data-side-for="d2"]')).toHaveCount(1);
});

test("a new item with a picture, no kind, and Cue's SPLIT words before its Route words: a card with a thumbnail; no path, no words for the lead (#147)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const pic = "/site/shots/board.png";   // a picture in the repo: tests/ui/shots is git-ignored, so a clean copy has none
  await page.evaluate((pic) => {
    // Into the Desk's own items (deskOf gives a copy).
    state.desk.projects.find((x) => x.project === "cue").items.push({ id: "p1", project: "cue", kind: "", title: "The rail's dot is off by a pixel", state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you", images: [pic] });
    // As desk/mod.rs new_items_words says it: the SPLIT words on the Route line, and the kind words after it.
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n4 open\n</desk>\n\n' + [
      `New item #p1: The rail's dot is off by a pixel | you'll see: it sits centred | pictures: \`${pic}\``,
      "If one of yours holds two separate problems, SPLIT it first (Cue tells you the new numbers); if its title isn't short and plain, TITLE it. Route it now: ROUTE it to a session, or -> new. Say what one without a kind is in its ROUTE line: | kind: bug, feature or task.",
    ].join("\n") });
    renderMain();
  }, pic);
  const news = page.locator('.dk-chat [data-mg="new"]'), card = news.locator(".dk-ni");
  await expect(card).toHaveCount(1);
  // No kind yet: no kind icon (as on its row), just its number.
  await expect(card.locator("svg.dk-kic")).toHaveCount(0);
  await expect(card.locator(".mg-id")).toHaveText("#p1");
  await expect(card.locator(".dk-ni-t")).toHaveText("The rail's dot is off by a pixel");
  // No "New item" line (#203): just what you'll see, quiet, under the title.
  await expect(card.locator(".dk-ni-s")).toHaveText("you'll see it sits centred");
  await expect(card).not.toContainText("New item");
  // At the right, where your messages are, as wide as what it says, not floating mid-chat (#167).
  const cb = await news.boundingBox(), you = await page.locator(".dk-chat .cv-you-text").first().boundingBox();
  expect(Math.abs((cb.x + cb.width) - (you.x + you.width))).toBeLessThan(2);
  expect(cb.width).toBeLessThan(360);
  if (process.env.SHOT_DIR) await page.locator(".dk-chat").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-new-card.png` });
  // Its picture as a small thumbnail under the text (#203), rounded, with a line round it; loaded, not a path.
  const img = card.locator(".thumb img");
  await expect(img).toHaveCount(1);
  await expect(img).toHaveAttribute("src", pic);
  await expect.poll(() => img.evaluate((el) => el.complete && el.naturalWidth)).toBeGreaterThan(0);
  const box = await card.locator(".thumb").boundingBox();
  expect(Math.round(box.height)).toBe(40);
  expect(box.y).toBeGreaterThan((await card.locator(".dk-ni-s").boundingBox()).y);
  await expect(card.locator(".thumb")).toHaveCSS("border-top-width", "1px");
  await expect(card.locator(".thumb")).toHaveCSS("border-top-left-radius", "6px");
  // Nothing of the raw message: no path, no words for the lead, no "picture" count.
  const chat = page.locator(".dk-chat");
  for (const t of [pic, "pictures:", "SPLIT", "Route it now", "Say what one without a kind", "1 picture"]) await expect(chat).not.toContainText(t);
  await page.screenshot({ path: `${process.env.SHOT_DIR || process.env.TMPDIR || "/tmp"}/cue-desk-new-item-light.png` });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: `${process.env.SHOT_DIR || process.env.TMPDIR || "/tmp"}/cue-desk-new-item-dark.png` });
  // A click on the card opens the item.
  await card.locator(".dk-ni-s").click();
  await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "p1"]);
  await expect(page.locator('.dk-panel[data-side-for="p1"]')).toHaveCount(1);
  // By keyboard too: Tab reaches it (a ring shows), and Enter or Space opens it.
  for (const key of ["Enter", " "]) {
    await page.evaluate(() => deskShow(null));
    await page.keyboard.press("Shift");   // the keyboard in use, as after a Tab: focus shows its ring
    await card.focus();
    await expect(card).toHaveCSS("outline-style", "solid");
    await page.keyboard.press(key === " " ? "Space" : key);
    await expect.poll(() => page.evaluate(() => deskOpen && [deskOpen.kind, deskOpen.id])).toEqual(["item", "p1"]);
  }
});

test("a new item the lead gave a short title: its number on one line, the short title under it, your words lighter below, no narrow column (#203)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const words = "The new-item card in the Dispatcher chat wraps its long text into a thin column right beside the icon, and its label takes a line of its own, which wastes room and is hard to read";
  await page.evaluate((words) => {
    state.desk.projects.find((x) => x.project === "cue").items.push({ id: "t9", project: "cue", kind: "feature", title: "A tidy new-item card", words, state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you" });
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "you", text: '<desk project="cue">\n4 open\n</desk>\n\n' + [
      `New item #t9 (feature): ${words} | done when: one compact card`,
      "Route it now: ROUTE it to a session, or -> new.",
    ].join("\n") });
    renderMain();
  }, words);
  const card = page.locator('.dk-chat .dk-ni[data-desk-chk-item="t9"]');
  await expect(card.locator(".mg-line .mg-id")).toHaveText("Feature #t9");
  await expect(card.locator(".mg-line svg.dk-kic")).toHaveCount(1);
  await expect(card.locator(".dk-ni-t")).toHaveText("A tidy new-item card");
  await expect(card.locator(".dk-ni-w")).toHaveText(words);
  await expect(card).not.toContainText("New item");
  // The words sit under the item's line, as wide as the card, not in a column beside the icon.
  const cb = await card.boundingBox(), w = await card.locator(".dk-ni-w").boundingBox(), l1 = await card.locator(".mg-line").boundingBox();
  expect(w.y).toBeGreaterThanOrEqual(l1.y + l1.height - 1);
  expect(w.width).toBeGreaterThan(cb.width - 30);
  // Your words lighter than the title.
  const col = (loc) => loc.evaluate((el) => getComputedStyle(el).color);
  expect(await col(card.locator(".dk-ni-w"))).not.toBe(await col(card.locator(".dk-ni-t")));
  for (const c of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: c });
    if (process.env.SHOT_DIR) await page.locator(".dk-chat").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-new-card-203-${c}.png` });
  }
});

test("Cue's new-item note waiting to go to the lead: the same card as once it's sent, only the line under it says so (#370)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const note = (id, t) => '<desk project="cue">\n4 open\n</desk>\n\n' + [`New item #${id} (feature): ${t} | done when: it works`, "Route it now: ROUTE it to a session, or -> new."].join("\n");
  await page.evaluate((args) => {
    const [a, b] = args;
    const d = state.desk.projects.find((x) => x.project === "cue");
    for (const [id, title] of [["h1", "One Away switch"], ["h2", "Another switch"], ["h3", "A third switch"]]) d.items.push({ id, project: "cue", kind: "feature", title, state: "new", created: Date.now(), updated: Date.now(), holder: null, from: "you" });
    const lead = sessionOf("lead");
    lead.thread.push({ at_ms: Date.now() - 5000, role: "you", text: a });
    lead.state = "working";
    lead.held = { role: "you", text: b, at_ms: Date.now(), images: [] };
    renderMain();
  }, [note("h1", "One Away switch"), note("h2", "Another switch")]);
  const chat = page.locator(".dk-chat");
  const sent = chat.locator('.dk-ni[data-desk-chk-item="h1"]'), held = chat.locator('.cv-later .dk-ni[data-desk-chk-item="h2"]');
  await expect(held).toHaveCount(1);
  await expect(chat.locator(".cv-you.held")).toHaveCount(0);
  await expect(chat.locator('.cv-later > .mg-f')).toContainText("Not sent yet · Cue sends it when its turn ends");
  await expect(chat).not.toContainText("Route it now");
  const look = (l) => l.evaluate((el) => { const c = getComputedStyle(el); return [el.className, c.backgroundColor, c.borderTopColor, c.boxShadow, Math.round(el.getBoundingClientRect().right)]; });
  expect(await look(held), "waiting looks like sent").toEqual(await look(sent));
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-held-note.png`, animations: "disabled" });
  // Queued in the agent: the same.
  await page.evaluate((c) => { const lead = sessionOf("lead"); delete lead.held; lead.queued = { role: "you", text: c, at_ms: Date.now(), images: [] }; renderMain(); }, note("h3", "A third switch"));
  const queued = chat.locator('.cv-later .dk-ni[data-desk-chk-item="h3"]');
  await expect(queued).toHaveCount(1);
  await expect(chat.locator('.cv-later > .mg-f')).toContainText("Queued");
  expect(await look(queued)).toEqual(await look(sent));
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-queued-note.png`, animations: "disabled" });
});

test("Desk handoff shows only the routing note in Desk and session chat", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const note = "Successor now runs Feature A's Desk. Please route Fix the header.";
  await page.evaluate((note) => {
    const s = sessionOf("lead");
    s.harness = "codex";
    const at_ms = Date.now();
    const context = "Desk lead: successor\nNeeds routing: #193\nOther session (sid:abcdef, 142k)";
    s.thread.push({ role: "you", at_ms, text: `<desk project="cue">\n${context}\n</desk>\n\nEarlier handoff` });
    s.thread.push({ role: "you", at_ms: at_ms + 1, text: `<desk id="feature-a" name="Feature A" project="cue">\n${context}\n</desk>\n\n${note}` });
    renderMain();
  }, note);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".cv-you-text").last()).toHaveText(note);
  await expect(chat).toContainText("Earlier handoff");
  await expect(chat).not.toContainText(/<desk|#193|abcdef|142k/);
  const deskComposer = page.locator(".dk-box textarea");
  await deskComposer.press("ArrowUp");
  await expect(deskComposer).toHaveValue(note);
  await deskComposer.evaluate((el) => el.setSelectionRange(0, 0));
  await deskComposer.press("ArrowUp");
  await expect(deskComposer).toHaveValue("Earlier handoff");
  await page.evaluate(() => openSession("lead"));
  const sessionChat = page.locator("[data-chat]");
  await expect(sessionChat.locator(".cv-you-text").last()).toHaveText(note);
  await expect(sessionChat).not.toContainText(/<desk|#193|abcdef|142k/);
  const sessionComposer = page.locator('textarea[data-text="s:lead"]');
  await sessionComposer.press("ArrowUp");
  await expect(sessionComposer).toHaveValue(note);
  await sessionComposer.evaluate((el) => el.setSelectionRange(0, 0));
  await sessionComposer.press("ArrowUp");
  await expect(sessionComposer).toHaveValue("Earlier handoff");
});

test("Cue's note on a lead's DONE it couldn't act on shows as one card from Cue, a line an item, without its words for the lead", async ({ page, cue }) => {
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
  const notes = page.locator('.dk-chat [data-mg="done"]').last();
  await expect(notes.locator(".mg-h")).toHaveText(/^Cue\s*NOT CLOSED\s*\d+s$/);
  await expect(notes.locator(".mg-line")).toHaveCount(2);
  await expect(notes.locator(".mg-line").nth(0)).toHaveText("Feature #d2 Quick phrases at the right of Send · it waits on your check");
  await expect(notes.locator(".mg-line").nth(1)).toHaveText("#zz9 · this Desk has no item #zz9");
  await expect(page.locator(".dk-chat")).not.toContainText("Don't say DONE");
  await notes.locator('[data-desk-chk-item="d2"]').click();
  await expect(page.locator('.dk-panel[data-side-for="d2"]')).toHaveCount(1);
});

test("an item's pane: its words select and copy, its buttons don't", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d2"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  const sel = (loc) => loc.evaluate((el) => getComputedStyle(el).webkitUserSelect || getComputedStyle(el).userSelect);
  expect(await sel(pane.locator("h1"))).toBe("text");
  expect(await sel(pane.locator(".dk-ph3"))).toBe("text");
  expect(await sel(pane.locator("button").first())).toBe("none");
  const h1 = pane.locator("h1"), box = await h1.boundingBox();
  // Along its first line: with its number in front, the title can wrap (#134).
  await page.mouse.move(box.x + 2, box.y + 14);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 2, box.y + 14, { steps: 5 });
  await page.mouse.up();
  expect((await page.evaluate(() => String(getSelection()))).length).toBeGreaterThan(3);
});

test("an interrupted assignment needs resume while a dependency stays queued", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').click();
  // The item's page and its column at the right (#418), the Items list hidden while it's open.
  const pane = page.locator(".dk-board");
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "host").state = "stopped";
    const items = st.desk.projects.find((d) => d.project === "cue").items;
    items.find((i) => i.id === "d5").state = "needs_resume";
    items.find((i) => i.id === "d4").after = "d5";
  });
  await expect(pane).toContainText("Needs resume");
  const rows = page.locator(".dk-items");
  await pane.locator(".dk-ph [data-desk-back]").click();
  await expect(rows.locator(".dk-gh", { hasText: "Needs resume" }).locator("+ .dk-gbody > .dk-gin > [data-desk-item='d5']:first-child")).toBeVisible();
  await expect(rows.locator("[data-desk-item='d4']")).toContainText("after #d5");
  await expect(page.locator('[data-desk-page="dispatcher"] .dk-psay')).toContainText("1 item needs to resume.");
  await expect(page.locator('[data-desk-page="dispatcher"] .dk-pfc')).toContainText("1 to resume");
  await rows.locator('[data-desk-item="d5"]').click();
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Resume");
  await expect(pane).toContainText("Resume wakes its session where it left off.");
  await pane.locator('[data-desk-act="start_now"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "start_now" });
  // A crew member's item: Resume only when that teammate isn't at work.
  await page.locator(".dk-back").click();
  await page.locator('[data-desk-item="d3"]').click();
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "lead").team.find((m) => m.agent_id === "id-search").state = "done";
    st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d3").state = "needs_resume";
  });
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Resume");
});

test("an item the lead paused shows as paused and keeps its holder (#365)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    st.sessions.find((s) => s.session_id === "host").state = "waiting";
    const d5 = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5");
    d5.state = "paused";
    d5.paused = [Math.floor(Date.now() / 1000), "#9 is urgent"];
  });
  const rows = page.locator(".dk-items");
  await expect(rows.locator(".dk-gh", { hasText: "Paused" }).locator("+ .dk-gbody > .dk-gin > [data-desk-item='d5']:first-child")).toBeVisible();
  await rows.locator('[data-desk-item="d5"]').click();
  const pane = page.locator(".dk-board");
  await expect(pane).toContainText("Paused");
  await expect(pane).toContainText("keeps its place. The Desk lead paused it: #9 is urgent.");
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveCount(0);
  await pane.locator(".dk-ph [data-desk-back]").click();
  await expect(page.locator('[data-desk-page="dispatcher"] .dk-psay')).toContainText("1 item is paused.");
});

test("a parked item sits in its own quiet group with a Start button; queued rows have Park (#385, #391)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    const it = d.items.find((i) => i.state === "new" || i.state === "queued") || d.items[0];
    Object.assign(it, { id: "h1", state: "held", holder: null, after: null, held: [Math.floor(Date.now() / 1000), "You", ""] });
    d.items.push({ id: "q7", project: "cue", kind: "task", title: "Tidy the footer", state: "queued", created: Date.now() / 1000, updated: Date.now() / 1000, holder: null, from: "you" });
  });
  const rows = page.locator(".dk-items");
  await expect(rows.locator(".dk-gh", { hasText: "Parked" })).toBeVisible();
  await expect(rows.locator(".dk-gh", { hasText: "On hold" })).toHaveCount(0);
  await expect(rows.locator(".dk-held [data-desk-item='h1']")).toBeVisible();
  await expect(rows.locator(".dk-held [data-desk-item='h1'] .dk-tag")).toHaveText("parked");
  await expect(rows.locator(".dk-gh.you + .dk-gbody > .dk-gin > [data-desk-item='h1']")).toHaveCount(0);
  await expect(rows.locator("[data-desk-item='h1'] [data-desk-act='start_now']")).toHaveText("Start");
  // A queued row has its own Park, right in the list; a press parks it, without opening or dragging it.
  const park = rows.locator("[data-desk-item='q7'] [data-desk-act='park']");
  await expect(park).toHaveText("Park");
  await park.click();
  await cue.called("desk_item_act", { id: "q7", act: "park" });
  await rows.locator('[data-desk-item="h1"]').click();
  const pane = page.locator(".dk-right .dk-pg");
  await expect(pane).toContainText("Parked");
  await expect(pane).toContainText("You parked it. Nothing starts it until you press Start.");
  await expect(pane.locator('[data-desk-act="start_now"]')).toHaveText("Start");
  await expect(pane.locator('[data-desk-act="park"]')).toHaveCount(0);
  // A queued item's page has Park too.
  await page.locator(".dk-back").click();
  await rows.locator('[data-desk-item="q7"] .dk-t').click();
  await expect(page.locator('.dk-panel[data-side-for] [data-desk-act="park"]')).toHaveText("Park");
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
  await page.locator(".dk-back").click();
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
  // Under its Files tab (#149), counted there.
  const ftab = page.locator('.dk-pane [data-desk-itab="files"]');
  await expect(ftab).toHaveText("Files2");
  await ftab.click();
  await expect(page.locator(".dk-pane .dk-thread")).toHaveCount(0);
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

test("the panel lists the files an item and a crew member changed, three then the rest, a click opening its diff (#313)", async ({ page, cue }) => {
  // A window wide enough for the panel to show beside the thread (#200).
  await page.setViewportSize({ width: 1512, height: 945 });
  await cue.open("desk");
  await openDesk(page);
  const files = ["ui/a.js", "ui/b.js", "ui/c.js", "ui/d.js", "ui/e.css"];
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/a.js", "ui/b.js", "ui/c.js", "ui/d.js", "ui/e.css"]; });
  await page.locator('.dk-items [data-desk-item="d2"] .dk-t').click();
  const sec = page.locator(".dk-panel .dk-pfiles");
  await expect(sec.locator("h4")).toHaveText("Files changed · 5");
  await expect(sec.locator(".ib-file code")).toHaveText(files.slice(0, 3));
  await expect(sec.locator(".ib-file").first()).toContainText("+2");
  // Said once: no Files line under Who as well.
  await expect(page.locator(".dk-panel .dk-pw", { hasText: "Files" })).toHaveCount(0);
  await sec.locator(".ib-file").nth(1).click();
  await expect(page.locator(".dov-fh .dov-p")).toHaveText("ui/b.js");
  await page.keyboard.press("Escape");
  await sec.locator(".ib-link").click();
  await expect(sec.locator(".ib-link")).toHaveCount(1);
  await expect(page.locator(".dov-fh .dov-p")).toHaveText("ui/d.js");
  await expect(page.locator(".dov-f")).toHaveCount(5);
  await page.keyboard.press("Escape");
  // A crew member: its diff so far, as its Diff reads it.
  await page.evaluate(() => deskShow({ kind: "holder", sid: "lead" }));
  await page.locator('.dk-right [data-desk-crew="id-search"]').click();
  await expect(sec.locator(".ib-file code")).toHaveText(["ui/header.js", "ui/styles/header.css"]);
  await sec.locator(".ib-file").nth(1).click();
  await expect(page.locator(".dov-fh .dov-p")).toHaveText("ui/styles/header.css");
  await expect(page.locator(".dov-title")).toHaveText("search-centre");
  await page.keyboard.press("Escape");
  // A session: what its turn changed, as Inbox's Changed this turn.
  const diff = "diff --git a/cache.ts b/cache.ts\n--- a/cache.ts\n+++ b/cache.ts\n@@ -1,2 +1,3 @@\n const key = sku;\n+const hit = cache.get(key);\n";
  await cue.override("diff_git", ({ op, against }, d) => (op === "toplevel" ? { code: 0, stdout: "/x\n" } : op === "diff" ? { code: 0, stdout: against === "abc123" ? d : "" } : { code: 0, stdout: "" }), diff);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").turn_from = { root: "/x", commit: "abc123", untracked: [] }; });
  await page.evaluate(() => deskShow({ kind: "chat", sid: "lead" }));
  await expect(sec.locator("h4")).toHaveText("Changed this turn · 1");
  await sec.locator(".ib-file").click();
  await expect(page.locator(".dov-fh .dov-p")).toHaveText("cache.ts");
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
  // The box emptied, the item's page still open.
  await expect(page.locator(".dk-box textarea")).toHaveValue("");
  await expect(page.locator(".dk-pane")).toHaveCount(1);
  // Off Waiting on you: back in the Items list, it's In progress (#418: the list is behind ‹ Items while it's open).
  await page.locator(".dk-back").click();
  await expect.poll(() => listOrder(page)).toEqual(["# Queue", "d4", "d6", "# Waiting on you", "d1", "# In progress", "d3", "d2", "d5", "# Done", "d7", "d8", "d9"]);
  // One that doesn't go: still waiting on you, red in its chat with why, Retry and Edit (#422).
  await page.locator('[data-desk-item="d1"]').click();
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d1").state = "to_check"; });
  await cue.override("desk_item_act", () => { throw "its session has ended"; });
  await page.locator(".dk-box textarea").fill("Still loses focus");
  await page.locator(".dk-box textarea").press("Enter");
  await expect(page.locator(".dk-right .dk-ipane .cv-you.msg-failed .msg-line")).toContainText("Couldn't send · its session has ended · Retry · Edit");
  await expect(page.locator(".dk-right .dk-pg h1")).toHaveText("Bug #d1 · Reply box loses focus after Send");
});
