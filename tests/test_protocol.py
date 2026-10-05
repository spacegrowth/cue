#!/usr/bin/python3
"""End-to-end protocol tests against a RUNNING Cue (real socket, real hook script).

  python3 tests/test_protocol.py

Each test plays Claude: pipes a real-shaped hook payload into Cue's built-in hook (via its shim), then answers
(or doesn't) through the socket like the window would, and checks what Claude would receive.
"""
import json
import os
import socket
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The hook is built into Cue's binary; Cue writes this shim at launch (what the agents' settings run).
HOOK = os.path.join(os.environ.get("CUE_HOME") or "", "bin", "cue-hook")
SOCK = os.path.join(os.environ.get("CUE_HOME") or os.path.expanduser("~/.cue"), "cue.sock")
SID = f"test-{os.getpid()}"


def call(msg):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(5)
    s.connect(SOCK)
    s.sendall((json.dumps(msg) + "\n").encode())
    return json.loads(s.makefile("r").readline())


def items():
    return call({"type": "list"})["items"]


def history():
    return call({"type": "list"})["history"]


def wait_for(pred, what, timeout=6):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.1)
    raise AssertionError(f"timed out waiting for {what}")


def payload(tool_name="Bash", tool_input=None, **kw):
    return {
        "session_id": kw.get("sid", SID),
        "transcript_path": kw.get("transcript", ""),
        "cwd": "/tmp/cue-test-project",
        "hook_event_name": "PermissionRequest",
        "tool_name": tool_name,
        "tool_input": tool_input if tool_input is not None else {"command": "touch x", "description": "make x"},
        "permission_suggestions": kw.get("suggestions", []),
    }


def start_hook(event, p, env=None, harness="claude"):
    proc = subprocess.Popen([HOOK, event, harness], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
    proc.stdin.write(json.dumps(p))
    proc.stdin.close()
    return proc


def pending_for(sid=SID, kind=None):
    return [i for i in items() if i["session_id"] == sid and (kind is None or i["kind"] == kind)]


def ask_and_get(p):
    proc = start_hook("permission", p)
    decisions = lambda: [i for i in pending_for(p["session_id"]) if i["kind"] != "waiting"]
    it = wait_for(lambda: next(iter(decisions()), None), "decision item to appear")
    return proc, it


def finish(proc):
    out = proc.stdout.read()
    proc.wait(5)
    return json.loads(out)["hookSpecificOutput"]["decision"] if out.strip() else None


# ---------------------------------------------------------------- tests

def test_allow():
    proc, it = ask_and_get(payload())
    assert it["kind"] == "permission" and it["tool_input"]["command"] == "touch x"
    assert it["harness"] == "claude" and it["project"] == "cue-test-project"
    assert call({"type": "respond", "id": it["id"], "decision": {"behavior": "allow"}})["ok"]
    assert finish(proc) == {"behavior": "allow"}
    h = next(i for i in history() if i["id"] == it["id"])
    assert h["status"] == "answered" and h["outcome"] == "allowed"


def test_deny_with_instruction():
    proc, it = ask_and_get(payload(tool_input={"command": "rm -rf build", "description": "clean"}))
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "deny", "message": "Use make clean instead."}})
    assert finish(proc) == {"behavior": "deny", "message": "Use make clean instead."}


def test_plain_deny_gets_a_message():
    proc, it = ask_and_get(payload())
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "deny"}})
    assert finish(proc) == {"behavior": "deny", "message": "Denied in Cue."}


def test_question_answers_flow_into_updated_input():
    qi = {"questions": [{"question": "Which color?", "header": "Color", "multiSelect": False,
                         "options": [{"label": "Red"}, {"label": "Green"}]}]}
    proc, it = ask_and_get(payload("AskUserQuestion", qi))
    assert it["kind"] == "question"
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "allow", "answers": {"Which color?": "Green"}}})
    d = finish(proc)
    assert d["behavior"] == "allow"
    assert d["updatedInput"]["answers"] == {"Which color?": "Green"}
    assert d["updatedInput"]["questions"] == qi["questions"], "original input must be kept"


def test_always_allow_passes_the_chosen_suggestion():
    sugg = [{"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "touch:*"}], "behavior": "allow", "destination": "session"}]
    proc, it = ask_and_get(payload(suggestions=sugg))
    assert it["suggestions"] == sugg
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "allow_always", "permission": sugg[0]}})
    assert finish(proc) == {"behavior": "allow", "updatedPermissions": sugg}


