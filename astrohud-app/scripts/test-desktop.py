#!/usr/bin/env python3
"""Exercise the actual Tauri/WebKit window through tauri-driver (local API only)."""
import base64
import json
import os
from pathlib import Path
import struct
import time
import urllib.error
import urllib.request
import zlib

driver = os.environ.get("TAURI_TEST_DRIVER", "http://127.0.0.1:4447")
assert driver.startswith("http://127.0.0.1:")
binary = str(Path(os.environ.get("TAURI_TEST_BINARY", "target/tauri-desktop/debug/memory-lane-app")).resolve())
session = None


def command(path, body=None, method=None):
    request = urllib.request.Request(driver + path, data=None if body is None else json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json"}, method=method)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response).get("value")
    except urllib.error.HTTPError as error:
        raise AssertionError(error.read().decode()) from None


def run(script, *args):
    return command(f"/session/{session}/execute/sync", {"script": script, "args": list(args)})


def asynchronous(script, *args):
    script = "const done=arguments[arguments.length-1];(async()=>{" + script + "})().then(done,error=>done({error:String(error)}));"
    value = command(f"/session/{session}/execute/async", {"script": script, "args": list(args)})
    assert not isinstance(value, dict) or "error" not in value, value
    return value


def wait(script, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if run(script):
            return
        time.sleep(0.1)
    raise AssertionError("Timed out: " + script)


def element(selector):
    value = command(f"/session/{session}/element", {"using": "css selector", "value": selector})
    return value["element-6066-11e4-a52e-4f735466cecf"]


def click(selector):
    command(f"/session/{session}/element/{element(selector)}/click", {})


def type_into(selector, value):
    command(f"/session/{session}/element/{element(selector)}/value", {"text": value})


def new_session():
    return command("/session", {"capabilities": {"alwaysMatch": {"tauri:options": {"application": binary}}}})["sessionId"]


def chunk(kind, data):
    return struct.pack("!I", len(data)) + kind + data + struct.pack("!I", zlib.crc32(kind + data))


photo = Path("target/tauri-tools/sample.png").resolve()
photo.parent.mkdir(parents=True, exist_ok=True)
photo.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!2I5B", 2, 2, 8, 2, 0, 0, 0))
                  + chunk(b"IDAT", zlib.compress((b"\x00" + b"\x40\x80\xc0" * 2) * 2)) + chunk(b"IEND", b""))
try:
    session = new_session()
    wait("return !!document.querySelector('#demo') && !!window.memoryLaneLocalServer;")
    click("#demo")
    wait("return !!document.querySelector('#create-demo');")
    click("#create-demo")
    wait("return document.querySelectorAll('.local-demo-frame').length===3;", 60)
    print("Native QA: sample frames and native multipart uploads passed", flush=True)
    click("#open-owner")
    wait("return !!document.querySelector('#owner-navigation') && !document.querySelector('#owner-navigation').hidden;")
    frame = asynchronous("return (await (await fetch('/api/beta/owner')).json()).frames[0].frame_id;")
    click('.owner-nav-link[data-view="photos-panel"]')
    wait("return [...document.querySelectorAll('#frame-photos img')].some(image=>image.naturalWidth>0&&image.src.startsWith('blob:'));")
    print("Native QA: protected gallery images rendered", flush=True)
    click('.owner-nav-link[data-view="owner-send-panel"]')
    type_into("#owner-photo", str(photo))
    wait("return !document.querySelector('#owner-preview-shell').hidden;")
    click('#owner-photo-form button[type="submit"]')
    wait("return document.querySelector('#owner-send-status').textContent.includes('1 photo received');")
    print("Native QA: file selection, preview and owner upload passed", flush=True)
    secrets = asynchronous("""
      const recovery=await (await fetch('/api/beta/owner/recovery',{method:'POST'})).json();
      const invite=await (await fetch('/api/beta/owner/invitations',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({label:'Desktop sender QA'})})).json();
      return {origin:window.memoryLaneServerOrigin,recovery:recovery.token,invite:invite.token};
    """)
    click(".native-connection button")
    wait("return !!document.querySelector('#private-link');")
    assert asynchronous("return (await fetch('/api/beta/owner')).status;") == 401
    type_into("#private-link", secrets["origin"] + "/recover.html#" + secrets["recovery"])
    click('#connect button[type="submit"]')
    wait("return !!document.querySelector('#owner-navigation') && !document.querySelector('#owner-navigation').hidden;")
    print("Native QA: private owner recovery connection passed", flush=True)
    click(".native-connection button")
    wait("return !!document.querySelector('#private-link');")
    type_into("#private-link", secrets["origin"] + "/sender-activate.html#" + secrets["invite"])
    click('#connect button[type="submit"]')
    wait("return location.pathname==='/send.html' && !!document.querySelector('#photo');")
    type_into("#photo", str(photo))
    wait("return document.querySelector('#photo-form button[type=submit]').disabled===false;")
    click('#photo-form button[type="submit"]')
    wait("return [...document.querySelectorAll('[role=status]')].some(element=>element.textContent.includes('received'));")
    assert asynchronous("return (await fetch('/api/beta/owner')).status;") == 401
    print("Native QA: invited sender connection/upload and owner-role isolation passed", flush=True)
    screenshot = command(f"/session/{session}/screenshot")
    Path("/tmp/memory-lane-tauri-draft.png").write_bytes(base64.b64decode(screenshot))
    command(f"/session/{session}", method="DELETE")
    session = new_session()
    wait("return !!document.querySelector('#demo') && !!window.memoryLaneLocalServer;")
    assert asynchronous("return (await fetch('/api/beta/owner')).status;") == 401
    assert asynchronous("return (await fetch('/api/beta/sender')).status;") == 401
    print("Native QA: app restart clears preview access. All checks passed.", flush=True)
finally:
    if session:
        command(f"/session/{session}", method="DELETE")
