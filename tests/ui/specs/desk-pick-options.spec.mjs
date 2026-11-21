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
  await page.locator('[data-desk-item="d5"] .dk-t').click();
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
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    // The theme redraws the page: again once it's settled.
    if (process.env.SHOT_DIR) await expect(async () => page.locator(".dk-panel[data-side-for]").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-445-${theme}.png`, timeout: 2000 })).toPass();
  }
  await opts.nth(1).locator('[data-desk-act="pick"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "pick", text: "M2" });
  await expect(head.locator(".dk-pword")).toHaveText("Done");
  await expect(head.locator(".dk-opts")).toHaveCount(0);
  const n = (await items(page)).find((i) => i.id === "d5").built_as;
  await expect(head.locator(".dk-built")).toHaveText(`You picked option M2. It's being built as #${n}`);
  expect((await items(page)).find((i) => i.id === n).title).toBe("Build: working items (option M2)");
});

test("options listed one to a line are picked from even with no pick line; one option alone isn't a list (#445)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await patchItem(cue, "d5", { state: "to_check", designs: "https://claude.ai/artifact/MOCKS", said: [{ at: Date.now(), by: "host-lead", text: "On the canvas:\nA: rows\nB) cards\nC — a table" }] });
  await page.locator('[data-desk-item="d5"] .dk-t').click();
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
  await page.locator('[data-desk-item="d5"] .dk-t').click();
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
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
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
  await page.locator('[data-desk-item="d5"] .dk-t').click();
  for (const opts of [page.locator(".dk-panel[data-side-for] .dk-pnow .dk-opts li"), page.locator(".dk-right .dk-ichat .dk-chat-opts li")]) {
    await expect(opts.locator(".dk-optk")).toHaveText(["A", "B", "C"]);
    await expect(opts.locator(".dk-optw")).toHaveText(["rows", "cards (suggested)", "a table"]);
    await expect(opts.locator('[data-desk-act="pick"]')).toHaveCount(3);
  }
});
