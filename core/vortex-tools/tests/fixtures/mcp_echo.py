"""Minimale MCP-server (stdio) voor tests: tools 'echo' (readOnly), 'count' (houdt teller bij), 'boom' (isError)."""
import json, sys

count = 0
for line in sys.stdin:
    try:
        msg = json.loads(line)
    except Exception:
        continue
    mid = msg.get("id")
    method = msg.get("method")
    if mid is None:
        continue
    if method == "initialize":
        res = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "echo", "version": "1"}}
    elif method == "tools/list":
        if (msg.get("params") or {}).get("cursor") is None:
            res = {"tools": [{"name": "echo", "description": "Geeft tekst terug", "inputSchema": {"type": "object", "required": ["text"], "properties": {"text": {"type": "string"}}}, "annotations": {"readOnlyHint": True}}], "nextCursor": "p2"}
        else:
            res = {"tools": [
                {"name": "count", "description": "Verhoogt een teller", "inputSchema": {"type": "object", "properties": {}}},
                {"name": "boom", "description": "Faalt altijd", "inputSchema": {"type": "object", "properties": {}}},
            ]}
    elif method == "tools/call":
        p = msg["params"]
        if p["name"] == "echo":
            res = {"content": [{"type": "text", "text": "echo: " + p["arguments"]["text"]}]}
        elif p["name"] == "count":
            count += 1
            res = {"content": [{"type": "text", "text": str(count)}], "structuredContent": {"count": count}}
        else:
            res = {"content": [{"type": "text", "text": "kapot"}], "isError": True}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "onbekend"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": mid, "result": res}), flush=True)
