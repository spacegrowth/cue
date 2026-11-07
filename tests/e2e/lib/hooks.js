// An agent's side of Cue: hook payloads shaped as Claude Code sends them (the same shapes
// tests/test_protocol.py uses), run through the real hook (`cue hook`, via the shim Cue writes at
// launch), and the bare events an agent's hook sends over the socket.
import { spawn } from "node:child_process";
import path from "node:path";

/** A PermissionRequest for `tool` with `input`, from session `sid`. */
export function permission(sid, { tool = "Bash", input = { command: "touch x", description: "make x" }, cwd = "/tmp/cue-e2e-project" } = {}) {
  return {
    session_id: sid, transcript_path: "", cwd, hook_event_name: "PermissionRequest",
    tool_name: tool, tool_input: input, permission_suggestions: [],
  };
}

/** Run the real hook for `event` with `payload` on stdin. `decision` resolves with what Claude would be
 *  told: hookSpecificOutput.decision, or null for no output. */
export function runHook(world, event, payload) {
  const proc = spawn(path.join(world.cueHome, "bin", "cue-hook"), [event, "claude"], { env: world.env(), stdio: ["pipe", "pipe", "ignore"] });
  world.track(proc);
  let out = "";
  proc.stdout.on("data", (d) => (out += d));
  const decision = new Promise((resolve) => proc.on("exit", () => resolve(out.trim() ? JSON.parse(out).hookSpecificOutput.decision : null)));
  proc.stdin.end(JSON.stringify(payload));
  return { proc, decision };
}

/** A turn that ended: what Claude's Stop hook reports. `pane`: the tmux pane it runs in; `pid`: the
 *  agent's process (the hook sends its parent's). */
export function stopped(sid, { cwd = "/tmp/cue-e2e-project", message = "done", pane, pid } = {}) {
  return { type: "event", event: "stopped", harness: "claude", session_id: sid, cwd, message, ...(pane ? { tmux_pane: pane } : {}), ...(pid ? { agent_pid: pid } : {}) };
}

/** A turn that started (Claude's UserPromptSubmit). */
export function active(sid, { cwd = "/tmp/cue-e2e-project", message = "go", pane } = {}) {
  return { type: "event", event: "active", harness: "claude", session_id: sid, cwd, message, ...(pane ? { tmux_pane: pane } : {}) };
}