def test_no_cue_means_silent_fallthrough():
    env = {**os.environ, "CUE_HOME": tempfile.mkdtemp()}
    t = time.time()
    proc = start_hook("permission", payload(), env=env)
    assert finish(proc) is None, "no output => Claude shows its normal prompt"
    assert proc.returncode == 0 and time.time() - t < 2


def test_hook_killed_marks_item_gone():
    proc, it = ask_and_get(payload())
    proc.kill()
    proc.wait()
    h = wait_for(lambda: next((i for i in history() if i["id"] == it["id"]), None), "item to leave the queue")
    assert h["status"] == "gone"
    assert not call({"type": "respond", "id": it["id"], "decision": {"behavior": "allow"}})["ok"], "late answer is refused"


def test_answered_in_terminal_via_transcript():
    tool_input = {"command": "touch y", "description": "make y"}
    with tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False) as f:
        f.write(json.dumps({"type": "user", "message": {"content": "make me a file y"}}) + "\n")
        f.write(json.dumps({"type": "assistant", "message": {"content": [
            {"type": "text", "text": "Creating y now."},
            {"type": "tool_use", "id": "toolu_T1", "name": "Bash", "input": tool_input}]}}) + "\n")
        path = f.name
    proc, it = ask_and_get(payload(tool_input=tool_input, transcript=path))
    assert [c["text"] for c in it["context"]] == ["make me a file y", "Creating y now."], it["context"]
    time.sleep(1.5)
    assert pending_for(), "still pending before the terminal answers"
    with open(path, "a") as f:  # what Claude writes when you answer in its terminal
        f.write(json.dumps({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_T1", "content": "ok"}]}}) + "\n")
    h = wait_for(lambda: next((i for i in history() if i["id"] == it["id"]), None), "transcript watcher to clear it", timeout=5)
    assert h["status"] == "answered_elsewhere"
    assert finish(proc) is None, "hook stands down silently"
    os.unlink(path)


def test_dead_agent_clears_its_items():
    sleeper = subprocess.Popen(["sleep", "30"])
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    sid = SID + "-dead"
    s.sendall((json.dumps({"type": "ask", "kind": "permission", "harness": "claude", "session_id": sid, "cwd": "/tmp/p",
                           "tool_name": "Bash", "tool_input": {"command": "ls"}, "agent_pid": sleeper.pid}) + "\n").encode())
    it = wait_for(lambda: next(iter(pending_for(sid)), None), "item")
    sleeper.kill()
    sleeper.wait()
    h = wait_for(lambda: next((i for i in history() if i["id"] == it["id"]), None), "dead agent cleanup", timeout=4)
    assert h["status"] == "gone" and h["outcome"] == "session ended"
    s.close()


def test_waiting_lifecycle():
    sid = SID + "-wait"
    p = {"session_id": sid, "cwd": "/tmp/cue-wait", "transcript_path": "", "last_assistant_message": "All done. Tests pass."}
    finish(start_hook("stop", p))
    w = wait_for(lambda: pending_for(sid, "waiting"), "waiting card")[0]
    assert w["message"] == "All done. Tests pass."
    finish(start_hook("stop", p))  # a second Stop replaces, never duplicates
    time.sleep(0.3)
    assert len(pending_for(sid, "waiting")) == 1
    finish(start_hook("prompt", {"session_id": sid, "cwd": "/tmp/cue-wait"}))  # you replied in the terminal
    wait_for(lambda: not pending_for(sid, "waiting"), "waiting card to clear")


def test_ask_replaces_waiting_and_stop_does_not_duplicate_an_open_ask():
    sid = SID + "-mix"
    finish(start_hook("stop", {"session_id": sid, "cwd": "/tmp/m", "last_assistant_message": "done"}))
    wait_for(lambda: pending_for(sid, "waiting"), "waiting card")
    proc, it = ask_and_get(payload(sid=sid))
    assert not pending_for(sid, "waiting"), "an ask supersedes the waiting card"
    finish(start_hook("stop", {"session_id": sid, "cwd": "/tmp/m", "last_assistant_message": "done"}))
    time.sleep(0.3)
    assert not pending_for(sid, "waiting"), "no waiting card while a decision is open"
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "allow"}})
    finish(proc)


def test_session_end_clears_everything_for_that_session():
    sid = SID + "-end"
    proc, it = ask_and_get(payload(sid=sid))
    finish(start_hook("end", {"session_id": sid, "cwd": "/tmp/e"}))
    wait_for(lambda: not pending_for(sid), "session items to clear")
    assert finish(proc) is None


