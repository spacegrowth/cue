// A mocks item waiting for you to check lists its options, A, B… or M1, M2…, each with what it is and a Pick (#445).
// A pick closes the mocks item and makes its build item for the lead to route; its chat is for comments on the mocks.
import { test, expect, openDesk } from "./cue.mjs";

const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);
const items = (page) => page.evaluate(() => CueBridge.world.state.desk.projects.find((x) => x.project === "cue").items);

test("a mocks-only item lists its options M1, M2 with a Pick each; a pick closes it and makes its build item (#445)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Mocks only (#408, #436): no DESIGNS: line, the canvas is the link its DONE made.
  const text = "Two ways on the canvas: https://claude.ai/artifact/MOCKS\n- **M1**: a strip under the header\n- **M2** — a card in the list\nPick one: M1 or M2.";
  await patchItem(cue, "d5", { state: "to_check", title: "Mocks: working items", designs: "", made: ["https://claude.ai/artifact/MOCKS"], finish: { step: "checked", at: Date.now(), mocks: true }, said: [{ at: Date.now(), by: "host-lead", text }] });
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const head = page.locator(".dk-panel[data-side-for] .dk-pnow");
  const opts = head.locator(".dk-opts li");
  await expect(opts).toHaveCount(2);
  await expect(opts.locator(".dk-optk")).toHaveText(["M1", "M2"]);
  await expect(opts.locator(".dk-optw")).toHaveText(["a strip under the header", "a card in the list"]);
  await expect(opts.locator('[data-desk-act="pick"]')).toHaveText(["Pick", "Pick"]);
  // Its line says where the mocks are and what to pick, not the whole reply.
  await expect(head.locator(".dk-pickl")).toHaveText("Mocks · open the canvas · Pick one: M1 or M2.");
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  // Its chat is for comments on the mocks; the pick is the button.
  await expect(page.locator(".dk-right .dk-box textarea")).toHaveAttribute("placeholder", /^Comments… \(Pick an option above/);
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });   // as the Mac switches it: a redraw puts Settings → Appearance (system) back over a data-theme set by hand
    // The theme redraws the page: again once it's settled.
    if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-panel[data-side-for]").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-445-${theme}.png`, timeout: 2000 })).toPass();
  }
  await opts.nth(1).locator('[data-desk-act="pick"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "pick", text: "M2" });
  await expect(head.locator(".dk-pword")).toHaveText("Done");
  await expect(head.locator(".dk-opts")).toHaveCount(0);
  const n = (await items(page)).find((i) => i.id === "d5").built_as;
  await expect(head.locator(".dk-built")).toHaveText(`You picked M2 on #d5; #${n} builds it`);
  expect((await items(page)).find((i) => i.id === n).title).toBe("Build: working items (option M2)");
});

test("options listed one to a line are picked from even with no pick line; one option alone isn't a list (#445)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d5", { state: "to_check", designs: "https://claude.ai/artifact/MOCKS", said: [{ at: Date.now(), by: "host-lead", text: "On the canvas:\nA: rows\nB) cards\nC — a table" }] });
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const opts = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts li");
  await expect(opts.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
  await expect(opts.locator(".dk-optw")).toHaveText(["rows", "cards", "a table"]);
  await patchItem(cue, "d5", { said: [{ at: Date.now(), by: "host-lead", text: "A: the one way, on the canvas." }] });
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts")).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-box textarea")).not.toHaveAttribute("placeholder", /^Comments… \(Pick an option above/);
});

