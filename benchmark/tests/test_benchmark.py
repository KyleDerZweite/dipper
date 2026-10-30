import json
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

from dipperbench.common import read_json, write_json
from dipperbench.datasets import nyra_text, split_for
from dipperbench.evaluate import evaluate
from dipperbench.metrics import score
from dipperbench.optimize import select_pools
from dipperbench.publish import build, public_runs


class BenchmarkTest(unittest.TestCase):
    def sample(self, **changes):
        sample = {"id": "one", "dataset": "nyra", "language": "de", "split": "test", "privacy": "public", "license": "unknown",
                  "reference_status": "dataset_reference", "input_text": "äh hallo", "literal_reference": "äh hallo",
                  "intended_reference": "hallo", "group_id": "a" * 64}
        return {**sample, **changes}

    def test_scoring_empty_reference_and_insertions(self):
        self.assertIsNone(score("", "invented")["wer"])
        self.assertEqual(score("one", "one two three")["wer"], 2)
        self.assertTrue(score("Äpfel!", "äpfel")["exact"])

    def test_annotation_adapter_and_exposed_rows(self):
        self.assertEqual(nyra_text("[UM] ich w* will [cough] gehen"), "ähm ich will gehen")
        s = self.sample(source_split="test", source_index=29)
        self.assertEqual(split_for(s, "f" * 64), "exploratory")

    def test_exposed_content_reserved_even_when_quota_is_full(self):
        import pyarrow as pa
        import pyarrow.parquet as pq
        import dipperbench.datasets as datasets
        from dipperbench.common import digest
        from dipperbench.metrics import normalize
        from unittest.mock import patch
        secret = next(f"previously inspected {i}" for i in range(100) if int(digest(normalize(f"previously inspected {i}"))[:8], 16) % 10 >= 8)
        rows = [{"input": f"seen row {i}", "output": f"seen row {i}"} for i in range(29)]
        rows += [{"input": secret, "output": secret}, {"input": secret, "output": secret}]
        with tempfile.TemporaryDirectory() as tmp:
            data = Path(tmp)
            source = data / "sources/sotto/data/validation-00000.parquet"
            source.parent.mkdir(parents=True)
            pq.write_table(pa.Table.from_pylist(rows), source)
            with patch.object(datasets, "DATA", data):
                samples = datasets.prepare(["sotto"], per_split=1)
            self.assertFalse(any(sample["split"] == "test" and sample["input_text"] == secret for sample in samples))

    def test_chain_requires_asr_and_matching_sample_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)
            write_json(path / "wrong.json", {"task": "cleanup", "rows": []})
            with self.assertRaises(ValueError):
                evaluate([self.sample()], {"kind": "identity"}, "chain", "", path / "wrong", upstream=path / "wrong.json")
            write_json(path / "stale.json", {"task": "asr", "rows": [{"sample_id": "one", "sample_sha256": "stale"}]})
            with self.assertRaises(ValueError):
                evaluate([self.sample()], {"kind": "identity"}, "chain", "", path / "stale", upstream=path / "stale.json")

    def test_optimizer_excludes_private_unconfirmed_and_test(self):
        a = self.sample(split="train", id="train", group_id="a")
        b = self.sample(split="val", id="val", group_id="b")
        secret = self.sample(split="test", id="secret", input_text="SECRET")
        pools = select_pools([a, b, secret, self.sample(split="train", id="private", privacy="private")])
        self.assertEqual({x["id"] for pool in pools.values() for x in pool}, {"train", "val"})
        with self.assertRaises(ValueError):
            select_pools([a, {**b, "group_id": "a"}])

    def test_real_protocol_scoring_failure_and_publication_boundary(self):
        requests = []
        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                requests.append(body)
                data = json.dumps({"choices": [{"message": {"content": "hallo"}, "finish_reason": "stop"}]}).encode()
                self.send_response(200)
                self.end_headers()
                self.wfile.write(data)
            def log_message(self, *args):
                pass
        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp)
                config = {"kind": "chat", "model": "lfm3-text", "endpoint": f"http://127.0.0.1:{server.server_port}/v1",
                          "weights": "/PRIVATE/PATH", "api_key": "SECRET"}
                run = evaluate([self.sample(), self.sample(id="private", privacy="private", reference_status="unconfirmed_transcript",
                    intended_reference=None, input_text="PRIVATE TRANSCRIPT")], config, "cleanup", "UNAPPROVED PROMPT", path / "run")
                self.assertEqual(run["summary"]["wer"], 0)
                self.assertEqual(run["summary"]["scored_samples"], 1)
                self.assertIsNone(run["rows"][1]["score"])
                self.assertNotIn("reference", requests[0]["messages"][1]["content"])
                exported = public_runs([path / "run/run.json"])
                serialized = json.dumps(exported)
                for secret in ["SECRET", "PRIVATE", "UNAPPROVED PROMPT", "api_key", "weights\""]:
                    self.assertNotIn(secret, serialized)
                self.assertEqual(exported["runs"][0]["examples"], [])
                self.assertEqual(exported["runs"][0]["samples"], 1)
                build([path / "run/run.json"], path / "site")
                (path / "site/private.txt").write_text("SECRET")
                with self.assertRaises(ValueError):
                    build([path / "run/run.json"], path / "site")
                (path / "site/private.txt").unlink()
                self.assertEqual(set(p.name for p in (path / "site").iterdir()), {"index.html", "style.css", "app.js", "results.json", ".nojekyll"})
                failed = evaluate([self.sample()], {"kind": "chat", "model": "bad", "endpoint": "http://127.0.0.1:1/v1", "timeout_s": 1},
                                  "cleanup", "prompt", path / "failed")
                self.assertEqual(failed["summary"]["failed_samples"], 1)
                self.assertEqual(failed["summary"]["wer"], 1)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_no_overwrite_or_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                evaluate([self.sample(id="../escape")], {"kind": "identity"}, "cleanup", "", Path(tmp) / "run")
            with self.assertRaises(FileExistsError):
                evaluate([self.sample()], {"kind": "identity"}, "cleanup", "", tmp)


if __name__ == "__main__":
    unittest.main()