def test_two_sessions_at_once_answered_out_of_order():
    a, ia = ask_and_get(payload(sid=SID + "-A", tool_input={"command": "echo A"}))
    b, ib = ask_and_get(payload(sid=SID + "-B", tool_input={"command": "echo B"}))
    call({"type": "respond", "id": ib["id"], "decision": {"behavior": "deny"}})
    call({"type": "respond", "id": ia["id"], "decision": {"behavior": "allow"}})
    assert finish(b)["behavior"] == "deny"
    assert finish(a)["behavior"] == "allow"


def test_survives_cue_restart():
    """A waiting request re-asks when Cue comes back, and the answer still reaches the agent."""
    if not os.environ.get("CUE_BIN"):
        print("   (skipped: run via tests/run.sh, which can restart its own Cue)")
        return
    proc, it = ask_and_get(payload(tool_input={"command": "echo survive"}))
    restart_cue()
    it2 = wait_for(lambda: next(iter([i for i in pending_for() if i["tool_input"].get("command") == "echo survive"]), None),
                   "request to reappear after restart", timeout=15)
    call({"type": "respond", "id": it2["id"], "decision": {"behavior": "allow"}})
    assert finish(proc) == {"behavior": "allow"}


def test_history_survives_restart():
    if not os.environ.get("CUE_BIN"):
        print("   (skipped: run via tests/run.sh)")
        return
    proc, it = ask_and_get(payload(tool_input={"command": "echo remember-me"}))
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "deny", "message": "not now"}})
    finish(proc)
    restart_cue()
    h = next((i for i in history() if i["tool_input"].get("command") == "echo remember-me"), None)
    assert h and h["outcome"] == "denied: “not now”", h
    proc2, it2 = ask_and_get(payload(tool_input={"command": "echo fresh"}))
    assert it2["id"] not in {i["id"] for i in history()}, "new ids never collide with saved ones"
    call({"type": "respond", "id": it2["id"], "decision": {"behavior": "allow"}})
    finish(proc2)


def test_sessions_and_finished_turns_survive_restart():
    """A restart doesn't forget a live session (its chat) or its finished-turn card."""
    if not os.environ.get("CUE_BIN"):
        print("   (skipped: run via tests/run.sh)")
        return
    sid = SID + "-live"
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    s.sendall((json.dumps({"type": "event", "event": "stopped", "harness": "claude", "session_id": sid, "cwd": "/tmp/live",
                           "message": "staged and waiting for review"}) + "\n").encode())
    s.close()
    wait_for(lambda: pending_for(sid, "waiting"), "finished card")
    restart_cue()
    st = call({"type": "list"})
    card = next((i for i in st["items"] if i["session_id"] == sid and i["kind"] == "waiting"), None)
    assert card and card["message"] == "staged and waiting for review", st["items"]
    sess = next((x for x in st.get("sessions", []) if x["session_id"] == sid), None)
    assert sess and sess["state"] == "waiting", st.get("sessions")
    assert any(e["text"] == "staged and waiting for review" for e in sess["thread"]), sess["thread"]


def test_nothing_is_typed_into_a_session_while_it_waits_on_a_decision():
    """Its terminal is showing a permission prompt: a message must wait, not answer the prompt."""
    proc, it = ask_and_get(payload(tool_input={"command": "echo hold"}))
    r = call({"type": "send_to", "session_id": SID, "text": "y"})
    assert r["ok"] is False and "answer that" in r["detail"], r
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "allow"}})
    assert finish(proc) == {"behavior": "allow"}


CUE_PROC = None


