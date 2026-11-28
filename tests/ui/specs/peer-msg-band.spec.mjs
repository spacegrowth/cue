// Messages between two sessions in a session's chat (peer messages, option B with To and From): a band across the
// chat, the two-arrow tile, "From <sender> · <project>" for one it got (with "<this session> answers it below") and
// "To <recipient>" for one it sent, with how it went: Sending…, Waiting for its answer, Answered · <age>, Didn't
// send: <why>. "Open its chat" shows only when the other session is one Cue knows, and opens that chat.
import { test, expect } from "./cue.mjs";
import { NOW } from "./clock.mjs";

for (const scheme of ["light", "dark"]) {
  test(`messages to and from another session are bands, From and To, with how each went (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1400, height: 1000 });
    await cue.open("board-waiting");
    await cue.change((st, at) => {
      const t = st.sessions.find((x) => x.session_id === "relay").thread;
      const m = 60 * 1000;
      t.push(
        { role: "peer", from: "cross-message-claude-2", project: "cue", from_sid: "det", text: "Short reply from the other session.", at_ms: at - 9 * m },
        { role: "peer", from: "cue", text: "An older one, with only its project.", at_ms: at - 8 * m },
        { role: "peer", to: "cross-message-claude-2", to_sid: "det", text: "Run the notch check, please.", at_ms: at - 7 * m, sent: "sending" },
        { role: "peer", to: "cross-message-claude-2", to_sid: "det", text: "Still there?", at_ms: at - 6 * m, sent: "waiting" },
        { role: "peer", to: "cross-message-claude-2", to_sid: "det", text: "Tell me what you see.", at_ms: at - 6 * m, sent: "answered", answered_ms: at - 4 * m },
        { role: "peer", to: "gone-session", to_sid: "nobody", text: "Are you there?", at_ms: at - 3 * m, sent: "failed", why: "that session has ended" },
      );
    }, NOW);
    await page.locator('.nrow[data-big="n1"]').click();
    const pane = page.locator(".active-pane");
    const bands = pane.locator(".peer-band");
    await expect(bands).toHaveCount(6);
    const [me, det] = await page.evaluate(() => [nameOf("relay", sessionOf("relay").project), nameOf("det", "cross-message-claude-2")]);

    // Got: From <sender> · <project>, the words, who answers it, Open its chat (a known session).
    const got = pane.locator('.peer-band[data-peer="got"]');
    await expect(got).toHaveCount(2);
    await expect(got.nth(0).locator(".dk-band-tile svg")).toHaveCount(1);
    await expect(got.nth(0).locator(".dk-band-desk")).toHaveText(`From ${det}`);
    await expect(got.nth(0).locator(".dk-band-by")).toHaveText("· cue");
    await expect(got.nth(0).locator(".dk-band-age")).toHaveText("9m");
    await expect(got.nth(0).locator(".dk-band-t")).toHaveText("Short reply from the other session.");
    await expect(got.nth(0).locator(".dk-band-st")).toHaveText(`${me} answers it below; you don't need to.`);
    await expect(got.nth(0).locator(".dk-band-open")).toHaveText("Open its chat");
    // An older one names only its project: still a received band, with no Open (no session to open).
    await expect(got.nth(1).locator(".dk-band-desk")).toHaveText("From cue");
    await expect(got.nth(1).locator(".dk-band-by")).toHaveCount(0);
    await expect(got.nth(1).locator(".dk-band-open")).toHaveCount(0);

    // Sent: To <recipient>, the outgoing band, and each state.
    const sent = (st) => pane.locator(`.peer-band.out[data-peer="sent"][data-sent="${st}"]`);
    for (const st of ["sending", "waiting", "answered", "failed"]) await expect(sent(st)).toHaveCount(1);
    await expect(sent("sending").locator(".dk-band-desk")).toHaveText(`To ${det}`);
    await expect(sent("sending").locator(".dk-band-f")).toContainText("Sending…");
    await expect(sent("waiting").locator(".dk-band-f")).toContainText("Waiting for its answer");
    await expect(sent("answered").locator(".dk-band-ok")).toHaveText("Answered · 4m ago");
    await expect(sent("answered").locator(".dk-band-ok i")).toHaveCount(1);
    await expect(sent("failed").locator(".dk-band-desk")).toHaveText("To gone-session");
    await expect(sent("failed").locator(".dk-band-fail")).toHaveText("Didn't send: that session has ended");
    for (const st of ["sending", "waiting", "answered"]) await expect(sent(st).locator(".dk-band-open")).toHaveText("Open its chat");
    await expect(sent("failed").locator(".dk-band-open")).toHaveCount(0);

    // The look: across the chat, tinted against what's under it, no edge stripe, no hover text, no old bubble.
    const look = await bands.first().evaluate((e) => {
      const c = getComputedStyle(e); let p = e.parentElement;
      while (p && getComputedStyle(p).backgroundColor === "rgba(0, 0, 0, 0)") p = p.parentElement;
      const r = e.getBoundingClientRect(), pr = e.parentElement.getBoundingClientRect();
      return { left: c.borderLeftWidth, bg: c.backgroundColor, under: p ? getComputedStyle(p).backgroundColor : "", wider: r.width >= pr.width };
    });
    expect(look.left).toBe("0px");
    expect(look.bg).not.toBe(look.under);
    expect(look.wider).toBe(true);
    await expect(pane.locator(".peer-msg")).toHaveCount(0);
    await expect(bands.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/peer-msg-band-${scheme}.png` });

    // Open its chat: that session's chat.
    await got.nth(0).locator(".dk-band-open").click();
    await expect.poll(() => page.evaluate(() => active?.sid)).toBe("det");
  });
}
