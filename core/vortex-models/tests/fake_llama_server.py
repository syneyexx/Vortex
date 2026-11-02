"""Test-dubbel van llama-server (alleen stdlib). Gebruikt door tests/lifecycle.rs.

Gedrag: /health geeft 503 tijdens "laden" (0,3 s), daarna 200. Modelbestand met inhoud
'CRASH' → proces stopt direct (laadfout). 'slow' als vraag → antwoord na 1 s (draining-test).
"""

import json
import os
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

args = sys.argv[1:]


def arg(name, default=None):
    return args[args.index(name) + 1] if name in args else default


model = arg("-m")
port = int(arg("--port", "8080"))
slot_dir = arg("--slot-save-path")
with open(model, encoding="utf-8") as f:
    content = f.read()
if "CRASH" in content:
    print("model laden mislukt", flush=True)
    sys.exit(1)
started = time.time()
embedding = "--embedding" in args


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def reply(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            self.reply(200 if time.time() - started > 0.3 else 503, {"status": "ok"})
        elif self.path == "/props":
            self.reply(200, {"chat_template": "{{ tool }}", "default_generation_settings": {"n_ctx": 4096},
                             "model_path": model, "modalities": {"vision": False}})
        else:
            self.reply(404, {})

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        body = json.loads(self.rfile.read(n) or b"{}")
        u = urlparse(self.path)
        if u.path == "/tokenize":
            self.reply(200, {"tokens": [1, 2, 3]})
        elif u.path == "/v1/chat/completions":
            last = body["messages"][-1]["content"]
            if last == "slow":
                time.sleep(1.0)
            self.reply(200, {"choices": [{"message": {"content": "ok " + os.path.basename(model)}, "finish_reason": "stop"}],
                             "usage": {"prompt_tokens": 3, "completion_tokens": 1}})
        elif u.path == "/v1/embeddings":
            self.reply(200, {"data": [{"index": i, "embedding": [0.1] * 512} for i, _ in enumerate(body["input"])]})
        elif u.path.startswith("/slots/"):
            action = parse_qs(u.query).get("action", [""])[0]
            if not slot_dir:
                self.reply(501, {})
                return
            path = os.path.join(slot_dir, body["filename"])
            if action == "save":
                with open(path, "w") as f:
                    f.write("kv")
                self.reply(200, {"filename": body["filename"]})
            elif action == "restore":
                self.reply(200 if os.path.exists(path) else 404, {})
        else:
            self.reply(404, {})


srv = ThreadingHTTPServer(("127.0.0.1", port), H)
threading.Thread(target=srv.serve_forever, daemon=True).start()
print(f"fake llama-server op {port} met {model}", flush=True)
while True:
    time.sleep(3600)
