// What the terminal specs share.

/** agent-relay's chat open and its terminal attached, its screen (the fake's permission menu) in the buffer. */
export async function openTerminal(page) {
  await page.locator('.nrow[data-big="n1"]').click();
  await page.locator('.tstrip[data-sid="relay"]').click();
  await page.waitForFunction(() => {
    const b = typeof termObj !== "undefined" && termObj?.buffer.active;
    return b && Array.from({ length: b.length }, (_, i) => b.getLine(i)?.translateToString()).some((l) => l?.includes("Do you want to proceed?"));
  });
}