// #539: an item whose reply marks one option suggested ("**B (suggested):**") and asks "Which option do you want
// built: A, B or C?" (no "pick" in it) lists all three, the suggested one too, in its panel and under that reply in
// its chat, where a Pick works as the panel's does.
test("three options with the middle one suggested: all three get a Pick, in the panel and under the question in the chat (#539)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const text = [
    "The plan offers three options:",
    "- **A, heads-up only:** a lead sends a note to the other Desks' leads.",
    "- **B, holds with heads-up (suggested):** a big item holds the files it will change.",
    "- **C, Cue spots overlaps itself:** Cue compares what each item has changed.",
    "",
    "- **Canvas page:** seven boards, all Mac.",
    "  - **A:** the heads-up note.",
    "  - **B (suggested):** both leads' chats and your Desk page.",
    "  - **C:** the warning Cue sends both leads.",
    "- **Ideas row:** added, with the status \"Mocks, waiting for a pick\".",
    "",
    "Which option do you want built: A, B or C?",
  ].join("\n");
  const at = Date.now() - 5000;
  await cue.change((st, [text, at]) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", title: "Plan and mocks: Desks on the same code", designs: "https://claude.ai/artifact/MOCKS", said: [{ at, by: "host-lead", text }] });
    st.sessions.find((x) => x.session_id === "host").thread = [{ role: "you", text: "About #d5 (Plan and mocks): go on", at_ms: at - 60000 }, { role: "agent", text: "Reading the code first.", at_ms: at - 30000 }, { role: "agent", text, at_ms: at }];
  }, [text, at]);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const side = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts li");
  await expect(side.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
  await expect(side.locator(".dk-optw")).toHaveText(["heads-up only: a lead sends a note to the other Desks' leads.", /^holds with heads-up \(suggested\): /, /^Cue spots overlaps itself: /]);
  await expect(side.locator('[data-desk-act="pick"]')).toHaveText(["Pick", "Pick", "Pick"]);
  await expect(page.locator(".dk-panel[data-side-for] .dk-pickl")).toContainText("Which option do you want built: A, B or C?");
  // In the chat: under the reply that asks, not the one before it.
  const chat = page.locator(".dk-right .dk-ichat");
  await expect(chat.locator(".dk-chat-opts")).toHaveCount(1);
  const asks = chat.locator(".cv-agent").filter({ hasText: "Which option do you want built" });
  const inChat = asks.locator(".dk-chat-opts li");
  await expect(inChat.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
  await expect(inChat.locator('[data-desk-act="pick"]')).toHaveText(["Pick", "Pick", "Pick"]);
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: theme });   // as the Mac switches it: a redraw puts Settings → Appearance (system) back over a data-theme set by hand
    if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-539-${theme}.png`, timeout: 2000 })).toPass();
  }
  await inChat.nth(1).locator('[data-desk-act="pick"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "pick", text: "B" });
  await expect(chat.locator(".dk-chat-opts")).toHaveCount(0);
  await expect(page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts")).toHaveCount(0);
});

test("options one to a line with the middle one marked suggested and no pick line: all three in both places (#539)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const text = "On the canvas:\n- **A:** rows\n- **B (suggested):** cards\n- **C:** a table", at = Date.now() - 5000;
  await cue.change((st, [text, at]) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", designs: "https://claude.ai/artifact/MOCKS", said: [{ at, by: "host-lead", text }] });
    st.sessions.find((x) => x.session_id === "host").thread = [{ role: "agent", text, at_ms: at }];
  }, [text, at]);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  for (const opts of [page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts li"), page.locator(".dk-right .dk-ichat .dk-chat-opts li")]) {
    await expect(opts.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
    await expect(opts.locator(".dk-optw")).toHaveText(["rows", "cards (suggested)", "a table"]);
    await expect(opts.locator('[data-desk-act="pick"]')).toHaveCount(3);
  }
});

// #619: a mocks item sent back and redone, its holder asking again with numbered options (4, 6, 8, 9) and IF PICKED:
// lines for them: the page, the chat and the Desk row show the new options with their names, A/B/C go, and a Pick
// files that option's plan.
test("a redone mocks item asks again with numbered options: the new ones replace A, B, C everywhere (#619)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const plan = (option, title) => ({ option, kind: "feature", title, see: "", done_when: "", by: "new" });
  const first = "Three on the canvas:\n- **A:** a needs-you band\n- **B:** lanes\n- **C:** a track\nPick one: A, B or C.", at = Date.now() - 60000;
  await cue.change((st, [text, at, next]) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", title: "Mocks: the Desk page", designs: "https://claude.ai/artifact/MOCKS", next, said: [{ at, by: "host-lead", text }] });
    st.sessions.find((x) => x.session_id === "host").thread = [{ role: "agent", text, at_ms: at }];
  }, [first, at, [plan("A", "Old band"), plan("B", "Old lanes"), plan("C", "Old track")]]);
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const side = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts li");
  await expect(side.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
  // Sent back: the old round's plans go with it.
  await page.locator('.dk-panel[data-side-for] [data-desk-act="send_back"]').click();
  await page.locator('textarea[data-text="back:d5"]').fill("Not these: start again");
  await page.locator('[data-desk-act="back_go"][data-id="d5"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "send_back", text: "Not these: start again" });
  expect((await items(page)).find((i) => i.id === "d5").next).toEqual([]);
  // Redone: it asks again, numbered, with what follows each.
  const again = "Four new mocks are on the canvas, each its own page.\n\nWhich option, 4, 6, 8 or 9, should we build?", at2 = Date.now();
  const next = [plan("9", "A calm list with progress under each row"), plan("6", "Progress is the page"), plan("8", "A List / Progress switch"), plan("4", "The picked row opens its journey")];
  await cue.change((st, [text, at, next]) => {
    Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5"), { state: "to_check", next, said: [...st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d5").said, { at, by: "host-lead", text }] });
    st.sessions.find((x) => x.session_id === "host").thread.push({ role: "agent", text, at_ms: at });
  }, [again, at2, next]);
  const chat = page.locator(".dk-right .dk-ichat .dk-chat-opts li");
  for (const opts of [side, chat]) {
    await expect(opts.locator(".dk-optk")).toHaveText(["4", "6", "8", "9"]);
    await expect(opts.locator(".dk-optw")).toHaveText(["The picked row opens its journey", "Progress is the page", "A List / Progress switch", "A calm list with progress under each row"]);
    await expect(opts.locator('[data-desk-act="pick"]')).toHaveCount(4);
  }
  // Under the new ask only; the old one's A/B/C are gone.
  await expect(page.locator(".dk-right .dk-ichat .dk-chat-opts")).toHaveCount(1);
  await expect(page.locator(".dk-right").getByText("Old band")).toHaveCount(0);
  // The Desk row says what to pick (in the Items list, ‹ Items: the item's page took its place, #666); then the item again.
  await page.locator(".dk-right .dk-ph .dk-back").first().click();
  await expect(page.locator('[data-desk-item="d5"]').first()).toContainText("4, 6, 8 or 9");
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await expect(side.locator(".dk-optk")).toHaveText(["4", "6", "8", "9"]);
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-right").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-619-${theme}.png`, timeout: 2000 })).toPass();
  }
  await side.nth(2).locator('[data-desk-act="pick"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "pick", text: "8" });
  const built = (await items(page)).find((i) => i.id === "d5").built_as;
  expect((await items(page)).find((i) => i.id === built).title).toBe("A List / Progress switch");
});

// The numbered options one to a line with no pick line, and a range ("1 to 10") on the pick line, are read too.
test("numbered options one to a line, or a pick line's range, are picked from (#619)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d5", { state: "to_check", designs: "https://claude.ai/artifact/MOCKS", next: [], said: [{ at: Date.now(), by: "host-lead", text: "Three versions:\n1. **Calmest:** a Desk menu in the head\n2. **Closest to today:** Desk pills\n3. **Most on screen:** a Desk strip" }] });
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  const opts = page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts li");
  await expect(opts.locator(".dk-optk")).toHaveText(["1", "2", "3"]);
  await expect(opts.locator(".dk-optw")).toHaveText(["Calmest: a Desk menu in the head", "Closest to today: Desk pills", "Most on screen: a Desk strip"]);
  await patchItem(cue, "d5", { said: [{ at: Date.now(), by: "host-lead", text: "Ten on the canvas.\n\nWhich option, 1 to 10, should we build?" }] });
  await expect(opts.locator(".dk-optk")).toHaveText(["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]);
});
