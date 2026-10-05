/**
 * Cue extension for Pi.
 *
 * Pi has no permission prompts of its own, so this extension is the gate: for the tool calls
 * chosen by Cue's config.json (~/Library/Application Support/dev.spacegrowth.cue) it asks BOTH the terminal and Cue at once — whichever you answer
 * first wins, and the other side is closed. It also adds an `ask_user` tool (multiple-choice
 * questions answerable from Cue) and tells Cue when a session finishes and is waiting for you.
 *
 * Config (config.json in Cue's data folder), all optional:
 *   { "pi": { "gate": "dangerous" } }
 *   gate: "dangerous" (default: rm -rf, sudo, force-push, …) | "all" (every bash/write/edit) | "off"
 *
 * If Cue isn't running, gated calls fall back to the terminal prompt alone.
 */
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";
import * as readline from "node:readline";

const CUE_HOME = process.env.CUE_HOME || path.join(os.homedir(), "Library", "Application Support", "dev.spacegrowth.cue");
const SOCK = path.join(CUE_HOME, "cue.sock");

const DANGEROUS = [
	/\brm\s+(-[a-z]*r[a-z]*f|-[a-z]*f[a-z]*r|--recursive)/i,
	/\bsudo\b/,
	/\bgit\s+push\b.*(--force|-f\b)/,
	/\bgit\s+(reset\s+--hard|clean\s+-[a-z]*f)/,
	/\b(chmod|chown)\b.*\b777\b/,
	/\bmkfs\b|\bdd\s+if=/,
	/\b(drop|truncate)\s+(table|database)\b/i,
];

type Gate = "dangerous" | "all" | "off";

function gateMode(): Gate {
	try {
		const g = JSON.parse(fs.readFileSync(path.join(CUE_HOME, "config.json"), "utf8"))?.pi?.gate;
		if (g === "all" || g === "off" || g === "dangerous") return g;
	} catch {}
	return "dangerous";
}

function needsGate(toolName: string, input: any, mode: Gate): boolean {
	if (mode === "off") return false;
	if (mode === "all") return ["bash", "write", "edit"].includes(toolName);
	return toolName === "bash" && DANGEROUS.some((p) => p.test(String(input?.command ?? "")));
}

let tty = "";
function origin(ctx: ExtensionContext) {
	if (!tty) {
		try {
			const t = execFileSync("ps", ["-o", "tty=", "-p", String(process.pid)], { encoding: "utf8" }).trim();
			tty = t && t !== "??" ? `/dev/${t}` : "";
		} catch {}
	}
	return {
		harness: "pi",
		session_id: ctx.sessionManager.getSessionId(),
		cwd: ctx.cwd,
		tty,
		agent_pid: process.pid,
		term_program: process.env.TERM_PROGRAM ?? "",
		iterm_session_id: process.env.ITERM_SESSION_ID ?? "",
		tmux_pane: process.env.TMUX ? (process.env.TMUX_PANE ?? "") : "",
	};
}

function textOf(message: any): string {
	const c = message?.content;
	if (typeof c === "string") return c;
	if (Array.isArray(c)) return c.filter((b: any) => b?.type === "text").map((b: any) => b.text).join("\n");
	return "";
}

/** Fire-and-forget event. Never throws, never blocks Pi for more than a moment. */
/** A tool call in plain words, the way Cue says it for Claude Code and Codex too. */
function describeTool(name: string, input: any): string {
	const first = (t: unknown) => String(t ?? "").split("\n").map((l) => l.trim()).find(Boolean) ?? "";
	const cut = (t: string, n: number) => (t.length > n ? t.slice(0, n) + "…" : t);
	const file = (p: unknown) => String(p ?? "").split("/").pop() || "a file";
	switch (name) {
		case "bash": case "powershell": return `Running: ${cut(first(input?.command), 90)}`;
		case "edit": case "write": return `Editing ${file(input?.path)}`;
		case "read": return `Reading ${file(input?.path)}`;
		case "grep": return `Searching: ${cut(first(input?.pattern), 60)}`;
		case "find": case "ls": return `Looking through ${file(input?.path) || "files"}`;
		default: return `Using ${name}`;
	}
}

function sendEvent(ctx: ExtensionContext, event: string, message = "", turn?: { text: string; hooked: boolean }[]) {
	try {
		const s = net.createConnection(SOCK);
		s.on("error", () => {});
		s.setTimeout(500, () => s.destroy());
		// CUE_DRIVEN_BY: set by a tool that runs this session for another agent (its finished turns are that agent's to handle).
		const drivenBy = (process.env.CUE_DRIVEN_BY || "").trim();
		s.end(JSON.stringify({ type: "event", event, message, ...(turn?.length ? { turn } : {}), ...(drivenBy ? { driven_by: drivenBy } : {}), ...origin(ctx) }) + "\n");
	} catch {}
}

