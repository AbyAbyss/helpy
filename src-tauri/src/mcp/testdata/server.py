"""A tiny MCP server over stdio, for Helpy's tests: one read-only tool that
adds numbers and one that fails."""
import json
import sys

TOOLS = [
    {
        "name": "add",
        "description": "Adds two numbers",
        "inputSchema": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}},
        "annotations": {"readOnlyHint": True},
    },
    {"name": "fail", "description": "Always fails", "inputSchema": {"type": "object"}},
]


def reply(id_, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": id_, "result": result}) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    msg = json.loads(line)
    method, id_ = msg.get("method"), msg.get("id")
    if id_ is None:
        continue  # a notification
    if method == "initialize":
        reply(id_, {
            "protocolVersion": msg["params"]["protocolVersion"],
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fake", "version": "1"},
        })
    elif method == "tools/list":
        reply(id_, {"tools": TOOLS})
    elif method == "tools/call":
        p = msg["params"]
        if p["name"] == "add":
            a = p.get("arguments", {})
            reply(id_, {"content": [{"type": "text", "text": str(a.get("a", 0) + a.get("b", 0))}]})
        else:
            reply(id_, {"content": [{"type": "text", "text": "it broke"}], "isError": True})
    else:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": id_, "error": {"code": -32601, "message": "no such method"}}) + "\n")
        sys.stdout.flush()
