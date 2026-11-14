#!/usr/bin/env python3
"""Accept only the public channel URL and Ed25519 verification key."""
import json
from pathlib import Path
import re
import sys
from urllib.parse import urlsplit


def validate(path):
    value = json.loads(Path(path).read_text())
    if not isinstance(value, dict) or set(value) != {"manifest_url", "public_key"}:
        raise ValueError("Updater config must contain only manifest_url and public_key")
    key = value["public_key"]
    if not isinstance(key, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", key):
        raise ValueError("Updater public_key must be a 32-byte hexadecimal Ed25519 key")
    url = value["manifest_url"]
    if not isinstance(url, str) or any(c.isspace() for c in url):
        raise ValueError("Updater manifest_url must be a public HTTPS URL")
    parsed = urlsplit(url)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username is not None
            or parsed.password is not None or parsed.fragment or parsed.query):
        raise ValueError("Updater manifest_url must use HTTPS without credentials, query or fragment")
    return value


if __name__ == "__main__":
    validate(sys.argv[1])
