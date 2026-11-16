#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('capture', Path(__file__).with_name('capture-cec.py'))
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


def frame(payload, rx=1, tx=0):
    return capture.MESSAGE.pack(0,1_000_000_000,len(payload),0,0,0,payload.ljust(16,b'\0'),0,rx,tx,0,0,0,0)


class CaptureTests(unittest.TestCase):
    def test_standby_retains_origin_and_destination(self):
        tv = capture.decode(frame(bytes.fromhex('0f36')))
        self.assertEqual(tv['opcode'],'standby')
        self.assertEqual(tv['initiator'],0)
        self.assertEqual(tv['destination'],15)
        source = capture.decode(frame(bytes.fromhex('4036')))
        self.assertEqual(source['initiator'],4)
        abort = capture.decode(frame(bytes.fromhex('04003600')))
        self.assertEqual(abort['opcode'],'feature_abort')

    def test_power_replies_include_transition_states(self):
        for value,name in capture.POWER.items():
            row = capture.decode(frame(bytes([0x04,0x90,value])))
            self.assertEqual(row['reported_power'],name)
            self.assertEqual(row['uptime'],1)

    def test_keypresses_vendor_data_and_poll_frames_are_not_recorded(self):
        for payload in [bytes.fromhex('044441'),bytes.fromhex('0487616263'),bytes([0x04])]:
            self.assertIsNone(capture.decode(frame(payload)))
        self.assertEqual(capture.MESSAGE.size,56)


if __name__ == '__main__': unittest.main()
