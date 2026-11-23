// A message from an item's page (yours to its holder or the Desk's lead, a send-back, the lead's note) is in the same
// states as any message in the session it went to (#422): its thread, held or queued in it, still on its way.
/** Retry on step `step` of item `id`: its message sent again, as it was. */
async function deskRetry(id, step) {
  try { await invoke("desk_item_act", { id, act: "retry", text: step }); toast("Sent again"); }
  catch (e) { toast(`Not sent: ${e}`, 5000); }
  renderMain();
}
