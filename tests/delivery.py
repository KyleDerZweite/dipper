"""Exercise installation transaction failures without user directories or native apps."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('install_release', ROOT / 'scripts/install-release.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class Delivery(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.args = argparse.Namespace(prefix=self.root / 'prefix', data_dir=self.root / 'data',
                                       applications_dir=self.root / 'apps', bin_dir=self.root / 'bin',
                                       recover=False, rollback=False)
        self.args.data_dir.mkdir()
        self.settings = self.args.data_dir / 'settings.json'
        self.settings.write_text(json.dumps({'model_profile': 'large_v3_turbo', 'input_name': None,
                                            'native_lib_dir': '/existing/runtime', 'preferred_hold': 'F2',
                                            'model_path': '/private/kept-model.bin'}))
        self.original = self.settings.read_bytes()
        self.journal = self.args.data_dir / 'journal.sqlite3'
        self.journal.write_bytes(b'untouched-journal')

    def bundle(self, identity):
        root = self.root / identity
        root.mkdir()
        files = []
        names = ['dipper', 'dipper-worker', 'dipper-clipboard', 'launch', 'install.py',
                 'logo.png', 'WHISPER-LICENSE', 'runtime/build/bin/libwhisper.so',
                 'runtime/build/bin/libwhisper.so.1', 'runtime/build/bin/libggml.so.0',
                 'runtime/build/bin/libggml-base.so.0', 'runtime/build/bin/libggml-cpu.so.0',
                 'runtime/build/bin/libggml-vulkan.so.0']
        for name in names:
            p = root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes((identity + name).encode())
            p.chmod(0o755)
            files.append({'path': name, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest(), 'mode': 0o755})
        (root / 'release.json').write_text(json.dumps({'format': 1, 'release_id': identity,
            'journal_schema': 1, 'source_revision': 'test', 'files': files}))
        return root

    def install(self, identity):
        self.args.bundle = self.bundle(identity)
        return installer.activate(self.args)

    def assert_preserved(self):
        value = json.loads(self.settings.read_text())
        self.assertEqual(value['model_path'], '/private/kept-model.bin')
        self.assertEqual(value['preferred_hold'], 'F2')
        self.assertEqual(self.journal.read_bytes(), b'untouched-journal')

    def test_install_upgrade_and_rollback_preserve_user_data(self):
        a = self.install('release-a')
        b = self.install('release-b')
        self.assertEqual(Path(b['previous']), Path(a['current']))
        self.args.rollback = True
        rolled = installer.activate(self.args)
        self.assertEqual(rolled['release_id'], 'release-a')
        self.assert_preserved()
        self.assertEqual(json.loads(self.settings.read_text())['native_lib_dir'], str(self.args.prefix / 'current/runtime/build/bin'))
        self.assertEqual(Path(a['settings_backup']).joinpath('metadata.json').stat().st_mode & 0o777, 0o600)

    def test_corruption_rejected_before_settings_or_link_mutation(self):
        self.args.bundle = self.bundle('corrupt')
        (self.args.bundle / 'dipper-worker').write_bytes(b'corrupt')
        with self.assertRaisesRegex(ValueError, 'Checksum'):
            installer.activate(self.args)
        self.assertEqual(self.settings.read_bytes(), self.original)
        self.assertFalse((self.args.prefix / 'current').exists())

    def test_release_collision_preserves_installed_release(self):
        first = self.install('release-a')
        self.args.bundle = self.root / 'release-a'
        metadata = json.loads((self.args.bundle / 'release.json').read_text())
        metadata['source_revision'] = 'different'
        (self.args.bundle / 'release.json').write_text(json.dumps(metadata))
        with self.assertRaisesRegex(ValueError, 'collision'):
            installer.activate(self.args)
        self.assertEqual((self.args.prefix / 'current').resolve(), Path(first['current']))
        self.assert_preserved()

    def test_handled_failure_restores_settings_and_entries(self):
        self.install('release-a')
        before = self.settings.read_bytes()
        old = (self.args.prefix / 'current').resolve()
        self.args.bundle = self.bundle('release-b')
        real_link = installer.link
        def fail_current(path, target):
            if path == self.args.prefix / 'current' and Path(target).name == 'release-b':
                raise OSError('injected activation failure')
            return real_link(path, target)
        with patch.object(installer, 'link', fail_current):
            with self.assertRaisesRegex(OSError, 'injected'):
                installer.activate(self.args)
        self.assertEqual(self.settings.read_bytes(), before)
        self.assertEqual((self.args.prefix / 'current').resolve(), old)
        self.assertFalse((self.args.prefix / 'install-transaction.json').exists())
        self.assert_preserved()

    def test_interrupted_activation_recovers_before_next_operation(self):
        self.install('release-a')
        before = self.settings.read_bytes()
        old = (self.args.prefix / 'current').resolve()
        self.args.bundle = self.bundle('release-b')
        real_link = installer.link
        def interrupt(path, target):
            result = real_link(path, target)
            if path == self.args.prefix / 'current':
                raise KeyboardInterrupt('simulated process interruption')
            return result
        with patch.object(installer, 'link', interrupt):
            with self.assertRaises(KeyboardInterrupt):
                installer.activate(self.args)
        self.assertTrue((self.args.prefix / 'install-transaction.json').exists())
        self.args.recover = True
        installer.activate(self.args)
        self.assertEqual((self.args.prefix / 'current').resolve(), old)
        self.assertEqual(self.settings.read_bytes(), before)
        self.assert_preserved()

    def test_unlisted_file_and_escaping_library_link_are_rejected(self):
        bundle = self.bundle('unsafe')
        extra = bundle / 'unexpected'
        extra.write_bytes(b'unlisted')
        with self.assertRaisesRegex(ValueError, 'unlisted'):
            installer.verify(bundle)
        extra.unlink()
        library = bundle / 'runtime/build/bin/libwhisper.so'
        library.unlink()
        library.symlink_to(self.settings)
        with self.assertRaisesRegex(ValueError, 'escapes'):
            installer.verify(bundle)

    def test_running_app_lock_refuses_cli_install(self):
        runtime = self.root / 'runtime'
        runtime.mkdir()
        with (runtime / 'dipper-app.lock').open('a+b') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = subprocess.run(['python3', str(ROOT / 'scripts/install-release.py'),
                                     '--prefix', str(self.args.prefix)],
                                    env={**os.environ, 'XDG_RUNTIME_DIR': str(runtime)}, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'Quit Dipper', result.stderr)
        self.assertFalse(self.args.prefix.exists())


if __name__ == '__main__':
    unittest.main()
