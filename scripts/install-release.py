#!/usr/bin/python3
"""Verify and activate a local release; never download or delete user data."""
import argparse
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile
import uuid


def atomic(path, data, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name('.dipper-' + uuid.uuid4().hex)
    try:
        with temporary.open('xb') as output:
            os.chmod(temporary, mode)
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
        sync_directory(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def link(path, target):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name('.dipper-' + uuid.uuid4().hex)
    try:
        temporary.symlink_to(target)
        os.replace(temporary, path)
        sync_directory(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def snapshot(path):
    if path.is_symlink():
        return {'link': os.readlink(path)}
    if not path.exists():
        return None
    if not path.is_file() or path.stat().st_size > 1024 * 1024:
        raise ValueError(f'Refusing to replace non-file or oversized metadata: {path}')
    return {'data': base64.b64encode(path.read_bytes()).decode(),
            'mode': stat.S_IMODE(path.stat().st_mode)}


def restore(path, before):
    if before is None:
        path.unlink(missing_ok=True)
        sync_directory(path.parent)
    elif 'link' in before:
        link(path, before['link'])
    else:
        atomic(path, base64.b64decode(before['data']), before['mode'])


def recover(prefix):
    transaction = prefix / 'install-transaction.json'
    if not transaction.exists():
        return False
    record = json.loads(transaction.read_text())
    for row in reversed(record['before']):
        restore(Path(row['path']), row['value'])
    transaction.unlink()
    sync_directory(prefix)
    return True


def verify(bundle):
    manifest_path = bundle / 'release.json'
    if manifest_path.stat().st_size > 1024 * 1024:
        raise ValueError('Oversized release manifest')
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('format') != 1 or manifest.get('journal_schema') != 1:
        raise ValueError('Unsupported release or Journal compatibility format')
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+_-]{0,160}', manifest['release_id']):
        raise ValueError('Invalid release identity')
    expected = set()
    if not 1 <= len(manifest['files']) <= 256:
        raise ValueError('Invalid manifest file count')
    for row in manifest['files']:
        relative = Path(row['path'])
        if relative.is_absolute() or '..' in relative.parts or relative == Path('.') or row['path'] in expected:
            raise ValueError('Unsafe or duplicate release path')
        expected.add(row['path'])
        path = bundle / relative
        if not path.resolve().is_relative_to(bundle.resolve()):
            raise ValueError('Release file escapes its bundle')
        if 'link' in row:
            if Path(row['link']).name != row['link'] or row['link'] in ('.', '..'):
                raise ValueError('Unsafe library link')
            if not path.is_symlink() or os.readlink(path) != row['link']:
                raise ValueError(f'Library link mismatch: {relative}')
        else:
            if path.is_symlink() or not path.is_file() or path.stat().st_size > 512 * 1024 * 1024:
                raise ValueError(f'Invalid release file: {relative}')
            with path.open('rb') as stream:
                digest = hashlib.file_digest(stream, 'sha256').hexdigest()
            if digest != row['sha256'] or stat.S_IMODE(path.stat().st_mode) != row['mode']:
                raise ValueError(f'Checksum or permissions mismatch: {relative}')
    actual = {str(p.relative_to(bundle)) for p in bundle.rglob('*') if p.is_file() or p.is_symlink()}
    if actual != expected | {'release.json'}:
        raise ValueError('Release has missing or unlisted files')
    for row in manifest['files']:
        if 'link' in row and str(Path(row['path']).parent / row['link']) not in expected:
            raise ValueError('Library link target is unlisted')
    for required in ['dipper', 'dipper-worker', 'dipper-clipboard', 'launch', 'install.py', 'logo.png', 'WHISPER-LICENSE', 'runtime/build/bin/libwhisper.so', 'runtime/build/bin/libwhisper.so.1', 'runtime/build/bin/libggml.so.0', 'runtime/build/bin/libggml-base.so.0', 'runtime/build/bin/libggml-cpu.so.0', 'runtime/build/bin/libggml-vulkan.so.0']:
        if required not in expected:
            raise ValueError(f'Release is incomplete: {required}')
    return manifest


def desktop_string(value):
    return str(value).replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r')


def activate(args):
    prefix = args.prefix
    prefix.mkdir(mode=0o700, parents=True, exist_ok=True)
    recovered = recover(prefix)
    if args.recover:
        return {'recovered': recovered}
    releases = prefix / 'releases'
    releases.mkdir(exist_ok=True)
    current = prefix / 'current'
    previous = prefix / 'previous'
    if current.exists() and not current.is_symlink():
        raise ValueError('Current release must be a managed symlink')
    old = current.resolve() if current.is_symlink() else None
    if old and old.parent != releases:
        raise ValueError('Current release points outside the managed releases')
    if args.rollback:
        if not previous.is_symlink() or previous.resolve().parent != releases:
            raise ValueError('No managed previous release available')
        target = previous.resolve()
        metadata = verify(target)
        if old and verify(old)['journal_schema'] != metadata['journal_schema']:
            raise ValueError('Rollback needs a reviewed Journal compatibility contract')
    else:
        source = args.bundle.resolve()
        metadata = verify(source)
        target = releases / metadata['release_id']
        if target.exists():
            if verify(target) != metadata:
                raise ValueError('Release identity collision; installed files are preserved')
        else:
            stage = Path(tempfile.mkdtemp(prefix='.staging-', dir=releases))
            try:
                shutil.copytree(source, stage, dirs_exist_ok=True, symlinks=True)
                verify(stage)
                for file in stage.rglob('*'):
                    if file.is_file() and not file.is_symlink():
                        with file.open('rb') as stream:
                            os.fsync(stream.fileno())
                for directory in sorted((p for p in stage.rglob('*') if p.is_dir() and not p.is_symlink()), key=lambda p: len(p.parts), reverse=True):
                    sync_directory(directory)
                sync_directory(stage)
                os.rename(stage, target)
                sync_directory(releases)
            finally:
                if stage.exists():
                    shutil.rmtree(stage)
    settings = args.data_dir / 'settings.json'
    entries = [args.applications_dir / name for name in ['io.github.KyleDerZweite.dipper.desktop', 'dipper.desktop']]
    executable = args.bin_dir / 'dipper'
    paths = [settings, *entries, executable, previous, current]
    for path in paths:
        path.parent.mkdir(parents=True, exist_ok=True)
    transaction = {'before': [{'path': str(p), 'value': snapshot(p)} for p in paths]}
    encoded = (json.dumps(transaction, indent=2) + '\n').encode()
    backup = prefix / 'backups' / uuid.uuid4().hex
    backup.mkdir(mode=0o700, parents=True)
    atomic(backup / 'metadata.json', encoded)
    atomic(prefix / 'install-transaction.json', encoded)
    try:
        if settings.exists():
            value = json.loads(settings.read_text())
            if not isinstance(value, dict) or not isinstance(value.get('native_lib_dir'), str):
                raise ValueError('Invalid existing settings; preserved')
            value['native_lib_dir'] = str(current / 'runtime/build/bin')
            atomic(settings, (json.dumps(value, indent=2) + '\n').encode())
        launch = str(current / 'launch')
        quoted = launch.replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$')
        entry = ('[Desktop Entry]\nType=Application\nName=Dipper\nComment=Local voice dictation\n'
                 f'Exec="{quoted}" --show\nIcon={desktop_string(current / "logo.png")}\n'
                 'Terminal=false\nCategories=Utility;Audio;\nStartupWMClass=dipper\n')
        atomic(entries[0], entry.encode(), 0o644)
        atomic(entries[1], (entry + 'NoDisplay=true\n').encode(), 0o644)
        link(executable, current / 'launch')
        if old and old != target:
            link(previous, old)
        link(current, target)
        (prefix / 'install-transaction.json').unlink()
        sync_directory(prefix)
    except Exception:
        recover(prefix)
        raise
    return {'release_id': metadata['release_id'], 'source_revision': metadata['source_revision'],
            'current': str(target), 'previous': str(previous.resolve()) if previous.is_symlink() else None,
            'settings_backup': str(backup), 'recovered_interruption': recovered}


def main():
    home = Path.home()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('bundle', nargs='?', type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument('--prefix', type=Path, default=home / '.local/opt/dipper')
    parser.add_argument('--data-dir', type=Path, default=Path(os.environ.get('XDG_DATA_HOME', home / '.local/share')) / 'dipper/app')
    parser.add_argument('--applications-dir', type=Path, default=Path(os.environ.get('XDG_DATA_HOME', home / '.local/share')) / 'applications')
    parser.add_argument('--bin-dir', type=Path, default=home / '.local/bin')
    parser.add_argument('--rollback', action='store_true')
    parser.add_argument('--recover', action='store_true')
    args = parser.parse_args()
    for name in ['prefix', 'data_dir', 'applications_dir', 'bin_dir']:
        path = getattr(args, name)
        if not path.is_absolute() or any(c in str(path) for c in '\n\r%'):
            parser.error('Installation paths must be absolute and contain no newline or percent characters')
    runtime = os.environ.get('XDG_RUNTIME_DIR')
    if not runtime or not Path(runtime).is_absolute():
        parser.error('XDG_RUNTIME_DIR is unavailable')
    lock = Path(runtime) / 'dipper-app.lock'
    os.umask(0o077)
    with lock.open('a+b') as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit('Quit Dipper while idle before installing or rolling back.')
        print(json.dumps(activate(args), indent=2))


if __name__ == '__main__':
    main()
