#!/usr/bin/env python3
"""Configure an offline appliance rootfs for a quiet TV boot; no extra services."""
import re
import sys
from pathlib import Path

BEGIN = '# BEGIN Memory Lane quiet boot'
END = '# END Memory Lane quiet boot'


def configure(root: Path) -> None:
    boot = root / 'boot/firmware'
    if not (boot / 'cmdline.txt').is_file():
        boot = root / 'boot'
    cmdline = boot / 'cmdline.txt'
    config = boot / 'config.txt'
    original = cmdline.read_text()
    args = original.split()
    if not any(arg.startswith('root=') for arg in args):
        raise ValueError('Refusing to edit a boot command line without a root filesystem')
    # Preserve root/partition/device options and serial diagnostics. tty3 remains
    # available for attended maintenance; tty1 belongs exclusively to the frame.
    owned = ('loglevel=', 'logo.nologo', 'vt.global_cursor_default=',
             'systemd.show_status=', 'rd.systemd.show_status=', 'consoleblank=')
    args = [arg for arg in args if arg not in ('quiet', 'splash', 'ignore_loglevel', 'console=tty1', 'console=tty3')
            and not arg.startswith(owned)]
    args += ['console=tty3', 'quiet', 'loglevel=3', 'logo.nologo',
             'vt.global_cursor_default=0', 'systemd.show_status=false',
             'rd.systemd.show_status=false', 'consoleblank=0']
    backup = cmdline.with_name('cmdline.txt.memory-lane-backup')
    if not backup.exists():
        backup.write_text(original)
    cmdline.write_text(' '.join(args) + '\n')
    text = config.read_text()
    text = re.sub(re.escape(BEGIN) + r'.*?' + re.escape(END) + r'\n?', '', text, flags=re.S)
    config.write_text(text.rstrip() + f'\n\n{BEGIN}\n[all]\ndisable_splash=1\n{END}\n')
    unit = root / 'etc/systemd/system/getty@tty1.service'
    unit.parent.mkdir(parents=True, exist_ok=True)
    if unit.is_symlink():
        if str(unit.readlink()) != '/dev/null':
            raise ValueError('Unexpected tty1 service override')
    elif unit.exists():
        raise ValueError('Refusing to replace an existing tty1 service override')
    else:
        unit.symlink_to('/dev/null')


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: configure-boot.py <offline-rootfs>')
    configure(Path(sys.argv[1]).resolve())