def restart_cue():
    global CUE_PROC
    # The app only (the binary with no arguments): hooks are this binary too ("cue hook …").
    subprocess.run(["pkill", "-f", os.environ["CUE_BIN"] + "$"])
    time.sleep(1)
    CUE_PROC = subprocess.Popen([os.environ["CUE_BIN"]], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait_for(lambda: os.path.exists(SOCK) and _alive(), "Cue to come back", timeout=15)


def _alive():
    try:
        call({"type": "list"})
        return True
    except OSError:
        return False


def test_reply_types_into_the_session_and_confirms():
    """A finished card's reply is typed into the agent's tmux pane, submitted, and confirmed."""
    sid = SID + "-reply"
    out = tempfile.mktemp(suffix=".txt")
    # A stand-in agent: reads one line from its terminal, saves it, then reports a new turn
    # the way Claude's UserPromptSubmit hook does.
    agent = (f'read line; printf %s "$line" > {out}; '
             f'echo \'{{"session_id":"{sid}","cwd":"/tmp/r"}}\' | {HOOK} prompt; sleep 30')
    sess = f"cue-reply-{os.getpid()}"
    subprocess.run(["tmux", "new-session", "-d", "-s", sess, "-e", f"CUE_HOME={os.path.dirname(SOCK)}", "bash", "-c", agent], check=True)
    pane = subprocess.run(["tmux", "display-message", "-p", "-t", sess, "#{pane_id}"], capture_output=True, text=True).stdout.strip()
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.connect(SOCK)
        s.sendall((json.dumps({"type": "event", "event": "stopped", "harness": "claude", "session_id": sid, "cwd": "/tmp/r",
                               "message": "done", "tmux_pane": pane}) + "\n").encode())
        s.close()
        card = wait_for(lambda: next(iter(pending_for(sid, "waiting")), None), "finished card")
        r = call({"type": "reply", "id": card["id"], "text": 'ship it, then run "make test"'})
        assert r["ok"] and r["detail"].startswith("sent via tmux pane"), r
        assert open(out).read() == 'ship it, then run "make test"', "exact text reached the terminal"
        h = next(i for i in history() if i["id"] == card["id"])
        assert h["outcome"] == 'replied: “ship it, then run "make test"”', h["outcome"]
        assert not pending_for(sid, "waiting")
    finally:
        subprocess.run(["tmux", "kill-session", "-t", sess])


def test_reply_to_unreachable_terminal_fails_cleanly():
    sid = SID + "-noterm"
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    s.sendall((json.dumps({"type": "event", "event": "stopped", "harness": "claude", "session_id": sid, "cwd": "/tmp/x", "message": "done"}) + "\n").encode())
    s.close()
    card = wait_for(lambda: next(iter(pending_for(sid, "waiting")), None), "finished card")
    r = call({"type": "reply", "id": card["id"], "text": "hello"})
    assert not r["ok"] and "Go to session" in r["detail"], r
    assert pending_for(sid, "waiting"), "card stays so you can still go there"


def test_codex_uses_the_same_hook_and_decision_format():
    proc = start_hook("permission", payload(sid=SID + "-codex", tool_input={"command": "cargo test"}), harness="codex")
    it = wait_for(lambda: next(iter([i for i in pending_for(SID + "-codex") if i["kind"] != "waiting"]), None), "codex request")
    assert it["harness"] == "codex"
    call({"type": "respond", "id": it["id"], "decision": {"behavior": "deny", "message": "use cargo nextest"}})
    assert finish(proc) == {"behavior": "deny", "message": "use cargo nextest"}
    finish(start_hook("stop", {"session_id": SID + "-codex", "cwd": "/tmp/cx", "last_assistant_message": "done"}, harness="codex"))
    w = wait_for(lambda: pending_for(SID + "-codex", "waiting"), "codex finished card")[0]
    assert w["harness"] == "codex" and w["message"] == "done"


def test_direct_reply_reaches_a_subscribed_session():
    """Pi listens on the socket: a reply is delivered down that connection, not typed anywhere."""
    import threading
    sid = SID + "-sub"
    sub = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sub.connect(SOCK)
    sub.sendall((json.dumps({"type": "subscribe", "harness": "pi", "session_id": sid, "cwd": "/tmp/pi"}) + "\n").encode())
    time.sleep(0.2)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    s.sendall((json.dumps({"type": "event", "event": "stopped", "harness": "pi", "session_id": sid, "cwd": "/tmp/pi", "message": "done"}) + "\n").encode())
    s.close()
    card = wait_for(lambda: next(iter(pending_for(sid, "waiting")), None), "finished card")
    got = {}

    def agent():  # what the Pi extension does: receive, then the new turn reports "active"
        got["msg"] = json.loads(sub.makefile("r").readline())
        a = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        a.connect(SOCK)
        a.sendall((json.dumps({"type": "event", "event": "active", "harness": "pi", "session_id": sid, "cwd": "/tmp/pi", "message": got["msg"].get("text", "")}) + "\n").encode())
        a.close()

    t = threading.Thread(target=agent)
    t.start()
    r = call({"type": "reply", "id": card["id"], "text": "go ahead with AMD"})
    t.join(5)
    assert got["msg"] == {"type": "reply", "text": "go ahead with AMD", "images": [], "now": False}, got
    assert r["ok"] and r["detail"] == "sent via the session directly", r
    h = next(i for i in history() if i["id"] == card["id"])
    assert h["outcome"] == "replied: “go ahead with AMD”"
    sess = next(x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid)
    assert sess["state"] == "working" and sess["prompt"] == "go ahead with AMD"
    sub.close()


def test_send_to_a_working_session():
    """The session panel's box: a working Pi session gets the message directly, and it's remembered
    as context; a session Cue doesn't know is refused."""
    sid = SID + "-say"
    sub = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sub.connect(SOCK)
    sub.sendall((json.dumps({"type": "subscribe", "harness": "pi", "session_id": sid, "cwd": "/tmp/say"}) + "\n").encode())
    a = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    a.connect(SOCK)
    a.sendall((json.dumps({"type": "event", "event": "active", "harness": "pi", "session_id": sid, "cwd": "/tmp/say", "message": "refactor the router"}) + "\n").encode())
    a.close()
    wait_for(lambda: any(x["session_id"] == sid and x["state"] == "working" for x in call({"type": "list"})["sessions"]), "working session")
    r = call({"type": "send_to", "session_id": sid, "text": "also keep the old API"})
    # The session is mid-turn, so the message is queued for when it finishes its current step.
    assert r["ok"] and r["detail"].startswith("queued via the session directly"), r
    assert json.loads(sub.makefile("r").readline()) == {"type": "reply", "text": "also keep the old API", "images": [], "now": False}
    sess = next(x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid)
    assert [e["text"] for e in sess["thread"]][-2:] == ["refactor the router", "also keep the old API"]
    assert not call({"type": "send_to", "session_id": "nope", "text": "hi"})["ok"]
    sub.close()


def test_stop_presses_esc_in_the_terminal_and_marks_it_stopped():
    """Stop (Esc twice in Cue) reaches a terminal agent as a real Esc key; Cue shows it stopped."""
    sid = SID + "-stop"
    out = tempfile.mktemp(suffix=".hex")
    # A stand-in agent that records the first raw key it gets, as hex.
    agent = f'IFS= read -rsn1 k; printf %s "$k" | xxd -p > {out}; sleep 30'
    sess = f"cue-stop-{os.getpid()}"
    subprocess.run(["tmux", "new-session", "-d", "-s", sess, "bash", "-c", agent], check=True)
    pane = subprocess.run(["tmux", "display-message", "-p", "-t", sess, "#{pane_id}"], capture_output=True, text=True).stdout.strip()
    try:
        a = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        a.connect(SOCK)
        a.sendall((json.dumps({"type": "event", "event": "active", "harness": "claude", "session_id": sid, "cwd": "/tmp/stop",
                               "message": "a long refactor", "tmux_pane": pane}) + "\n").encode())
        a.close()
        wait_for(lambda: any(x["session_id"] == sid and x["state"] == "working" for x in call({"type": "list"})["sessions"]), "working session")
        r = call({"type": "interrupt", "session_id": sid})
        assert r["ok"] and r["detail"] == f"stopped via tmux pane {pane}", r
        wait_for(lambda: os.path.exists(out) and open(out).read().strip(), "the key to arrive")
        assert open(out).read().strip() == "1b", "the terminal got Esc"
        s = next(x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid)
        assert s["state"] == "stopped" and s["thread"][-1]["text"] == "⏹ stopped it", s
    finally:
        subprocess.run(["tmux", "kill-session", "-t", sess])


def test_pi_stop_and_send_now_go_straight_to_the_session():
    """Pi listens directly: Stop sends it an interrupt; Ctrl+Enter sends the message as 'now' (a steer)."""
    sid = SID + "-pistop"
    sub = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sub.connect(SOCK)
    sub.sendall((json.dumps({"type": "subscribe", "harness": "pi", "session_id": sid, "cwd": "/tmp/ps"}) + "\n").encode())
    a = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    a.connect(SOCK)
    a.sendall((json.dumps({"type": "event", "event": "active", "harness": "pi", "session_id": sid, "cwd": "/tmp/ps", "message": "go"}) + "\n").encode())
    a.close()
    wait_for(lambda: any(x["session_id"] == sid and x["state"] == "working" for x in call({"type": "list"})["sessions"]), "working session")
    lines = sub.makefile("r")
    r = call({"type": "send_to", "session_id": sid, "text": "use the new API instead", "now": True})
    assert r["ok"] and r["detail"] == "sent via the session directly", r
    assert json.loads(lines.readline()) == {"type": "reply", "text": "use the new API instead", "images": [], "now": True}
    r = call({"type": "interrupt", "session_id": sid})
    assert r["ok"] and r["detail"] == "stopped via the session directly", r
    assert json.loads(lines.readline()) == {"type": "interrupt"}
    sub.close()


def test_stop_hook_follow_up_does_not_hide_the_answer():
    """A Stop hook made Claude write a checklist after its real answer: the card shows the answer,
    and the checklist rides along as the follow-up."""
    sid = SID + "-hooked"
    with tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False) as f:
        for e in [
            {"type": "user", "message": {"content": "make the panel clickable"}},
            {"type": "assistant", "message": {"content": [{"type": "text", "text": "Done: working sessions open a panel."}]}},
            {"type": "user", "isMeta": True, "message": {"content": "Stop hook feedback:\nanswer the checklist"}},
            {"type": "assistant", "message": {"content": [{"type": "text", "text": "Checked: lint passes.\nChecked: tests pass."}]}},
        ]:
            f.write(json.dumps(e) + "\n")
        path = f.name
    finish(start_hook("stop", {"session_id": sid, "cwd": "/tmp/h", "transcript_path": path, "last_assistant_message": "Checked: lint passes.\nChecked: tests pass."}))
    card = wait_for(lambda: next(iter(pending_for(sid, "waiting")), None), "finished card")
    assert card["message"] == "Done: working sessions open a panel.", card["message"]
    assert card["followup"] == "Checked: lint passes.\nChecked: tests pass."
    os.unlink(path)


