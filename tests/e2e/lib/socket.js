// Cue's own socket ($CUE_HOME/cue.sock): one JSON line out, one back, as cue-ctl and the hooks talk to it.
import net from "node:net";

/** Send `msg` and resolve with Cue's one-line answer. */
export function call(sock, msg, timeout = 10_000) {
  return new Promise((resolve, reject) => {
    const c = net.createConnection(sock);
    let buf = "";
    const timer = setTimeout(() => { c.destroy(); reject(new Error(`no answer to ${msg.type} in ${timeout} ms`)); }, timeout);
    c.on("connect", () => c.write(JSON.stringify(msg) + "\n"));
    c.on("data", (d) => {
      buf += d;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      clearTimeout(timer);
      c.end();
      try { resolve(JSON.parse(buf.slice(0, nl))); } catch (e) { reject(e); }
    });
    c.on("error", (e) => { clearTimeout(timer); reject(e); });
  });
}

/** Send `msg` and hang up once it's written: how an agent's hook reports an event. */
export function tell(sock, msg) {
  return new Promise((resolve, reject) => {
    const c = net.createConnection(sock, () => c.end(JSON.stringify(msg) + "\n", resolve));
    c.on("error", reject);
  });
}
