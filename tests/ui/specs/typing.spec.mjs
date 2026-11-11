// Typing to sessions: a reply to a finished turn, a message to a working one (kept in Cue until its turn ends,
// sent now with ⌘↵, taken back with Esc), quick phrases, "!" lines, and drafts.
import { test, expect } from "./cue.mjs";

/** A session's message box in the Active pane. */
const box = (page, sid) => page.locator(`.active-pane textarea[data-text="s:${sid}"]`);
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
/** A working session's chat, from its card under Sessions. */
const openWorking = (page, sid) => page.locator(`.working[data-session="${sid}"]`).first().click();

test.beforeEach(async ({ cue }) => { await cue.open("board-waiting"); });

test("Enter in a finished turn's box replies with what's typed", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("carry it over to the successor");
  await box(page, "relay").press("Enter");
  const c = await cue.called("reply", { id: "n1" });
  expect(c.args.text).toBe("carry it over to the successor");
  await expect(page.locator('.nrow[data-big="n1"]')).toHaveCount(0);
  await expect(box(page, "relay")).toHaveValue("");
});

test("A finished turn's box says where the reply goes: typed into its terminal, or straight to a hosted session", async ({ cue, page }) => {
  await openCard(page, "n1");
  await expect(box(page, "relay")).toHaveAttribute("placeholder", "Reply to agent-relay (typed into its terminal)…");
  await cue.change((st) => { st.sessions.find((s) => s.session_id === "relay").hosted = true; });
  await expect(box(page, "relay")).toHaveAttribute("placeholder", "Reply to agent-relay…");
});

test("A hosted Claude lead's Crew settings are at the top right, never in its reply box; a terminal session has none", async ({ cue, page }) => {
  await openCard(page, "n1");
  await expect(page.locator(".active-pane [data-crew-menu]")).toHaveCount(0);
  await cue.change((st) => { Object.assign(st.sessions.find((s) => s.session_id === "relay"), { hosted: true, harness: "claude" }); });
  await expect(box(page, "relay")).toHaveAttribute("placeholder", "Reply to agent-relay…");
  await expect(page.locator(".active-pane .foot .team-sw")).toHaveCount(0);
  await page.locator('.ap-head [data-crew-menu="relay"]').click();
  await page.locator('.active-pane [data-team-sw="relay"][data-team-mode="on"]').click();
  expect((await cue.called("team_mode", { sessionId: "relay" })).args.mode).toBe("on");
});

test("Send replies, and Shift+Enter keeps the lines together", async ({ cue, page }) => {
  await openCard(page, "n2");
  await box(page, "det").pressSequentially("tune the stop");
  await box(page, "det").press("Shift+Enter");
  await box(page, "det").pressSequentially("then rerun");
  expect(await cue.calls("reply")).toEqual([]);
  await page.locator(".active-pane .btn.send").click();
  expect((await cue.called("reply", { id: "n2" })).args.text).toBe("tune the stop\nthen rerun");
});

test("Nothing typed: Enter sends nothing", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").press("Enter");
  await expect(page.locator(".active-pane .btn.send")).toBeDisabled();
  expect(await cue.calls("reply")).toEqual([]);
});

test("A quick phrase replies with its words", async ({ cue, page }) => {
  await openCard(page, "n1");
  await page.locator('.active-pane .qp[data-phrase="Go ahead"]').click();
  expect((await cue.called("reply", { id: "n1" })).args.text).toBe("Go ahead");
});

test("To a working session: kept in Cue until its turn ends", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially("also keep the old key valid for a day");
  await box(page, "eng").press("Enter");
  const c = await cue.called("send_to_session", { sessionId: "eng" });
  expect(c.args).toMatchObject({ text: "also keep the old key valid for a day", now: false });
  expect(c.result).toMatch(/^kept in Cue/);
  await expect(page.locator(".active-pane")).toContainText("also keep the old key valid for a day");
});

test("⌘↵ sends to a working session now", async ({ cue, page }) => {
  await openWorking(page, "alerts");
  await box(page, "alerts").pressSequentially("stop, check the retry count first");
  await box(page, "alerts").press("Meta+Enter");
  expect((await cue.called("send_to_session", { sessionId: "alerts" })).args).toMatchObject({ text: "stop, check the retry count first", now: true });
});

