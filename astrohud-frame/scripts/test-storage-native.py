#!/usr/bin/env python3
"""Run the real headless viewer and storage API against disposable local data."""
import http.cookiejar
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import urllib.request
from urllib.parse import urlsplit
import uuid
import zlib

base = os.environ.get("STORAGE_TEST_URL", "http://127.0.0.1:18116")
assert urlsplit(base).hostname in ("localhost", "127.0.0.1")
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))


def request(path, body=None, content_type="application/json"):
    if isinstance(body, dict):
        body = json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=body, headers={"Content-Type": content_type})
    with opener.open(req, timeout=10) as response:
        return json.load(response)


device = dict(device_id=str(uuid.uuid4()), device_code="AB23CD",
              credential=uuid.uuid4().hex + uuid.uuid4().hex,
              bootstrap_token=uuid.uuid4().hex + uuid.uuid4().hex)
request("/api/beta/devices/enrollments", device)
request("/api/beta/bootstrap/claim", dict(token=device["bootstrap_token"], place_name="Native storage QA"))


def chunk(kind, data):
    return struct.pack("!I", len(data)) + kind + data + struct.pack("!I", zlib.crc32(kind + data))


png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!2I5B", 2, 2, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress((b"\x00" + b"\x40\x80\xc0" * 2) * 2)) + chunk(b"IEND", b""))
body = (b'--native-storage\r\nContent-Disposition: form-data; name="photo"; filename="sample.png"\r\n'
        b'Content-Type: image/png\r\n\r\n' + png + b'\r\n--native-storage--\r\n')
request(f'/api/beta/owner/frames/{device["device_id"]}/photos', body, "multipart/form-data; boundary=native-storage")


def wait_for(check):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError("Native storage fixture timed out")


with tempfile.TemporaryDirectory(prefix="memory-lane-native-storage-") as directory:
    root = Path(directory)
    credential = root / "credential"
    credential.write_text(device["credential"])
    credential.chmod(0o600)
    environment = dict(os.environ, SDL_VIDEODRIVER="dummy", ASTROHUD_SERVER_URL=base,
                       ASTROHUD_DEVICE_CREDENTIAL_FILE=str(credential),
                       ASTROHUD_FRAME_CACHE_DIR=str(root / "cache"))
    binary = os.environ.get("FRAME_TEST_BINARY", "target/debug/astrohud-frame")
    for attempt in range(2):
        with (root / "viewer.log").open("w") as log:
            process = subprocess.Popen([binary, "--windowed"], env=environment, stdout=log, stderr=log)
            try:
                if attempt == 0:
                    wait_for(lambda: list((root / "cache" / "media").glob("*.image")))
                else:
                    def stored_photo_report():
                        value = request(f'/api/beta/owner/frames/{device["device_id"]}/storage')
                        return value if value.get("report", {}).get("photo_count") == 1 else None
                    value = wait_for(stored_photo_report)
                    assert value["report"]["viewer_version"] == "0.4.0"
                    assert value["report"]["photo_bytes"] > 0
                    assert value["estimated_photos_remaining"] > 0
                    assert value["report"]["reserve_bytes"] == 512 * 1024 * 1024
            finally:
                process.terminate()
                process.wait(timeout=5)
print("Native storage check passed: real viewer download, restart, filesystem measurements, authenticated report, and photo estimate.")
