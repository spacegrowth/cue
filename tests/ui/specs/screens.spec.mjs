// The main screens, each a screenshot compared with its baseline (shots.mjs). The clock is frozen at the
// scenarios' NOW and nothing animates (?still=1), so a screen draws the same pixels every run.
import { test, expect } from "./cue.mjs";
import { compareShot } from "./shots.mjs";
import { pillProblems } from "./pills.mjs";

// The smallest the window goes (tauri.conf.json minWidth; Cue has no phone layout of its own).
const NARROWEST = { width: 900, height: 600 };
const SETTINGS_TABS = ["general", "agents", "chat", "summaries", "voice", "insights", "actions", "usage"];

/** Opens a session's chat from its card in Waiting. */
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const openSettings = async (page, tab) => {
  await page.keyboard.press(",");
  await page.locator(`[data-act="set-tab"][data-tab="${tab}"]`).click();
  await expect(page.locator(`[data-act="set-tab"][data-tab="${tab}"]`)).toHaveAttribute("aria-selected", "true");
};
/** agent-relay's terminal open, its screen written, and nothing focused (a focused terminal's cursor blinks). */
const openTerminal = async (page) => {
  await openCard(page, "n1");
  await page.locator('.tstrip[data-sid="relay"]').click();
  // Its screen is in the terminal's buffer (the fake's permission menu); it's drawn by the time the shot is taken.
  await page.waitForFunction(() => {
    const b = typeof termObj !== "undefined" && termObj?.buffer.active;
    return b && Array.from({ length: b.length }, (_, i) => b.getLine(i)?.translateToString()).some((l) => l?.includes("Do you want to proceed?"));
  });
};

/** world-team's ask (hub, a teammate of the lead), and its lead's chat with the team open. */
const TEAM_ASK = "n1793984390004";
/** The teammates over the box, open or closed as `open` says (they open by themselves while one waits on you). */
const showTeam = async (page, open = true) => {
  const b = page.locator("[data-mt-open]").first();
  if ((await b.getAttribute("aria-expanded")) !== String(open)) await b.click();
};
const openTeam = async (page) => { await openCard(page, TEAM_ASK); await showTeam(page); };
/** Text dropped on `sel`, as a selection dragged there drops it. */
const dropOn = (page, sel, text) => page.evaluate(([sel, text]) => {
  const dt = new DataTransfer();
  dt.setData("text/plain", text);
  const el = document.querySelector(sel);
  for (const type of ["dragover", "drop"]) el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
}, [sel, text]);