PNG_B64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="


def test_images_reach_a_direct_session_as_files():
    sid = SID + "-img"
    sub = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sub.connect(SOCK)
    sub.sendall((json.dumps({"type": "subscribe", "harness": "pi", "session_id": sid, "cwd": "/tmp/img"}) + "\n").encode())
    a = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    a.connect(SOCK)
    a.sendall((json.dumps({"type": "event", "event": "active", "harness": "pi", "session_id": sid, "cwd": "/tmp/img"}) + "\n").encode())
    a.close()
    wait_for(lambda: any(x["session_id"] == sid for x in call({"type": "list"})["sessions"]), "session")
    r = call({"type": "send_to", "session_id": sid, "text": "", "images": [{"name": "dot.png", "mime": "image/png", "data": "data:image/png;base64," + PNG_B64}]})
    assert r["ok"], r
    msg = json.loads(sub.makefile("r").readline())
    assert msg["type"] == "reply" and msg["text"] == "" and len(msg["images"]) == 1
    im = msg["images"][0]
    assert im["mime"] == "image/png" and os.path.exists(im["path"]) and open(im["path"], "rb").read()[:4] == b"\x89PNG"
    assert not call({"type": "send_to", "session_id": sid, "text": "", "images": [{"name": "x", "mime": "text/plain", "data": "aGk="}]})["ok"], "non-images refused"
    sub.close()