type CueAnswer = { behavior: string; message?: string; answers?: Record<string, string> };

/**
 * Open an Ask on Cue. `answer` resolves with Cue's decision, or never (Cue not running, or
 * cancelled). Survives a Cue restart by re-asking. `resolvedElsewhere()` tells Cue the
 * terminal answered; `close()` drops it.
 */
function askCue(ask: object) {
	let sock: net.Socket | undefined;
	let finished = false;
	let tries = 0;
	const answer = new Promise<CueAnswer>((resolve) => {
		const connect = () => {
			if (finished) return;
			try {
				const s = net.createConnection(SOCK);
				sock = s;
				s.on("error", () => {});
				s.on("connect", () => {
					tries = 0;
					s.write(JSON.stringify({ type: "ask", ...ask }) + "\n");
				});
				readline.createInterface({ input: s }).on("line", (line) => {
					try {
						const m = JSON.parse(line);
						if (m.type === "decision") {
							finished = true;
							resolve(m);
						}
						if (m.type === "cancel") finished = true;
					} catch {}
				});
				// Cue quit or restarted while we wait: re-ask when it's back (about 15 minutes of retries).
				s.on("close", () => {
					if (!finished && tries++ < 450) setTimeout(connect, 2000);
				});
			} catch {}
		};
		connect();
	});
	return {
		answer,
		resolvedElsewhere(behavior: string) {
			finished = true;
			try {
				sock?.end(JSON.stringify({ type: "resolved", by: "terminal", behavior }) + "\n");
			} catch {}
		},
		close() {
			finished = true;
			sock?.destroy();
		},
	};
}

const never = new Promise<never>(() => {});

