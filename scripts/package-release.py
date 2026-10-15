#!/usr/bin/env python3
"""Package an already built Fedora release, excluding private application data."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PIN = '0c7be696371069e0baa2515ed7aa08d5a5473aa6'
LIBS = ['libwhisper.so', 'libwhisper.so.1', 'libggml.so', 'libggml.so.0',
        'libggml-base.so', 'libggml-base.so.0', 'libggml-cpu.so',
        'libggml-cpu.so.0', 'libggml-vulkan.so', 'libggml-vulkan.so.0']


def main():
    os.chdir(ROOT)
    if platform.system() != "Linux" or platform.machine() != "x86_64" or platform.freedesktop_os_release().get("ID") != "fedora" or platform.freedesktop_os_release().get("VERSION_ID") != "43":
        raise SystemExit("This release profile supports Fedora x86_64 only.")
    if subprocess.check_output(['git', 'status', '--porcelain']).strip():
        raise SystemExit('Commit the source before packaging a traceable release.')
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    whisper = Path(os.environ.get('DIPPER_WHISPER_DIR', str(Path.home() / '.local/share/dipper/prototype-tools/whisper.cpp')))
    actual = subprocess.check_output(['git', '-C', str(whisper), 'rev-parse', 'HEAD'], text=True).strip()
    if actual != PIN or subprocess.check_output(['git', '-C', str(whisper), 'status', '--porcelain']).strip():
        raise SystemExit('Whisper source must be clean at the pinned revision.')
    cache = (whisper / 'build/CMakeCache.txt').read_text()
    if f'CMAKE_HOME_DIRECTORY:INTERNAL={whisper.resolve()}' not in cache or 'GGML_VULKAN:BOOL=ON' not in cache:
        raise SystemExit('Whisper build must be configured from this pinned source with Vulkan enabled.')
    # Rebuild every shipped native target and all three Rust binaries together.
    # A desktop-only build must never stamp stale helpers as this release.
    subprocess.run(['cmake', '--build', str(whisper / 'build'), '--target', 'whisper', '--parallel', '2'], check=True)
    environment = {**os.environ, 'DIPPER_WHISPER_DIR': str(whisper)}
    subprocess.run([str(ROOT / 'scripts/build.sh'), '--release'], env=environment, check=True)
    if subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() != revision or subprocess.check_output(['git', 'status', '--porcelain']).strip():
        raise SystemExit('Source changed during the build; commit and package again.')
    release = ROOT / 'target/release'
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    identity = f'{version}-local.{revision[:12]}-fedora43-{platform.machine()}'
    dest = ROOT / 'target/dist' / ('dipper-' + identity)
    if dest.exists():
        raise SystemExit(f'Release already staged: {dest}')
    dest.mkdir(parents=True)
    try:
        expected = f'Dipper {version} ({revision})'
        if subprocess.check_output([str(release / 'dipper'), '--version'], text=True).strip() != expected:
            raise RuntimeError('App binary revision does not match source; rebuild first.')
        for name in ['dipper', 'dipper-worker', 'dipper-clipboard']:
            shutil.copy2(release / name, dest / name)
        libdir = dest / 'runtime/build/bin'
        libdir.mkdir(parents=True)
        for name in LIBS:
            source = whisper / 'build/bin' / name
            real = source.resolve()
            if not (libdir / real.name).exists():
                shutil.copy2(real, libdir / real.name)
            if name != real.name:
                (libdir / name).symlink_to(real.name)
        shutil.copy2(whisper / 'LICENSE', dest / 'WHISPER-LICENSE')
        shutil.copy2(ROOT / 'Logo.png', dest / 'logo.png')
        shutil.copy2(ROOT / 'scripts/install-release.py', dest / 'install.py')
        shutil.copy2(ROOT / 'scripts/launch-release.py', dest / 'launch')
        (dest / 'launch').chmod(0o755)
        (dest / 'install.py').chmod(0o755)
        files = []
        for path in sorted(dest.rglob('*')):
            if path.is_symlink():
                files.append({'path': str(path.relative_to(dest)), 'link': os.readlink(path)})
            elif path.is_file():
                files.append({'path': str(path.relative_to(dest)), 'sha256': hashlib.file_digest(path.open('rb'), 'sha256').hexdigest(), 'mode': path.stat().st_mode & 0o777})
        metadata = {'format': 1, 'release_id': identity, 'version': version,
                    'source_revision': revision, 'whisper_revision': PIN,
                    'platform': 'Fedora 43 x86_64', 'journal_schema': 1,
                    'build_steps': ['cmake --build pinned-whisper/build --target whisper', 'scripts/build.sh --release (complete workspace)'], 'files': files}
        (dest / 'release.json').write_text(json.dumps(metadata, indent=2) + '\n')
        archive = dest.with_name(dest.name + '.tar.gz')
        with tarfile.open(archive, 'w:gz') as bundle:
            bundle.add(dest, arcname=dest.name)
        digest = hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
        archive.with_name(archive.name + '.sha256').write_text(f'{digest}  {archive.name}\n')
        print(archive)
    except BaseException:
        shutil.rmtree(dest)
        raise


if __name__ == '__main__':
    main()