def test_typed_reply_carries_image_paths():
    """Claude/Codex: the image is saved and its path typed after your words (one paste, then Enter)."""
    sid = SID + "-imgtyped"
    out = tempfile.mktemp(suffix=".txt")
    agent = (f'read line; printf %s "$line" > {out}; '
             f'echo \'{{"session_id":"{sid}","cwd":"/tmp/r"}}\' | {HOOK} prompt; sleep 30')
    sess = f"cue-img-{os.getpid()}"
    subprocess.run(["tmux", "new-session", "-d", "-s", sess, "-e", f"CUE_HOME={os.path.dirname(SOCK)}", "bash", "-c", agent], check=True)
    pane = subprocess.run(["tmux", "display-message", "-p", "-t", sess, "#{pane_id}"], capture_output=True, text=True).stdout.strip()
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.connect(SOCK)
        s.sendall((json.dumps({"type": "event", "event": "stopped", "harness": "claude", "session_id": sid, "cwd": "/tmp/r", "message": "done", "tmux_pane": pane}) + "\n").encode())
        s.close()
        card = wait_for(lambda: next(iter(pending_for(sid, "waiting")), None), "finished card")
        r = call({"type": "reply", "id": card["id"], "text": "what's wrong here?", "images": [{"name": "dot.png", "mime": "image/png", "data": PNG_B64}]})
        assert r["ok"], r
        typed = open(out).read()
        assert typed.startswith("what's wrong here? /") and typed.endswith(".png") and "/uploads/" in typed, typed
        assert os.path.exists(typed.split(" ")[-1])
        h = next(i for i in history() if i["id"] == card["id"])
        assert h["outcome"] == "replied: “what's wrong here?” + 1 image", h["outcome"]
    finally:
        subprocess.run(["tmux", "kill-session", "-t", sess])


