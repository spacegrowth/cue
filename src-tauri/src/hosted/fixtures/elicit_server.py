"""An MCP server for the hosted-session compatibility tests (compat_tests.rs): one tool, `need_input`, that
asks the user to pick a color through MCP elicitation and returns the answer as its result. Raw JSON-RPC over
stdio, so it needs nothing but Python."""
import json, sys

caps = {}

def out(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)

while True:
    m = read()
    method, mid = m.get("method"), m.get("id")
    if method == "initialize":
        caps = m["params"].get("capabilities", {})
        out({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": m["params"]["protocolVersion"],
             "capabilities": {"tools": {}}, "serverInfo": {"name": "compat", "version": "0"}}})
    elif method == "tools/list":
        out({"jsonrpc": "2.0", "id": mid, "result": {"tools": [{"name": "need_input",
             "description": "Asks the user to pick a color. Call it when told to.",
             "inputSchema": {"type": "object", "properties": {}}}]}})
    elif method == "tools/call":
        if "elicitation" not in caps:
            text = "no elicitation capability"
        else:
            out({"jsonrpc": "2.0", "id": "e1", "method": "elicitation/create", "params": {
                "message": "Pick a color", "requestedSchema": {"type": "object",
                "properties": {"color": {"type": "string", "enum": ["red", "blue"]}}, "required": ["color"]}}})
            while True:
                r = read()
                if r.get("id") == "e1" and "method" not in r:
                    text = "elicitation answer: " + json.dumps(r.get("result", r.get("error")))
                    break
        out({"jsonrpc": "2.0", "id": mid, "result": {"content": [{"type": "text", "text": text}]}})
    elif mid is not None:
        out({"jsonrpc": "2.0", "id": mid, "result": {}})
