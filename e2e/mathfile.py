"""Run the Markdown attachment/math checks: python e2e/mathfile.py."""

import http.server
import os
from pathlib import Path
import subprocess
import sys
import threading

ROOT = Path(__file__).resolve().parent.parent
BODY = (ROOT / "e2e/fixtures/math.md").read_bytes()


class Fixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/markdown; charset=utf-8")
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        self.wfile.write(BODY)

    def log_message(self, *_):
        pass


def main():
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    env = dict(os.environ, RUSTCORD_E2E_MATH_URL=f"http://127.0.0.1:{server.server_port}/formula.md", RUSTCORD_E2E_TO="1")
    try:
        return subprocess.call([sys.executable, str(ROOT / "e2e/from_step.py"), "1890"], cwd=ROOT, env=env)
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    sys.exit(main())