def test_a_session_driven_by_another_agent_is_not_your_turn():
    sid = SID + "-driven"
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    s.sendall((json.dumps({"type": "event", "event": "stopped", "harness": "claude", "session_id": sid, "cwd": "/tmp/exec",
                           "message": "staged + report written, awaiting the lead's review", "driven_by": "its relay lead (demo)"}) + "\n").encode())
    s.close()
    sess = wait_for(lambda: next((x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid), None), "session")
    assert sess["state"] == "agent" and sess["driven_by"] == "its relay lead (demo)"
    assert sess["segments"][-1]["kind"] == "idle"
    time.sleep(0.3)
    assert not pending_for(sid), "no Your-turn card for an executor its lead handles"


def test_the_hook_reads_who_drives_a_session_from_its_environment():
    """A tool that runs agents for another agent labels them with CUE_DRIVEN_BY; the hook passes it
    on, so the helper's finished turn isn't your turn."""
    sid = SID + "-labelled"
    env = {**os.environ, "CUE_DRIVEN_BY": "its lead (lines)"}
    payload = json.dumps({"session_id": sid, "cwd": "/tmp/helper", "last_assistant_message": "staged, report written"})
    subprocess.run([HOOK, "stop", "claude"], input=payload, text=True, env=env, timeout=10, check=True)
    sess = wait_for(lambda: next((x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid), None), "session")
    assert sess["state"] == "agent" and sess["driven_by"] == "its lead (lines)", sess
    assert not pending_for(sid), "no Your-turn card for a labelled helper"
    # No label: the same finished turn is yours.
    sid2 = SID + "-unlabelled"
    env2 = {k: v for k, v in os.environ.items() if k != "CUE_DRIVEN_BY"}
    subprocess.run([HOOK, "stop", "claude"], input=payload.replace(sid, sid2), text=True, env=env2, timeout=10, check=True)
    wait_for(lambda: pending_for(sid2, "waiting"), "your-turn card")


def test_a_message_to_a_busy_session_is_queued_until_its_next_turn():
    sid = SID + "-queue"
    sub = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sub.connect(SOCK)
    sub.sendall((json.dumps({"type": "subscribe", "harness": "pi", "session_id": sid, "cwd": "/tmp/q"}) + "\n").encode())
    ev = lambda event, msg="": (lambda s: (s.connect(SOCK), s.sendall((json.dumps({"type": "event", "event": event, "harness": "pi", "session_id": sid, "cwd": "/tmp/q", "message": msg}) + "\n").encode()), s.close()))(socket.socket(socket.AF_UNIX, socket.SOCK_STREAM))
    ev("active", "refactor the router")
    sess = lambda: next((x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid), None) or {}
    wait_for(lambda: sess().get("state") == "working", "working")
    r = call({"type": "send_to", "session_id": sid, "text": "keep the old API"})
    assert r["ok"] and r["detail"].startswith("queued via the session directly"), r
    assert sess()["queued"]["text"] == "keep the old API"
    ev("stopped", "done, old API kept")  # the agent finished: it has seen the note
    wait_for(lambda: sess().get("queued", "x") is None, "queued note cleared")
    r = call({"type": "send_to", "session_id": sid, "text": "thanks, ship it"})  # now waiting: a reply
    assert r["detail"] == "sent via the session directly", r
    sub.close()


def session_state(sid):
    return next((x for x in call({"type": "list"})["sessions"] if x["session_id"] == sid), None)


