// Waiting on what can be observed: every wait in the journeys is one of these, never a fixed sleep.

/** Ask `probe` until it returns something truthy (that value is returned) or `timeout` ms pass. `what`
 *  names the condition in the failure, with the last value seen or error thrown. */
export async function waitFor(probe, what, { timeout = 10_000, every = 50 } = {}) {
  const end = Date.now() + timeout;
  let last;
  for (;;) {
    try {
      last = await probe();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    if (Date.now() >= end) {
      const seen = last instanceof Error ? last.message : JSON.stringify(last);
      throw new Error(`timed out after ${timeout} ms waiting for ${what} (last: ${seen})`);
    }
    await pause(every);
  }
}

/** The gap between two looks of a poll. */
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