export default function (pi: ExtensionAPI) {
	let lastAssistant = "";
	// Everything Pi writes in this run; `hooked` once an extension pushed it to keep going after it
	// meant to stop (that follow-up isn't the answer you want first).
	let turn: { text: string; hooked: boolean }[] = [];
	let hooked = false;

	pi.on("message_end", async (event) => {
		const m: any = event.message;
		if (m?.role === "assistant") {
			const t = textOf(m).trim();
			if (t) {
				lastAssistant = t;
				turn.push({ text: t, hooked });
			}
		} else if (m && m.role !== "user" && m.role !== "toolResult" && turn.length) {
			hooked = true; // an extension injected a message mid-run
		}
		return undefined;
	});
	// The prompt rides along so Cue's Working column can show what this session is doing.
	pi.on("before_agent_start", async (e, ctx) => {
		sendEvent(ctx, "active", e.prompt ?? "");
		return undefined;
	});
	pi.on("agent_start", async (_e, ctx) => sendEvent(ctx, "active"));
	pi.on("agent_end", async (_e, ctx) => {
		sendEvent(ctx, "stopped", lastAssistant, turn);
		turn = [];
		hooked = false;
	});
	pi.on("session_shutdown", async (_e, ctx) => {
		shuttingDown = true;
		replySock?.destroy();
		sendEvent(ctx, "ended");
	});

	// Replies typed in Cue arrive here and go straight into the session — no terminal typing, so it
	// works in any terminal. Reconnects if Cue restarts.
	let replySock: net.Socket | undefined;
	let shuttingDown = false;
	const listenForReplies = (ctx: ExtensionContext) => {
		if (shuttingDown) return;
		try {
			const s = net.createConnection(SOCK);
			replySock = s;
			s.on("error", () => {});
			s.on("connect", () => s.write(JSON.stringify({ type: "subscribe", ...origin(ctx) }) + "\n"));
			readline.createInterface({ input: s }).on("line", (line) => {
				try {
					const m = JSON.parse(line);
					// Stop button / Esc twice in Cue: abort the turn, like Esc in Pi's own terminal.
					if (m.type === "interrupt") {
						if (!ctx.isIdle()) ctx.abort();
						return;
					}
					// Renamed in Cue: the same as /name in Pi.
					if (m.type === "rename") {
						if (typeof m.name === "string" && m.name.trim()) pi.setSessionName(m.name.trim());
						return;
					}
					if (m.type !== "reply") return;
					const images = (Array.isArray(m.images) ? m.images : []).flatMap((im: { path: string; mime: string }) => {
						try {
							return [{ type: "image" as const, data: fs.readFileSync(im.path).toString("base64"), mimeType: im.mime }];
						} catch {
							return [];
						}
					});
					const text = typeof m.text === "string" ? m.text.trim() : "";
					if (!text && !images.length) return;
					// "/…" from Cue runs as a command (/pilead:review, a prompt template), as it would typed
					// here; " /…" (a leading space) is a message that starts with a slash. Pi runs commands
					// from an extension only when asked to.
					const command = typeof m.text === "string" && m.text.startsWith("/");
					// Ctrl+Enter in Cue ("now"): a steer, read mid-turn. Otherwise it waits for the turn to end.
					pi.sendUserMessage(images.length ? [...(text ? [{ type: "text" as const, text }] : []), ...images] : text, { deliverAs: m.now ? "steer" : "followUp", expandPromptTemplates: command });
				} catch {}
			});
			s.on("close", () => {
				if (!shuttingDown) setTimeout(() => listenForReplies(ctx), 3000);
			});
		} catch {}
	};
	pi.on("session_start", async (_e, ctx) => {
		listenForReplies(ctx);
		return undefined;
	});

	// Between steps too, so Cue isn't left showing a command that already finished.
	pi.on("tool_execution_end", async (_e, ctx) => sendEvent(ctx, "doing", "Thinking…"));
	pi.on("message_start", async (e: any, ctx) => {
		if (e?.message?.role === "assistant") sendEvent(ctx, "doing", "Writing…");
	});
	pi.on("tool_call", async (event, ctx) => {
		// What it's doing right now, shown on its card and in Cue's Active pane ("Running: cargo test").
		sendEvent(ctx, "doing", describeTool(event.toolName, event.input as any));
		if (!needsGate(event.toolName, event.input, gateMode())) return undefined;

		const cue = askCue({
			kind: "permission",
			tool_name: event.toolName,
			tool_input: event.input,
			context: lastAssistant ? [{ role: "assistant", text: lastAssistant }] : [],
			...origin(ctx),
		});
		const ac = new AbortController();
		const summary = event.toolName === "bash" ? String((event.input as any).command) : `${event.toolName} ${(event.input as any).path ?? ""}`;
		const fromTerminal = ctx.hasUI
			? ctx.ui
					.select(`Allow ${event.toolName}?\n\n  ${summary}`, ["Yes", "No"], { signal: ac.signal })
					.then((c) => (c === undefined ? never : { who: "terminal", allow: c === "Yes" }))
			: never;
		const fromCue = cue.answer.then((d) => ({ who: "cue", allow: d.behavior !== "deny", message: d.message }));

		// No terminal UI and Cue is unreachable: nobody can approve, so block.
		if (!ctx.hasUI) {
			const timeout = new Promise<{ who: string; allow: boolean; message?: string }>((r) =>
				setTimeout(() => r({ who: "none", allow: false, message: "Blocked: no terminal UI and no answer from Cue." }), 10 * 60_000),
			);
			const r = await Promise.race([fromCue, timeout]);
			cue.close();
			return r.allow ? undefined : { block: true, reason: r.message ?? "Blocked in Cue" };
		}

		const r: { who: string; allow: boolean; message?: string } = await Promise.race([fromCue, fromTerminal]);
		ac.abort();
		if (r.who === "terminal") cue.resolvedElsewhere(r.allow ? "allowed" : "denied");
		else cue.close();
		return r.allow ? undefined : { block: true, reason: r.message || "Blocked by user" };
	});

	pi.registerTool({
		name: "ask_user",
		label: "Ask user",
		description:
			"Ask the user a multiple-choice question when you need their decision to proceed. They can also type a free-form answer.",
		parameters: Type.Object({
			question: Type.String({ description: "The question to ask" }),
			options: Type.Array(
				Type.Object({
					label: Type.String({ description: "Short option label" }),
					description: Type.Optional(Type.String({ description: "What choosing this means" })),
				}),
				{ description: "2-5 options" },
			),
		}),
		executionMode: "sequential",
		async execute(_id, params, _signal, _onUpdate, ctx) {
			const OTHER = "Type something…";
			const cue = askCue({
				kind: "question",
				tool_name: "ask_user",
				tool_input: { questions: [{ question: params.question, header: "", options: params.options, multiSelect: false }] },
				context: lastAssistant ? [{ role: "assistant", text: lastAssistant }] : [],
				...origin(ctx),
			});
			const ac = new AbortController();
			const fromTerminal: Promise<{ who: string; answer: string }> = ctx.hasUI
				? ctx.ui
						.select(params.question, [...params.options.map((o) => o.label), OTHER], { signal: ac.signal })
						.then(async (c) => {
							if (c === undefined) return never;
							if (c !== OTHER) return { who: "terminal", answer: c };
							const typed = await ctx.ui.input(params.question, "Your answer", { signal: ac.signal });
							return typed === undefined ? never : { who: "terminal", answer: typed };
						})
				: never;
			const fromCue = cue.answer.then((d) => ({
				who: "cue",
				answer: d.behavior === "deny" ? `(declined${d.message ? `: ${d.message}` : ""})` : (Object.values(d.answers ?? {})[0] ?? ""),
			}));
			const r = await Promise.race([fromCue, fromTerminal]);
			ac.abort();
			if (r.who === "terminal") cue.resolvedElsewhere(`answered “${r.answer}”`);
			else cue.close();
			return {
				content: [{ type: "text", text: `User answered: ${r.answer}` }],
				details: { question: params.question, answer: r.answer, via: r.who },
			};
		},
	});
}