test("Esc takes a kept message back into the box", async ({ cue, page }) => {
  await openWorking(page, "eng");
  await box(page, "eng").pressSequentially("use the new pool");
  await box(page, "eng").press("Enter");
  await cue.called("send_to_session", { sessionId: "eng" });
  await expect(page.locator(".active-pane")).toContainText("use the new pool");
  await page.keyboard.press("Escape");
  await cue.called("unhold_message", { sessionId: "eng" });
  await expect(box(page, "eng")).toHaveValue("use the new pool");
});

test("A \"! \" line runs in the session's folder, not sent to it", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("! git status");
  await box(page, "relay").press("Enter");
  expect((await cue.called("run_line", { sessionId: "relay" })).args.line).toBe("git status");
  await expect(page.locator(".active-pane .ran-out")).toContainText("nothing to commit, working tree clean");
  expect(await cue.calls("reply")).toEqual([]);
});

test("\"!\" with no space after it is a message", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("!important: keep the footer");
  await box(page, "relay").press("Enter");
  expect((await cue.called("reply", { id: "n1" })).args.text).toBe("!important: keep the footer");
  expect(await cue.calls("run_line")).toEqual([]);
});

test("What's typed is kept as the box's draft", async ({ cue, page }) => {
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("half-written");
  await cue.called("set_draft", { key: "s:relay", text: "half-written" });
});

test("Typing in a box doesn't trigger the board's keys", async ({ cue, page }) => {
  await openCard(page, "n4");
  await page.keyboard.press("r");
  await page.locator('.active-pane textarea[data-text="n4"]').pressSequentially("a d h");
  expect(await cue.calls("respond")).toEqual([]);
  await expect(page.locator('[data-view="board"]')).toHaveClass(/on/);
});

const hosted = (cue) => cue.change((st) => { st.sessions.find((x) => x.session_id === "relay").hosted = true; });
async function openMcpOf(cue, page) {
  await hosted(cue);
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("/mcp");
  await box(page, "relay").press("Enter");
}
/** A server's row in the list, with the state line under it. */
const row = (page, name) => page.locator(".mcp-pick .mcpl-it", { hasText: name });
const state = (page, name) => row(page, name).locator("xpath=following-sibling::div[contains(@class,'mcpl-sub')][1]");

test("/mcp in a hosted Claude Code session shows its servers as Claude Code has them, and nothing is sent", async ({ cue, page }) => {
  await openMcpOf(cue, page);
  await cue.called("mcp_live", { sessionId: "relay" });
  await expect(box(page, "relay")).toHaveValue("");
  expect([...await cue.calls("reply"), ...await cue.calls("send_message")]).toEqual([]);
  await expect(state(page, "Linear")).toContainText("connected");
  await expect(state(page, "Linear")).toContainText("2 tools");
  await expect(state(page, "claude.ai Gmail")).toContainText("needs sign-in");
  await expect(state(page, "data-tools")).toContainText("didn't connect");
  await expect(page.locator(".mcp-pick .mcpl-why")).toHaveText("spawn uv ENOENT");
  await expect(state(page, "sentry")).toContainText("off");
  expect(await cue.calls("mcp_for"), "not the old list from its settings").toEqual([]);
});

test("Sign in opens the server's page in the browser and asks again until it's connected", async ({ cue, page }) => {
  await openMcpOf(cue, page);
  await row(page, "claude.ai Gmail").getByRole("button", { name: "Sign in" }).click();
  await cue.called("mcp_act", { sessionId: "relay", server: "claude.ai Gmail", action: "sign-in" });
  await cue.called("open_link", { url: "https://auth.example/claude.ai%20Gmail" });
  await expect(state(page, "claude.ai Gmail")).toContainText("finish signing in in your browser");
  await expect(page.locator(".mcpl-paste")).toHaveCount(0);
  // Signed in, as the browser finishes it: the next ask shows it connected.
  await page.evaluate(() => CueBridge.override("mcp_live", () => ({ servers: [{ name: "claude.ai Gmail", status: "connected", tools: ["search"], can_sign_out: false }] })));
  await page.clock.runFor(3000);
  await expect(state(page, "claude.ai Gmail")).toContainText("connected");
});

test("On another machine, the address the browser ended on is pasted back", async ({ cue, page }) => {
  await cue.open("board-waiting", { flags: "machine=1" });
  await openMcpOf(cue, page);
  await row(page, "claude.ai Gmail").getByRole("button", { name: "Sign in" }).click();
  const paste = page.locator(".mcpl-paste input");
  await paste.fill("http://localhost:4567/callback?code=abc&state=x");
  await page.locator(".mcpl-paste").getByRole("button", { name: "Done" }).click();
  await cue.called("mcp_signed_in", { sessionId: "relay", server: "claude.ai Gmail", url: "http://localhost:4567/callback?code=abc&state=x" });
});

