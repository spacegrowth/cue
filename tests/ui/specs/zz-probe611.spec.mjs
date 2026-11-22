import { test, expect, openDesk } from "./cue.mjs";
const CMD = "python3 - <<'EOF'\np='desk-quick-add.spec.mjs'; s=open(p).read()\nold='''  // One body line (#501: 13.5px/1.5) and the row's 1px above and below.\n  expect(a.height).toBeLessThan(23);'''\nassert old in s\ns=s.replace(old,'''  // One body line each (#501), at any text size (#594).\n  expect(await textLines(a)).toBe(1);'''\nopen(p,'w').write(s)\nprint('ok')\nEOF";
const tall = (page, tag) => page.evaluate((tag) => {
  const els = [...document.querySelectorAll("body *")].filter((e) => e.textContent.includes("assert old") && e.getBoundingClientRect().height > 0);
  const leaves = els.filter((e) => ![...e.children].some((c) => c.textContent.includes("assert old")));
  return leaves.map((e) => ({ tag, cls: e.className, chain: [e.parentElement?.className, e.parentElement?.parentElement?.className, e.closest("[class]:not(pre):not(.req-cut):not(.code-wrap)")?.className].join(" / "), h: Math.round(e.getBoundingClientRect().height), sh: e.scrollHeight }));
}, tag).then((r) => console.log(JSON.stringify(r)));
test("probe desk", async ({ cue, page }) => {
  await cue.open("desk");
  await cue.change((st, command) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.harness = "claude"; worker.hosted = true; worker.state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    Object.assign(d.items.find((item) => item.holder?.sid === "host"), { id: "227", kind: "feature", title: "Give the Desk its own icon", state: "needs_you" });
    st.items.push({ id: "perm", session_id: "host", harness: "claude", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command }, suggestions: [], created_ms: Date.now(), thread: [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }] });
  }, CMD);
  await openDesk(page);
  await tall(page, "list");
  await page.screenshot({ path: process.env.TMPDIR + "/p611-list.png" });
  await page.locator('.dk-items [data-desk-item="227"]').click();
  await page.waitForTimeout(300);
  await tall(page, "item");
  console.log(await page.evaluate(() => { const e = [...document.querySelectorAll(".dk-q")].find((x) => x.textContent.includes("assert old")); const a = []; for (let x = e; x && a.length < 12; x = x.parentElement) a.push(x.className); return a.join(" < "); }));
  console.log(await page.evaluate(() => { const i = state.desk.projects.find((x) => x.project === "cue").items.find((x) => x.id === "227"); return JSON.stringify({ st: i.state, notYet: !!deskNotStarted(i), chain: deskChain(i).slice(0, 200), html: document.querySelector(".dk-inow").innerHTML.slice(0, 300), ask: i.ask, holder: i.holder, s: !!deskItemChatOf(i), fin: i.finish, q: deskQuestion(i).slice(0, 80), ans: !!deskAnswerOf(i) }); }));
  await page.screenshot({ path: process.env.TMPDIR + "/p611-item.png" });
});
