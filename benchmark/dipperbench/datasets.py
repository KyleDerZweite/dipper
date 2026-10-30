from __future__ import annotations

import json
import re
import sqlite3
from collections import Counter
from pathlib import Path

from .common import DATA, ROOT, digest, file_digest, read_json, write_json
from .metrics import normalize


def download(names):
    from huggingface_hub import snapshot_download
    catalog = read_json(ROOT / "configs/datasets.json")
    for name in names:
        cfg = catalog[name]
        location = DATA / "sources" / name
        snapshot_download(repo_id=cfg["repo"], repo_type="dataset", revision=cfg["revision"],
                          allow_patterns=cfg["patterns"], local_dir=location, max_workers=3)
        files = [{"path": str(p.relative_to(location)), "bytes": p.stat().st_size, "sha256": file_digest(p)}
                 for p in sorted(location.rglob("*")) if p.is_file() and ".cache" not in p.parts and p.name != "capture.json"]
        write_json(location / "capture.json", {**cfg, "files": files})
        print(name, "downloaded", len(files), "files", flush=True)


def nyra_text(text):
    """Map the dataset's sound tags and cutoff fragments to ordinary ASR text."""
    text = re.sub(r"\[UM\]", "ähm", text, flags=re.I)
    text = re.sub(r"\[UH\]", "äh", text, flags=re.I)
    return " ".join(re.sub(r"\[[^]]*\]|\b\w+\*", " ", text).split())


def split_for(sample, group):
    # The first 30 examples were inspected before this protocol existed.
    if sample["dataset"] in {"nyra", "sotto"} and sample["source_split"] in {"test", "validation"} and sample["source_index"] < 30:
        return "exploratory"
    if sample["dataset"] == "fleurs":
        return {"train": "train", "validation": "val", "test": "test"}[sample["source_split"]]
    if sample["dataset"] == "sotto" and sample["source_split"] == "train":
        return "train"
    bucket = int(group[:8], 16) % 10
    return "train" if bucket < 6 else "val" if bucket < 8 else "test"


def prepare(names, per_split=40):
    """Materialize a fixed small suite; keep full source snapshots separately."""
    import pyarrow.parquet as pq
    if per_split < 1:
        raise ValueError("per_split must be positive")
    samples, seen = [], set()
    for name in names:
        cfg = read_json(ROOT / "configs/datasets.json")[name]
        counters = Counter()
        for file in sorted((DATA / "sources" / name).rglob("*.parquet")):
            source_split = file.name.split("-")[0]
            language = file.parent.name if name == "fleurs" else "de" if name == "nyra" else "en"
            index = 0
            for batch in pq.ParquetFile(file).iter_batches(batch_size=64):
                for row in batch.to_pylist():
                    literal = row.get("verbatim_transcript", row.get("raw_transcription", row.get("transcription")))
                    intended = row.get("intended_transcript", row.get("output"))
                    text = nyra_text(literal) if name == "nyra" else row.get("input")
                    identity = str(row.get("id", index))
                    sample = {"id": name + "-" + digest([language, source_split, identity])[:16], "dataset": name,
                              "language": language, "source_split": source_split, "source_index": index,
                              "source_id": identity, "source_revision": cfg["revision"], "license": cfg["license"],
                              "privacy": "public", "literal_reference": nyra_text(literal) if name == "nyra" else literal, "source_literal_reference": literal, "intended_reference": intended,
                              "reference_status": "dataset_reference", "input_text": text,
                              "input_adaptation": "nyra-spoken-fillers-v1" if name == "nyra" else "none"}
                    index += 1
                    # Intended wording groups duplicated content across all source splits.
                    group = digest(normalize(intended or literal or text or ""))
                    sample["group_id"] = group
                    sample["split"] = split_for(sample, group)
                    key = (name, language, sample["split"])
                    if group in seen:
                        continue
                    if sample["split"] == "exploratory":
                        seen.add(group)
                    if counters[key] >= per_split:
                        continue
                    seen.add(group)
                    audio = row.get("audio")
                    if audio:
                        blob = audio.get("bytes")
                        if blob is None:
                            raise ValueError(f"{name}: parquet audio does not contain bytes")
                        path = DATA / "audio" / name / (sample["id"] + Path(audio.get("path") or "clip.wav").suffix)
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_bytes(blob)
                        sample["audio_path"] = str(path.relative_to(ROOT))
                        sample["audio_sha256"] = file_digest(path)
                    samples.append(sample)
                    counters[key] += 1
            # Do not stop scanning early: duplicate detection must precede final counts.
        print(name, dict(counters), flush=True)
    write_json(DATA / "samples.json", {"schema_version": 1, "protocol": "dipper-mini-v1", "samples": samples})
    return samples


def import_journal(database, audio_root):
    DATA.mkdir(parents=True, exist_ok=True, mode=0o700)
    DATA.chmod(0o700)
    database, audio_root = Path(database).resolve(), Path(audio_root).resolve()
    connection = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    rows = connection.execute("""SELECT r.id,r.audio_path,r.started_at,a.raw_text,a.config_json
      FROM recordings r JOIN attempts a ON a.id=(SELECT id FROM attempts WHERE recording_id=r.id
      AND raw_text IS NOT NULL ORDER BY started_at DESC LIMIT 1) WHERE r.state='complete'""").fetchall()
    samples = []
    for row in rows:
        audio = (audio_root / row["audio_path"]).resolve()
        if not audio.is_relative_to(audio_root) or not audio.is_file():
            continue
        sample = {"id": "journal-" + row["id"], "dataset": "journal", "language": "auto", "split": "inspection",
                  "privacy": "private", "reference_status": "unconfirmed_transcript", "literal_reference": None,
                  "intended_reference": None, "input_text": row["raw_text"], "historical_transcript": row["raw_text"],
                  "historical_configuration": json.loads(row["config_json"]), "audio_path": str(audio),
                  "audio_sha256": file_digest(audio), "group_id": digest(normalize(row["raw_text"]))}
        samples.append(sample)
    connection.close()
    write_json(DATA / "journal.json", {"schema_version": 1, "samples": samples})
    print("imported", len(samples), "private recordings as unconfirmed", flush=True)


def attach_references(samples_path, references_path, output):
    """Attach explicit human review without editing the source capture manifest."""
    data, annotations = read_json(samples_path), read_json(references_path)
    by_id = {s["id"]: s for s in data["samples"]}
    for reference in annotations:
        if reference["id"] not in by_id:
            raise ValueError("Reference names an unknown sample")
        if not any(key in reference for key in ["literal_reference", "intended_reference"]):
            raise ValueError("Human review must supply an explicit reference")
        if not reference.get("reviewed_by") or not reference.get("reviewed_at"):
            raise ValueError("Human references require reviewer and date")
        sample = by_id[reference["id"]]
        for key in ["literal_reference", "intended_reference"]:
            if key in reference:
                if not isinstance(reference[key], str) or not reference[key].strip():
                    raise ValueError("A confirmed reference must be nonempty text")
                sample[key] = reference[key]
        sample["reference_status"] = "human_confirmed"
        sample["reviewed_by"], sample["reviewed_at"] = reference["reviewed_by"], reference["reviewed_at"]
    if Path(output).resolve() == Path(samples_path).resolve() or Path(output).exists():
        raise FileExistsError("Write reviewed samples to a new manifest")
    write_json(output, data)
