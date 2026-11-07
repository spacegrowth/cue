// relay's state as relay writes it, under the world's own HOME: a lead (its marker, and Claude Code's
// registry entry that says its process is live) and executors that reported a while ago.
import fs from "node:fs";
import path from "node:path";

function write(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, typeof value === "string" ? value : JSON.stringify(value));
}

/** relay's timestamp: this Mac's local time, `minutesAgo` back, no zone. */
function relayTime(minutesAgo) {
  const d = new Date(Date.now() - minutesAgo * 60_000);
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** Lead `lead` (Cue session id, its process `pid`) armed, with `executors` ({ id, session, topic }) that
 *  reported packet 1 five minutes ago and that nobody has woken the lead for. */
export function relayCrew(world, { lead, pid, project = "cue-e2e-project", executors }) {
  const relay = path.join(world.home, ".relay-tasks");
  write(path.join(world.home, ".claude", "sessions", `${pid}.json`), { pid, sessionId: lead });
  write(path.join(relay, "lead", lead, "marker.json"), { session_id: lead, project });
  for (const e of executors) {
    write(path.join(relay, e.id, "session.json"), {
      session_id: e.id, agent: "relay-executor", claude_session: e.session, owner_lead: lead, topic: e.topic,
      status: "reported", current_packet: 1, report_seen_packet: 0, updated: relayTime(5),
    });
    write(path.join(relay, e.id, "packets", "001-packet.md"), `GOAL: ${e.topic} the thing\n`);
    write(path.join(relay, e.id, "packets", "001-report.md"), `${e.topic}: done, staged.\n`);
  }
}
