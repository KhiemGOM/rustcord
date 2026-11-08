"""Check the mock's account reset: python e2e/mock_reset.py (build mockcord first)."""

import json
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

import run


def request(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{run.HTTP}/api/v9/{path}", data=data, method=method)
    with urllib.request.urlopen(req) as res:
        return json.load(res)


def main():
    with tempfile.TemporaryDirectory(prefix="rustcord_reset_") as tmp:
        mock = subprocess.Popen([str(run.ROOT / "target/release/examples/mockcord.exe"), str(run.HTTP), str(run.WS),
                                 str(Path(tmp) / "requests.jsonl"), str(run.VOICE), str(run.REMOTE)])
        try:
            time.sleep(0.5)
            request("POST", "users/@me/wishlist/items", {"sku_id": "5201"})
            assert request("GET", "wishlists/7700")["wishlist_items"]
            request("POST", "mock/account-reset", {})
            assert request("GET", "wishlists/7700")["wishlist_items"] == [], "account reset kept the previous wishlist"
            print("account reset clears the wishlist")
        finally:
            mock.kill()
            mock.wait()


if __name__ == "__main__":
    main()
