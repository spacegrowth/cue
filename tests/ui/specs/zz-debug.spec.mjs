import { test, expect, openDesk } from "./cue.mjs";
test("dbg", async ({ page, cue }) => {
  await cue.open("desk"); await openDesk(page);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "lead").thread.push({ role: "agent", text: "Holding.\n\nTELL DESK: website | Yes.", at_ms: Date.now() }); });
  const b = page.locator('.dk-chat .dk-band[data-band="to"]'); await expect(b).toHaveCount(1);
  console.log(await b.evaluate((e) => { let n = e; while (!n.parentElement.classList.contains("dk-chat")) n = n.parentElement; return n.outerHTML.replace(/<svg.*?<\/svg>/g, "").slice(0, 3000); }));
});