test("A server turns off and on at once, and one that failed reconnects, with no restart", async ({ cue, page }) => {
  await openMcpOf(cue, page);
  await row(page, "Linear").locator("input[type=checkbox]").uncheck();
  await cue.called("mcp_act", { sessionId: "relay", server: "linear", action: "off" });
  await expect(state(page, "Linear")).toContainText("off");
  await row(page, "data-tools").getByRole("button", { name: "Reconnect" }).click();
  await cue.called("mcp_act", { server: "data-tools", action: "reconnect" });
  await expect(state(page, "data-tools")).toContainText("connected");
  expect(await cue.calls("restart_session")).toEqual([]);
});

test("⋯ → MCP servers in a hosted session counts what Claude Code has, and opens the same list", async ({ cue, page }) => {
  await hosted(cue);
  await openCard(page, "n1");
  await page.locator(".active-pane .more-btn").click();
  const item = page.locator(".active-pane [data-mcp-ask]");
  await expect(item).toContainText("MCP servers · 1 connected · 1 need sign-in · 1 didn't connect");
  await expect(item).toContainText("nothing restarts");
  await item.click();
  await expect(state(page, "claude.ai Gmail")).toContainText("needs sign-in");
  expect(await cue.calls("mcp_for")).toEqual([]);
});

test("⋯ → MCP servers is there for a hosted session on another machine too", async ({ cue, page }) => {
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "relay"); s.hosted = true; s.machine = "build-box"; });
  await openCard(page, "n1");
  await page.locator(".active-pane .more-btn").click();
  await expect(page.locator(".active-pane [data-mcp-ask]")).toContainText("MCP servers");
});

test("A session that runs only servers picked for it says so, and can have all of its folder's again", async ({ cue, page }) => {
  await cue.open("board-waiting", { flags: "pinned=1" });
  await openMcpOf(cue, page);
  await expect(page.locator(".mcp-pick")).toContainText("It runs only the servers picked for it");
  await page.locator(".mcp-pick").getByRole("button", { name: "Use all of the folder's servers" }).click();
  await cue.called("mcp_apply_clear", { sessionId: "relay" });
});

test("⋯ → Move to terminal is there for a hosted session, says it's for moving away, and isn't a button by the box", async ({ cue, page }) => {
  await hosted(cue);
  await openCard(page, "n1");
  await expect(page.locator(".active-pane .tplace [data-act=move-term]")).toHaveCount(0);
  await page.locator(".active-pane .more-btn").click();
  const item = page.locator(".active-pane .more-menu [data-act=move-term]");
  await expect(item).toContainText("Only if you're moving away from Cue");
  await expect(item).toContainText("tab, and Cue stops running it");
  await item.click();
  await cue.called("move_to_terminal", { sessionId: "relay" });
  await expect(page.locator(".active-pane .more-menu")).toHaveCount(0);
});

test("⋯ → Move to terminal waits while the session works", async ({ cue, page }) => {
  await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "relay"); s.hosted = true; s.state = "working"; });
  await openCard(page, "n1");
  await page.locator(".active-pane .more-btn").click();
  const item = page.locator(".active-pane .more-menu [data-act=move-term]");
  await expect(item).toBeDisabled();
  await expect(item).toContainText("When this turn ends");
});

test("One of Claude Code's own screens, not sent from a hosted session: Run it opens Cue's terminal in its folder, running the line", async ({ cue, page }) => {
  await hosted(cue);
  const notSent = () => "not sent: /design-login is one of Claude Code's own screens, which only its terminal shows. To use it, run claude /design-login in a terminal in ~/code/agent-relay";
  await cue.override("send_to_session", notSent);
  await cue.override("reply", notSent);
  await openCard(page, "n1");
  await box(page, "relay").pressSequentially("/design-login");
  await box(page, "relay").press("Enter");
  await page.locator(".active-pane .run-here").click();
  const cwd = (await cue.called("shell_open")).args.cwd;
  expect(cwd).toBe("/Users/me/code/agent-relay");
  const typed = await cue.called("term_write", { data: "claude /design-login\r" });
  expect(typed.args.sessionId).toMatch(/^shell:/);
});
