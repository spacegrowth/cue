import { test, expect } from "./cue.mjs";

test("hosted Codex offers its models and sends the selected command without Claude context variants", async ({ cue, page }) => {
  await cue.open("world-team");
  await cue.change((state) => {
    const lead = state.sessions.find((session) => session.session_id === "lead");
    lead.harness = "codex";
    lead.hosted = true;
    lead.state = "waiting";
  });
  await cue.override("session_commands", () => ({
    list: [{
      name: "model", kind: "cue", desc: "Choose the model for subsequent turns", hint: "<model>",
      args: [
        { value: "model-a", label: "Model A", desc: "First model" },
        { value: "model-b", label: "Model B", desc: "Second model" },
      ],
    }],
    reading: false,
  }));
  await cue.override("send_to_session", () => "not sent: Model set to model-b for subsequent turns");
  await page.evaluate(() => cmdLists.clear());
  await page.locator('.nrow[data-big="n1793984390004"]').click();
  const box = page.locator('.active-pane textarea[data-text="s:lead"]');
  await box.fill("/model");
  await expect(page.locator('.cmd-row[data-cmd-name="model"]')).toContainText("in Cue");
  await box.press("Enter");
  await expect(box).toHaveValue("/model ");
  await expect(page.locator(".cmd-menu .cmd-row b")).toHaveText(["/model model-a Model A", "/model model-b Model B"]);
  await expect(page.locator(".cmd-menu")).not.toContainText("[1m]");
  expect(await cue.calls("send_to_session")).toHaveLength(0);
  await page.locator('.cmd-row[data-cmd-name="model model-b"]').click();
  await expect(box).toHaveValue("/model model-b");
  await box.press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "/model model-b" });
  await expect(box).toHaveValue("");
  const confirmation = await page.evaluate(() => {
    const row = document.createElement("div");
    row.innerHTML = outboxRow({ text: "/model model-b", images: [], sid: "lead", via: "not sent: Model set to model-b for subsequent turns" }, null, "confirmation");
    return row.querySelector(".cv-meta").textContent;
  });
  expect(confirmation).toBe("Model set to model-b for subsequent turns");
});