def test_a_usage_limit_pauses_the_session_without_a_card():
    sid = SID + "-limit"
    finish(start_hook("prompt", {"session_id": sid, "cwd": "/tmp/cue-limit", "prompt": "ok do all"}))
    wait_for(lambda: (session_state(sid) or {}).get("state") == "working", "working")
    finish(start_hook("failure", {"session_id": sid, "cwd": "/tmp/cue-limit", "hook_event_name": "StopFailure", "error_type": "rate_limit",
                                  "error_details": "You've hit your session limit · resets 4:10am (America/Los_Angeles)"}))
    s = wait_for(lambda: (lambda x: x if x and x["state"] == "limited" else None)(session_state(sid)), "limited")
    assert s["limit"]["scope"] == "all" and s["limit"]["kind"] == "session", s["limit"]
    assert s["limit"]["resets_ms"] > time.time() * 1000 - 60_000
    assert s["prompt"] == "ok do all"
    assert not pending_for(sid), "nothing to answer while out of usage"
    # Another Claude session finishing normally means the plan's limit is behind us.
    other = SID + "-limit-other"
    finish(start_hook("stop", {"session_id": other, "cwd": "/tmp/cue-limit2", "transcript_path": "", "last_assistant_message": "done"}))
    wait_for(lambda: session_state(sid)["limit"]["resets_ms"] <= time.time() * 1000 + 1000, "limit lifted")
    assert session_state(sid)["state"] == "limited", "still paused until you resend"
    finish(start_hook("prompt", {"session_id": sid, "cwd": "/tmp/cue-limit", "prompt": "ok do all"}))
    s = wait_for(lambda: (lambda x: x if x["state"] == "working" else None)(session_state(sid)), "working again")
    assert "limit" not in s


def test_a_model_limit_is_told_apart():
    sid = SID + "-fable"
    finish(start_hook("failure", {"session_id": sid, "cwd": "/tmp/cue-fable", "error_type": "rate_limit",
                                  "error_details": "You've reached your Fable limit. Run /usage-credits to continue or switch models with /model."}))
    s = wait_for(lambda: (lambda x: x if x and x["state"] == "limited" else None)(session_state(sid)), "limited")
    assert (s["limit"]["scope"], s["limit"]["kind"]) == ("Fable", "model"), s["limit"]


def test_any_other_api_error_is_your_turn():
    sid = SID + "-overloaded"
    finish(start_hook("failure", {"session_id": sid, "cwd": "/tmp/cue-err", "error_type": "overloaded", "error_details": "Overloaded"}))
    w = wait_for(lambda: pending_for(sid, "waiting"), "waiting card")[0]
    assert "Overloaded" in w["message"], w["message"]


def test_status_line_usage_reaches_the_window():
    reset = int(time.time()) + 3600
    p = {"session_id": SID, "rate_limits": {"five_hour": {"used_percentage": 62.5, "resets_at": reset}, "seven_day": {"used_percentage": 41}}}
    finish(start_hook("usage", p))
    u = wait_for(lambda: call({"type": "list"}).get("usage"), "usage")
    assert u["five_hour"] == {"pct": 62.5, "resets_ms": reset * 1000}, u
    assert u["seven_day"]["pct"] == 41


def test_a_turn_that_died_without_a_hook_is_caught_from_its_transcript():
    sid = SID + "-silent"
    t = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False)
    t.write(json.dumps({"type": "user", "message": {"role": "user", "content": "ok do all"}}) + "\n")
    t.close()
    finish(start_hook("prompt", {"session_id": sid, "cwd": "/tmp/cue-silent", "prompt": "ok do all", "transcript_path": t.name}))
    wait_for(lambda: (session_state(sid) or {}).get("state") == "working", "working")
    # The API refused the turn: Claude Code writes its synthetic reply, and no Stop hook runs.
    with open(t.name, "a") as f:
        f.write(json.dumps({"type": "assistant", "isApiErrorMessage": True, "error": "rate_limit",
                            "message": {"model": "<synthetic>", "content": [{"type": "text", "text": "You've hit your weekly limit · resets Oct 3 at 8am (America/Los_Angeles)"}]}}) + "\n")
    s = wait_for(lambda: (lambda x: x if x["state"] == "limited" else None)(session_state(sid)), "limited from the transcript", timeout=40)
    assert s["limit"]["kind"] == "weekly"
    os.unlink(t.name)


def test_garbage_line_gets_an_error_not_a_crash():
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK)
    s.sendall(b"this is not json\n")
    assert json.loads(s.makefile("r").readline())["type"] == "error"
    assert isinstance(items(), list), "server still alive"


if __name__ == "__main__":
    tests = [(n, f) for n, f in globals().items() if n.startswith("test_")]
    failed = 0
    for name, fn in tests:
        try:
            fn()
            print(f"PASS {name}")
        except Exception as e:
            failed += 1
            print(f"FAIL {name}: {type(e).__name__}: {e}")
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    sys.exit(1 if failed else 0)
