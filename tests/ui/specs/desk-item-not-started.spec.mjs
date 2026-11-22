// An item given out but not started yet (#476, #626): its page shows a status card, under its holder's chat or in
// place of "What's holding it up", saying what it waits for by title (the item before it, the items using the working
// folders) and what happens next, with Start now as a clear button; never "Waiting on you" of its idle holder unless
// something really waits on you; gone the moment it starts. Light and dark, from the design system's tokens.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
const d4Of = (st) => st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4");
// Its holder idle with no turns and nothing open: its own line would say "Waiting on you for your reply".
const idleLead = (st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", thread: [], since_ms: Date.now() - 60000 }); st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending")); };

test("a queued item after another shows a card naming that item, who is on it and what happens next, with Start now", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", thread: [], since_ms: Date.now() - 60000 }); st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending")); });
  await openItem(page, "d4");
  const ichat = page.locator(".dk-right .dk-ichat"), card = ichat.locator(".dk-ns");
  await expect(card).toHaveCount(1);
  await expect(card.locator(".dk-ns-h")).toHaveText("Not started yet");
  await expect(card.locator(".dk-ns-t")).toHaveText("It waits for Search stays centred in the header.");
  await expect(card.locator('.dk-ns-t a[data-desk-item="d3"]')).toHaveText("Search stays centred in the header");
  await expect(card.locator(".dk-ns-s")).toHaveText("search-centre is on it now · step 2 of 3 · has header.js, header.css open");
  await expect(card.locator(".dk-ns-n")).toHaveText("It starts by itself when that's done: both change header.js.");
  await expect(card).not.toContainText("#d3");
  const go = card.locator('button[data-desk-act="start_now"]');
  await expect(go).toHaveText("Start now");
  await expect(go).not.toHaveClass(/\bprimary\b/);
  // Said once: no chain of the same below it, no "Waiting on you", no holder's line.
  await expect(page.locator(".dk-right .dk-pane .dk-chain")).toHaveCount(0);
  await expect(ichat).not.toContainText("Waiting on you");
  await expect(ichat.locator('[data-key="where"]')).toHaveCount(0);
  await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  // A real ask of its holder's still shows: something does wait on you.
  await cue.change((st) => {
    st.items.push({ id: "q-476", session_id: "lead", kind: "question", status: "pending", created_ms: Date.now(), title: "Question", body: "Keep the dot red?", questions: [{ question: "Keep the dot red?", options: [] }] });
  });
  await expect(ichat.locator('[data-key="where"] .cv-what')).toContainText("Waiting on you for your answer");
  await expect(card).toHaveCount(1);
  // Start now starts it, as the panel's does.
  await go.click();
  await cue.called("desk_item_act", { id: "d4", act: "start_now" });
  // Started: the card is gone at once, the holder's own line back.
  await cue.change((st) => { st.items = st.items.filter((i) => i.id !== "q-476"); st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4").state = "working"; Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "working", doing: "Reading rail.js…", doing_ms: Date.now() }); });
  await expect(page.locator(".dk-right .dk-ns")).toHaveCount(0);
  await expect(ichat.locator('[data-key="where"]')).toContainText("Reading rail.js");
});

test("a queued item waiting for a free working folder says how many, with their titles folded away", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => {
    Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", thread: [], since_ms: Date.now() - 60000 }); st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending"));
    Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4"), { after: null, tree_wait: true, copy_users: ["d3", "d5"] });
  });
  await openItem(page, "d4");
  const ichat = page.locator(".dk-right .dk-ichat"), card = ichat.locator(".dk-ns");
  await expect(card.locator(".dk-ns-t")).toHaveText("Waiting for one of 2 items on This Mac to finish.");
  // The titles stay folded away till you ask (#671).
  await expect(card.locator(".dk-ns-s a").first()).toBeHidden();
  await card.locator(".dk-ns-s summary").click();
  await expect(card.locator(".dk-ns-s a")).toHaveText(["Search stays centred in the header", "Hosted sessions can't open the canvas"]);
  await expect(card.locator(".dk-ns-n")).toHaveText("It starts by itself when one is free.");
  await expect(card).not.toContainText("#d");
  await expect(card.locator('button[data-desk-act="start_now"]')).toHaveText("Start now");
  await expect(ichat).not.toContainText("Waiting on you");
  // Start now (the fake starts it without a free folder): the card is gone.
  await card.locator("button").click();
  await expect(page.locator(".dk-right .dk-ns")).toHaveCount(0);
});