/** name → { scenario, flags, viewport, setup(page): what to open, done(page): what's on screen once it's drawn } */
const SCREENS = {
  "board-empty": { scenario: "board-empty", done: (p) => p.locator(".switch-view").waitFor() },
  "board-waiting": { scenario: "board-waiting", done: (p) => p.locator('.nrow[data-big="n6"]').waitFor() },
  "many-sessions": { scenario: "many-sessions", done: (p) => p.locator('.nrow[data-big="mi0"]').waitFor() },
  "status-list": { scenario: "status-list", setup: async (p) => { await openCard(p, "n1"); await p.locator('.active-pane .st-bar[data-sid="relay"]').click(); }, done: (p) => p.locator(".active-pane .st-drawer").waitFor() },
  "chat-long-thread": { scenario: "board-waiting", setup: (p) => openCard(p, "n1"), done: (p) => p.locator(".active-pane pre").first().waitFor() },
  "terminal-open": { scenario: "terminal", setup: openTerminal, done: (p) => p.evaluate(() => document.activeElement?.blur()) },
  "terminal-stalled": { scenario: "terminal", flags: "stalled=1", setup: async (p) => { await openTerminal(p); await p.keyboard.type("ls"); }, done: async (p) => { await p.locator(".term-stalled").waitFor(); await p.evaluate(() => document.activeElement?.blur()); } },
  ...Object.fromEntries(SETTINGS_TABS.map((tab) => [`settings-${tab}`, { scenario: "board-waiting", setup: (p) => openSettings(p, tab), done: (p) => p.locator(".sheet .set-row, .sheet .set-group").first().waitFor() }])),
  "insights": { scenario: "board-waiting", setup: (p) => p.locator('[data-view="insights"]').click(), done: (p) => p.waitForFunction(() => CueBridge.called("insights_summary").some((c) => "result" in c)) },
  "shelf-parked": { scenario: "parked-ended", setup: (p) => p.locator('[data-shelf="parked"]').click(), done: (p) => p.locator('.prow.parked[data-session="guide"]').waitFor() },
  "shelf-closed": { scenario: "parked-ended", setup: (p) => p.locator('[data-shelf="closed"]').click(), done: (p) => p.locator('.prow.closed[data-session="lost2"]').waitFor() },
  "sessions-view": { scenario: "parked-ended", setup: async (p) => { await p.locator('[data-shelf="parked"]').click(); await p.locator('.sec-parked [data-lv="all"]').click(); }, done: (p) => p.locator('[data-sid="q2"]').first().waitFor() },
  // A hosted lead's team (docs/dev/cue-teams.md §4; world-team). team-closed: its chip on the title line. team-ask: the
  // team open, hub's row "needs you" and its card titled "cue › hub". team-open: that ask answered, every row in its
  // state. team-review: handoff's findings open. team-diff: skill's diff under its row. team-commit-main: Commit on docs
  // refused (its folder is on main), the reason under its row.
  "team-closed": { scenario: "world-team", setup: async (p) => { await openCard(p, TEAM_ASK); await showTeam(p, false); }, done: (p) => p.locator(".active-pane [data-mt-open]").waitFor() },
  "team-ask": { scenario: "world-team", setup: (p) => openTeam(p), done: async (p) => { await p.locator(".mt-row.need").waitFor(); await p.locator(".act-card .act-head .mt-of").waitFor(); } },
  "team-open": {
    scenario: "world-team",
    setup: async (p) => { await openCard(p, TEAM_ASK); await p.locator('.act-card [data-act="allow"]').click(); await showTeam(p); },
    done: async (p) => { await p.locator(".mt-row").nth(4).waitFor(); await expect(p.locator(".mt-row.need")).toHaveCount(0); },
  },
  "team-review": {
    scenario: "world-team",
    setup: async (p) => { await openTeam(p); await p.locator('[data-mt-fold="lead|id-handoff"]').click(); },
    done: (p) => p.locator(".mt-under .rv-k").first().waitFor(),
  },
  // team-diff: skill's Diff (one of its files in another repo, site) opens the lead's Diff tab on its files, each
  // labelled with its repo. session-diff: a session's Diff tab, its count on the tab.
  "team-diff": {
    scenario: "world-team",
    setup: async (p) => {
      await openTeam(p);
      await p.evaluate(() => { CueBridge.world.session("lead").team.find((t) => t.name === "skill").files.push("/work/site/docs/team.md"); CueBridge.world.changed(0); });
      await p.locator(".mt-row", { hasText: "3 files" }).waitFor();
      await p.locator('[data-mt-act="diff"][data-mt-agent="id-skill"]').click();
    },
    done: async (p) => { await p.locator(".dv-chip", { hasText: "skill's 3 files" }).waitFor(); await p.locator(".dv-repo", { hasText: "site" }).waitFor(); },
  },
  "session-diff": {
    scenario: "board-waiting",
    flags: "diff=1",
    setup: async (p) => { await openCard(p, "n1"); await p.locator('[data-stab=":diff"]', { hasText: "2 +6 −1" }).click(); },
    done: (p) => p.locator(".dv-file[open] .dv-lines .a", { hasText: "fmt(total)" }).waitFor(),
  },
  "team-commit-main": {
    scenario: "world-team",
    // docs's review says commit with a note, so its row offers Send fixes; Commit anyway is under ⋯.
    setup: async (p) => { await openTeam(p); await p.locator('[data-mt-more="lead|id-docs"]').click(); await p.locator('.mt-menu [data-mt-act="commit"][data-mt-agent="id-docs"]').click(); },
    done: async (p) => { await p.locator(".mt-note", { hasText: "is on main" }).waitFor(); await p.locator('.mt-note [data-mt-act="branch"]').waitFor(); },
  },
  // How work goes out to the crew (ui/team_send.js). team-switch: the Crew menu open at the top right, split on Always.
  // team-handout: a task dropped on the lead's card under Sessions, one of the files it names already rows's.
  "team-switch": {
    scenario: "world-team",
    setup: async (p) => { await openCard(p, TEAM_ASK); await p.locator('.act-card [data-act="allow"]').click(); await p.locator('[data-crew-menu="lead"]').click(); await p.locator('[data-team-sw="lead"][data-team-mode="on"]').click(); },
    done: async (p) => { await expect(p.locator(".team-sw button.on").first()).toHaveText("Always"); await p.waitForFunction(() => CueBridge.called("team_mode").some((c) => c.args?.mode === "on")); },
  },
  "team-handout": {
    scenario: "world-team",
    setup: async (p) => { await openCard(p, TEAM_ASK); await p.locator('.act-card [data-act="allow"]').click(); await p.locator(".working[data-session='lead']").waitFor(); await dropOn(p, ".working[data-session='lead']", "Show the team's spend on the lead's card: ui/team.js and ui/board.js"); },
    done: (p) => p.locator(".ho .ho-by", { hasText: "rows is changing it" }).waitFor(),
  },
  // Gone quiet (src-tauri/src/sessions/quiet.rs). team-quiet: hub stuck 20 minutes (Resume / Stop), rows 20 minutes into
  // its test run (left to it). agents-quiet: names' background agents, h4 stuck, h7 in a long run.
  "team-quiet": {
    scenario: "world-team",
    setup: async (p) => {
      await openCard(p, TEAM_ASK);
      await p.locator('.act-card [data-act="allow"]').click();
      await showTeam(p);
      await p.evaluate((ago) => {
        const st = CueBridge.world.state, lead = st.sessions.find((s) => s.session_id === "lead"), at = (n) => lead.team.find((t) => t.name === n);
        Object.assign(at("hub"), { quiet: "stuck", step_ms: st.now_ms - ago });
        Object.assign(at("rows"), { quiet: "tool", step_ms: st.now_ms - ago });
        CueBridge.emit("state", CueBridge.world.snapshot());
      }, 20 * 60_000);
    },
    done: (p) => p.locator(".mt-line", { hasText: "looks stuck" }).waitFor(),
  },
  "agents-quiet": {
    scenario: "board-waiting",
    flags: "many=1",
    setup: (p) => p.evaluate(() => openSession("names")),
    // Its row, and the board's columns done scrolling to its card (opening a session scrolls them there).
    done: async (p) => {
      await p.locator('.ag-row[data-key="h:h4"] [data-ag-wake]').first().waitFor();
      await p.waitForFunction(() => {
        const at = [...document.querySelectorAll("*")].filter((e) => e.scrollHeight > e.clientHeight + 1).map((e) => e.scrollTop).join();
        const same = window.__scrollWas === at;
        window.__scrollWas = at;
        return same;
      }, null, { polling: 150 });
    },
  },
  "narrowest-window": { scenario: "board-waiting", viewport: NARROWEST, done: (p) => p.locator('[data-big="n6"]').first().waitFor() },
};

for (const [name, s] of Object.entries(SCREENS)) {
  test(`screen: ${name}`, async ({ cue, page }) => {
    if (s.viewport) await page.setViewportSize(s.viewport);
    await cue.open(s.scenario, { flags: s.flags, frozen: true });
    await s.setup?.(page);
    await s.done(page);
    // Nothing under the pointer (where the last click left it, a row can show its hover look or not, depending on
    // when the layout moved under it); fonts in; two frames drawn since the last change (xterm draws on a frame).
    await page.mouse.move(0, 0);
    await page.evaluate(() => document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))));
    const png = await page.screenshot({ animations: "disabled", caret: "hide" });
    const r = compareShot(name, png);
    expect(r.baseline, `baseline for ${name}`).not.toBe("missing");
    // Every small label on it is a full capsule (pills.mjs), so a new one can't come in with corners of its own.
    expect(await pillProblems(page, "body"), `${name}: small labels that aren't a full capsule`).toEqual([]);
    expect(r.differ, `${name}: pixels that differ from tests/ui/baseline/${name}.png (see tests/ui/shots/${name}.diff.png)`).toBe(0);
  });
}
