#!/usr/bin/python3
"""Launch one immutable installed release without a development environment."""
import os
from pathlib import Path
import sys

release = Path(__file__).resolve().parent
if release.parent.name == 'releases' and (release.parent.parent / 'install-transaction.json').exists():
    raise SystemExit('Dipper installation was interrupted. Run this release install.py --recover before launching.')
for name in ['libwhisper.so.1', 'libggml.so.0', 'libggml-base.so.0',
             'libggml-cpu.so.0', 'libggml-vulkan.so.0']:
    if not (release / 'runtime/build/bin' / name).is_file():
        raise SystemExit(f'Dipper bundled runtime is incomplete: {name}. Reinstall this release.')
for key in ['LD_LIBRARY_PATH', 'APPDIR', 'APPIMAGE', 'NO_AT_BRIDGE']:
    os.environ.pop(key, None)
os.environ['DIPPER_WHISPER_DIR'] = str(release / 'runtime')
os.execv(release / 'dipper', ['dipper', *sys.argv[1:]])
