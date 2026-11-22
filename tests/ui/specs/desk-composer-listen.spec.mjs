// The Desk lead's box and Listen / Read are the Board's (#688): the session chat's composer (two lines, Queue while the
// lead works, the "/" menu, Send at), Listen's line over the box while a reply is read out, the Board's reader, and
// Listen and Read under a reply where a session's chat has them.
import { test, expect, openDesk, deskShowStrip } from "./cue.mjs";

const LONG = "Here is what I found in the footer code. ".repeat(3) + "\n\n" + "The layout breaks because the grid has no minimum width, so the columns collapse on narrow windows and the links wrap under each other. ".repeat(4) + "\n\n" + "I changed the grid to keep a minimum width and let the links wrap as a row instead. Tests pass and the screenshots look right. ".repeat(2);
/** Questions and long replies (long enough for Listen and Read, and to fill the chat) at the end of session `sid`'s chat. */
const reply = (st, a) => { const s = st.sessions.find((x) => x.session_id === a.sid); s.thread = [...(s.thread || []), ...[3, 2, 1].flatMap((n) => [{ role: "you", text: "What's wrong with the footer?", at_ms: Date.now() - n * 60000 }, { role: "agent", text: a.LONG, at_ms: Date.now() - n * 60000 + 30000 }])]; };
const working = (st, sid) => Object.assign(st.sessions.find((x) => x.session_id === sid), { state: "working", doing: "Reading footer.js…", doing_ms: Date.now(), since_ms: Date.now() });
/** Where things sit in a chat card (`card`): the box's, Listen's and the reply's, against the card's edges. */
const layout = (card) => card.evaluate((c) => {
  const cr = c.getBoundingClientRect(), r = (sel) => c.querySelector(sel)?.getBoundingClientRect();
  const foot = [...c.querySelectorAll(".foot")].pop().getBoundingClientRect(), ls = [...c.querySelectorAll("[data-listen]")].pop().getBoundingClientRect();
  const ta = [...c.querySelectorAll(".foot textarea")].pop();
  const chat = c.querySelector(".ap-chat, .dk-chat"), msg = [...c.querySelectorAll(".cv-agent .msg")].pop().getBoundingClientRect();
  chat.scrollTop = chat.scrollHeight;
  const ls2 = [...c.querySelectorAll("[data-listen]")].pop().getBoundingClientRect();
  return { footH: Math.round(foot.height), footFromBottom: Math.round(cr.bottom - foot.bottom), footLeft: Math.round(foot.left - cr.left), rows: +ta.getAttribute("rows"),
    listenLeft: Math.round(ls.left - cr.left), listenUnderMsg: Math.round(ls.top - msg.bottom), listenAtEndAboveFoot: Math.round(foot.top - ls2.bottom), field: !!r(".foot .composer .field") };
});
const boardCard = (page) => page.locator(".active-pane");
const deskCard = (page) => page.locator(".dk2b-chat");

test("the lead's box is the Board's composer: two lines, Queue while it works, the / menu and Send at (#688)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change(working, "lead");
  await openDesk(page);
  const card = deskCard(page);
  const box = card.locator(".foot .composer textarea");
  await expect(box).toHaveAttribute("rows", "2");
  await expect(card.locator(".foot .btn.send")).toHaveText("Queue");
  await expect(card.locator(".dk-box")).toHaveCount(0);
  // The "/" menu: the lead's commands, picked as in a session's box.
  await box.click();
  await page.keyboard.type("/comp");
  await expect(card.locator(".cmd-menu .cmd-row").first()).toContainText("/compact");
  await page.keyboard.press("Tab");
  await expect(box).toHaveValue("/compact ");
  await page.keyboard.press("Escape");
  await box.fill("");
  // Send at: the lead's session gets it then.
  await box.fill("Check the footer tomorrow");
  await card.locator("[data-send-at-open]").click();
  await card.locator(".send-at-bar [data-send-at-go]").click();
  const at = await cue.called("send_at", { sessionId: "lead" });
  expect(at.args.text).toBe("Check the footer tomorrow");
  await expect(box).toHaveValue("");
  // Queue sends through the Desk, kept while the lead works; idle, it says Send.
  await box.fill("Look at the header next");
  await card.locator(".foot .btn.send").click();
  await cue.called("desk_send", { project: "cue", text: "Look at the header next" });
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").state = "waiting"; });
  await expect(card.locator(".foot .btn.send")).toHaveText("Send");
});

test("the lead's box, Listen and Read sit where a session's chat on the Board has them (#688)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await cue.open("board-waiting");
  await cue.change(reply, { sid: "eng", LONG });
  await page.locator('.working[data-session="eng"]').first().click();
  await boardCard(page).locator("[data-listen]").first().waitFor();
  const board = await layout(boardCard(page));
  await cue.open("desk");
  await cue.change(reply, { sid: "lead", LONG });
  await cue.change(working, "lead");
  await openDesk(page);
  // As wide as the Board's chat: the panel folded to the strip (B2 opens with it shown beside the chat, #666).
  await deskShowStrip(page);
  await deskCard(page).locator("[data-listen]").first().waitFor();
  const desk = await layout(deskCard(page));
  expect(desk).toEqual(board);
});

test("Listen on a lead's reply shows the Listening line over its box; Read opens the Board's reader (#688)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await cue.open("board-waiting");
  await cue.change(reply, { sid: "eng", LONG });
  await page.locator('.working[data-session="eng"]').first().click();
  await boardCard(page).locator("[data-read]").last().click();
  const rd = (card) => card.evaluate((c) => { const cr = c.getBoundingClientRect(), t = c.querySelector(".rd .msg").getBoundingClientRect(), h = c.querySelector(".rd-head").getBoundingClientRect(); return { textLeft: Math.round(t.left - cr.left), textW: Math.round(t.width), headH: Math.round(h.height) }; });
  const boardRd = await rd(boardCard(page));
  await cue.open("desk");
  await cue.change(reply, { sid: "lead", LONG });
  await openDesk(page);
  const card = deskCard(page);
  await card.locator("[data-listen]").last().click();
  const line = card.locator(".ls-line");
  await expect(line).toContainText("Listening");
  // Over the box, as on the Board.
  expect(await line.evaluate((el) => el.nextElementSibling?.matches(".foot"))).toBe(true);
  await line.locator("[data-listen-stop]").click();
  await expect(line).toHaveCount(0);
  await card.locator("[data-read]").last().click();
  await expect(card.locator(".rd-head")).toBeVisible();
  expect(await rd(card)).toEqual(boardRd);
});

test("a / command or a ! line in the lead's box runs as one, as in a session's box: never sent through the Desk (#688)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").state = "waiting"; });
  await openDesk(page);
  const box = deskCard(page).locator(".foot .composer textarea");
  // "/compact": to the lead's session as it was typed, without the Desk's summary before it.
  await box.fill("/compact");
  await box.press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "/compact" });
  await expect(box).toHaveValue("");
  // "! ls": Cue runs it in the lead's folder, as the Board does; the lead never gets it.
  await box.fill("! ls");
  await box.press("Enter");
  await cue.called("run_line", { sessionId: "lead", line: "ls" });
  await expect(box).toHaveValue("");
  // Plain words still go through the Desk.
  await box.fill("Look at the header next");
  await box.press("Enter");
  await cue.called("desk_send", { project: "cue", text: "Look at the header next" });
  expect((await cue.calls("desk_send")).map((c) => c.args.text)).toEqual(["Look at the header next"]);
  expect((await cue.calls("send_to_session")).map((c) => c.args.text)).toEqual(["/compact"]);
});
