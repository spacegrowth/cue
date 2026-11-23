import { test, expect, openDesk } from "./cue.mjs";
test("dbg", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1800, height: 1095 });
  await cue.open("desk");
  await openDesk(page);
  await page.locator('[data-desk-item="d5"]').first().click();
  await page.waitForTimeout(300);
  console.log("SMETA", await page.locator(".dk-right .dkn-smeta").allTextContents());
  await page.locator('[data-desk-item="d6"]').first().click();
  console.log("H1 sel", await page.locator(".dk-right h1").evaluate((e) => [getComputedStyle(e).userSelect, getComputedStyle(e).webkitUserSelect]));
  const diff = "diff --git a/cache.ts b/cache.ts\n--- a/cache.ts\n+++ b/cache.ts\n@@ -1,2 +1,3 @@\n const key = sku;\n+const hit = cache.get(key);\n";
  await cue.override("diff_git", ({ op, against }, d) => (op === "toplevel" ? { code: 0, stdout: "/x\n" } : op === "diff" ? { code: 0, stdout: against === "abc123" ? d : "" } : { code: 0, stdout: "" }), diff);
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").turn_from = { root: "/x", commit: "abc123", untracked: [] }; });
  await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
  await page.locator('.dk-right .dkn-stab[data-desk-c-stab="files"]').click();
  await page.waitForTimeout(500);
  console.log("FILES", await page.locator(".dk-right .dkn-sfiles").evaluate((e) => e.outerHTML.slice(0, 1500)));
  console.log("BOX", await page.locator(".dk-right .dkn-sfiles .ib-file").evaluate((e) => { const r = e.getBoundingClientRect(); const p = e.closest(".dk-pfiles"); return [r.x, r.y, r.width, r.height, getComputedStyle(p).display, p.className, JSON.stringify(e.closest(".dkn-sfiles").getBoundingClientRect())]; }));
});
