"""A minimal MCP server, used to test the client against a real process.

Deliberately not written with an SDK. The point of the test is that true-code
talks to *someone else's* server over the wire, and a fixture built from the
same Rust crate as the client would only prove that the crate agrees with
itself. Sixty lines of JSON-RPC over stdin/stdout is the whole protocol surface
the client depends on.

Speaks: initialize, notifications/initialized, tools/list, tools/call.
Offers two tools: `echo` (succeeds) and `explode` (returns a tool-level error).
"""

import json
import sys

TOOLS = [
    {
        "name": "echo",
        "description": "Returns whatever it is given.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        },
    },
    {
        "name": "explode",
        "description": "Always fails, to prove failures are not read as output.",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def call(params):
    name = params.get("name")
    arguments = params.get("arguments") or {}
    if name == "echo":
        return {"content": [{"type": "text", "text": arguments.get("text", "")}]}
    if name == "explode":
        return {
            "content": [{"type": "text", "text": "the tool refused"}],
            "isError": True,
        }
    return {"content": [{"type": "text", "text": f"no such tool: {name}"}], "isError": True}


def handle(request):
    method = request.get("method")
    params = request.get("params") or {}

    if method == "initialize":
        # Echo the client's protocol version back rather than pinning one, so
        # this fixture keeps working as rmcp moves forward.
        return {
            "protocolVersion": params.get("protocolVersion", "2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "echo-fixture", "version": "1.0.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method == "tools/call":
        return call(params)
    return None


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        request = json.loads(line)

        # A notification has no id and must not be answered.
        if "id" not in request:
            continue

        result = handle(request)
        if result is None:
            response = {
                "jsonrpc": "2.0",
                "id": request["id"],
                "error": {"code": -32601, "message": "method not found"},
            }
        else:
            response = {"jsonrpc": "2.0", "id": request["id"], "result": result}

        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
