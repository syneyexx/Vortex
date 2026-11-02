import http.server
import threading
from pathlib import Path

import pytest

pytest.importorskip("playwright")
if not Path("/opt/pw-browsers/chromium").exists():
    pytest.skip("geen Chromium", allow_module_level=True)

from vortex_worker import browser

PAGE = b"""<html><head><title>Formulier</title></head><body>
<input id="naam"><button id="go" onclick="document.getElementById('out').innerText='Hallo ' + document.getElementById('naam').value">Ga</button>
<div id="out"></div></body></html>"""


class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("content-type", "text/html")
        self.end_headers()
        self.wfile.write(PAGE)

    def log_message(self, *a):
        pass


def test_fill_click_extract_screenshot(tmp_path):
    srv = http.server.HTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{srv.server_port}/"
    r = browser.run(
        [
            {"action": "goto", "url": url},
            {"action": "fill", "selector": "#naam", "text": "Vortex"},
            {"action": "click", "selector": "#go"},
            {"action": "extract", "selector": "#out"},
            {"action": "screenshot"},
            {"action": "click", "selector": "#bestaat-niet"},
        ],
        str(tmp_path),
        timeout_ms=3000,
    )
    srv.shutdown()
    assert r["title"] == "Formulier"
    assert r["results"][3]["text"] == "Hallo Vortex"
    assert Path(r["screenshots"][0]).exists()
    assert r["results"][-1]["ok"] is False