test("with no chat of its holder's, the card shows on its page in place of What's holding it up", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d4").holder = null; });
  await openItem(page, "d4");
  const pane = page.locator(".dk-right .dk-pane");
  await expect(pane.locator(".dk-ichat")).toHaveCount(0);
  const card = pane.locator(".dk-inow .dk-ns");
  await expect(card.locator(".dk-ns-t")).toHaveText("It waits for Search stays centred in the header.");
  await expect(card.locator("button")).toHaveText("Start now in a new session");
  await expect(pane.locator(".dk-chain")).toHaveCount(0);
  await expect(pane).not.toContainText("What's holding it up");
});

for (const scheme of ["light", "dark"]) {
  test(`the card is built from the design system's tokens (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await cue.change((st) => { Object.assign(st.sessions.find((x) => x.session_id === "lead"), { state: "waiting", thread: [], since_ms: Date.now() - 60000 }); st.items = st.items.filter((i) => !(i.session_id === "lead" && i.status === "pending")); });
    await openItem(page, "d4");
    const card = page.locator(".dk-right .dk-ichat .dk-ns");
    await expect(card).toBeVisible();
    const got = await card.evaluate((e) => {
      const c = getComputedStyle(e), root = getComputedStyle(document.documentElement);
      const tok = (n) => { const p = document.createElement("div"); p.style.color = `var(${n})`; document.body.append(p); const v = getComputedStyle(p).color; p.remove(); return v; };
      const fs = (sel) => getComputedStyle(e.querySelector(sel)).fontSize;
      return { bg: c.backgroundColor, panel2: tok("--panel-2"), border: c.borderTopColor, line: tok("--line"), leftW: c.borderLeftWidth, topW: c.borderTopWidth, leftC: c.borderLeftColor,
        radius: c.borderTopLeftRadius, pad: `${c.paddingTop} ${c.paddingLeft}`, ink: c.color, inkTok: tok("--ink"),
        body: root.getPropertyValue("--type-chat").trim(), small: root.getPropertyValue("--type-small").trim(),
        t: fs(".dk-ns-t"), s: fs(".dk-ns-s"), n: fs(".dk-ns-n"), muted: getComputedStyle(e.querySelector(".dk-ns-n")).color, mutedTok: tok("--muted"),
        btn: getComputedStyle(e.querySelector("button")).backgroundColor, accent: tok("--accent"), bgImg: c.backgroundImage };
    });
    // No fill: only your messages have a background in a Desk chat (#688); the hairline border frames it.
    expect(got.bg).toBe("rgba(0, 0, 0, 0)");
    expect(got.border).toBe(got.line);
    // No coloured left stripe: the left edge is the same hairline as the rest.
    expect([got.leftW, got.leftC]).toEqual([got.topW, got.border]);
    expect(got.bgImg).toBe("none");
    expect(got.radius).toBe("12px");
    expect(got.pad).toBe("10px 12px");
    expect(got.ink).toBe(got.inkTok);
    expect(got.t).toBe(got.body);
    expect([got.s, got.n]).toEqual([got.small, got.small]);
    expect(got.muted).toBe(got.mutedTok);
    // Start now is a quiet, ordinary button (#671), not the accent fill.
    expect(got.btn).not.toBe(got.accent);
    await expect(page.locator(".dk-right [title]")).toHaveCount(0);
  });
}
