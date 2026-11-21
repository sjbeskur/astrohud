#!/usr/bin/env python3
"""Temporary passive CEC monitor on the reference Pi. Never transmits frames.

Root is needed for the kernel's monitor modes. Existing CEC logical addresses
and the viewer's file handle are left alone. Emits filtered JSONL, not remote
control keycodes, vendor payloads, OSD names or device credentials.
"""
import argparse
import errno
import fcntl
import json
import os
from pathlib import Path
import struct
import time

# Linux CEC UAPI, x86_64/aarch64: include/uapi/linux/cec.h.
GET_CAPS = 0xc04c6100
SET_MODE = 0x40046109
RECEIVE = 0xc0386106
MESSAGE = struct.Struct('=QQIIII16s7B1x')
NAMES = {0x00:'feature_abort', 0x04:'image_view_on', 0x0d:'text_view_on',
         0x36:'standby', 0x80:'routing_change', 0x81:'routing_information',
         0x82:'active_source', 0x86:'set_stream_path', 0x8f:'give_power_status',
         0x90:'report_power_status', 0x9d:'inactive_source'}
POWER = {0:'on', 1:'standby', 2:'standby_to_on', 3:'on_to_standby'}


def decode(buffer):
    tx_ns, rx_ns, length, _, sequence, _, data, _, rx, tx, *_ = MESSAGE.unpack(buffer)
    if not 2 <= length <= 16 or data[1] not in NAMES:
        return None
    packet = data[:length]
    row = {'event':'cec', 'uptime':(rx_ns or tx_ns)/1e9, 'initiator':packet[0] >> 4,
           'destination':packet[0] & 15, 'opcode':NAMES[packet[1]],
           'bytes':packet.hex(':'), 'rx_status':rx, 'tx_status':tx, 'sequence':sequence}
    if packet[1] == 0x90 and length == 3:
        row['reported_power'] = POWER.get(packet[2], 'unknown')
    return row


def emit(row):
    print(json.dumps(row), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=int, default=180)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 300:
        parser.error('--seconds must be 1–300')
    if os.geteuid() != 0:
        parser.error('run as root to use passive CEC monitor mode')
    model = Path('/proc/device-tree/model').read_text().strip('\0')
    if not model.startswith('Raspberry Pi Zero 2 W'):
        parser.error('this local test is restricted to the reference Pi Zero 2 W')
    assert MESSAGE.size == 56
    with open('/dev/cec0', 'r+b', buffering=0) as adapter:
        caps = bytearray(76)
        fcntl.ioctl(adapter, GET_CAPS, caps, True)
        bits = struct.unpack_from('=I', caps, 68)[0]
        mode = 0xf0 if bits & (1 << 5) else 0xe0  # no initiator, monitor only
        try:
            fcntl.ioctl(adapter, SET_MODE, struct.pack('=I', mode))
        except OSError as error:
            if mode != 0xf0 or error.errno not in (errno.EINVAL, errno.ENOTTY):
                raise
            mode = 0xe0
            fcntl.ioctl(adapter, SET_MODE, struct.pack('=I', mode))
        emit({'event':'ready', 'mode':'monitor_all' if mode == 0xf0 else 'monitor_local_and_broadcast',
              'capabilities':bits, 'uptime':time.monotonic(), 'duration_seconds':args.seconds})
        deadline = time.monotonic()+args.seconds
        heartbeat = 0
        captured = 0
        ignored = 0
        while time.monotonic() < deadline:
            message = bytearray(MESSAGE.size)
            struct.pack_into('=I', message, 20, 250)
            try:
                fcntl.ioctl(adapter, RECEIVE, message, True)
            except OSError as error:
                if error.errno not in (errno.ETIMEDOUT, errno.EAGAIN, errno.EINTR):
                    raise
            else:
                row = decode(message)
                if row is not None:
                    captured += 1
                    emit(row)
                else:
                    ignored += 1
            now = time.monotonic()
            if now >= heartbeat:
                status = Path('/sys/class/drm/card0-HDMI-A-1/status').read_text().strip()
                emit({'event':'heartbeat','uptime':now,'hdmi':status,'captured':captured,'ignored':ignored})
                heartbeat = now+5
        emit({'event':'finished','uptime':time.monotonic(),'captured':captured,'ignored':ignored})


if __name__ == '__main__':
    main()
