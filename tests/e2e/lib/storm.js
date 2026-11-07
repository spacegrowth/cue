// A storm of hook events: `sessions` stand-in agents whose turns start and end, `perSecond` events in
// all, sent over the socket the way their hooks would, until stop(). Each event's delivery is counted.
import { active, stopped } from "./hooks.js";

export function startStorm(world, { perSecond = 100, sessions = 20 } = {}) {
  let n = 0;
  const sent = [];
  const failed = [];
  const timer = setInterval(() => {
    const sid = `e2e-storm-${n % sessions}`;
    const ev = Math.floor(n / sessions) % 2 ? stopped(sid, { message: `turn ${n}` }) : active(sid, { message: `turn ${n}` });
    n++;
    sent.push(world.tell(ev).catch((e) => failed.push(e)));
  }, 1000 / perSecond);
  return {
    /** How many events have gone out so far. */
    sent: () => sent.length,
    /** Stops it; resolves once every event went out, with how many were sent and how many failed. */
    async stop() {
      clearInterval(timer);
      await Promise.all(sent);
      return { sent: sent.length, failed: failed.length };
    },
  };
}
