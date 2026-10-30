"""Export a strict public schema, never a raw run directory."""
from __future__ import annotations

import shutil
from collections import defaultdict
from pathlib import Path

from .common import ROOT, digest, read_json, write_json
from .evaluate import summarize


def public_runs(paths, publication=None):
    publication = publication or read_json(ROOT / "configs/publication.json")
    result = []
    for path in paths:
        run = read_json(path)
        groups = defaultdict(list)
        for row in run["rows"]:
            # Private or unconfirmed content never enters the public artifact.
            if row["privacy"] != "public" or row["reference_status"] != "dataset_reference":
                continue
            dataset = row["dataset"] + (":" + row["language"] if row["dataset"] == "fleurs" else "")
            groups[(dataset, row["split"])].append(row)
        for (dataset, split), rows in groups.items():
            if dataset not in publication["datasets"] or split not in {"train", "val", "test", "exploratory"}:
                raise ValueError("Dataset or split is not approved for publication")
            stats = summarize(rows)
            config = run["configuration"]
            # Field allowlist, not redaction of arbitrary paths or provider config.
            model = config.get("model", config["kind"])
            if model not in publication["models"]:
                raise ValueError(f"Model is not approved for publication: {model}")
            safe = dict(publication["models"][model].get("configuration", {}))
            for key in ["weights_sha256", "projector_sha256", "binary_sha256"]:
                observed = run.get("provenance", {}).get(key)
                if observed:
                    if key in safe and safe[key] != observed:
                        raise ValueError("Observed model identity differs from its public registration")
                    safe[key] = observed
            for key in ["temperature", "max_tokens"]:
                value = config.get(key)
                if value is not None:
                    if not isinstance(value, (int, float)) or isinstance(value, bool):
                        raise ValueError("Invalid numeric configuration")
                    safe[key] = value
            approved_prompts = {p.read_text(): p.stem for p in (ROOT / "prompts").glob("*.md")}
            label = publication["models"][model]["label"]
            if run["task"] == "chain":
                upstream_model = run.get("upstream_model")
                if upstream_model not in publication["models"]:
                    raise ValueError("Chain upstream model is not approved for publication")
                label = publication["models"][upstream_model]["label"] + " + " + label
                safe["upstream_model"] = upstream_model
                safe["upstream_identity_status"] = run.get("upstream_identity_status", "unverified")
            if run["prompt"] in approved_prompts:
                label += " / " + approved_prompts[run["prompt"]]
            safe.update(identity_status=run.get("provenance", {}).get("identity_status", "unverified"),
                        dataset_revisions=sorted({r.get("source_revision") for r in rows if r.get("source_revision")}),
                        prompt=(run["prompt"] if run["prompt"] in approved_prompts else "Prompt retained locally; not approved for publication.") if run["prompt"] else "Not applicable", normalization=run["normalization"],
                        sample_manifest_sha256=run["sample_manifest_sha256"], flags=stats["flags"], unknown_flags=stats["unknown_flags"],
                        failed_samples=stats["failed_samples"], scored_samples=stats["scored_samples"])
            limitations = ["Small fixed sample; not a general model ranking.",
                          "No human semantic acceptance is implied by WER.",
                          "Unconfirmed private recordings are excluded."]
            if dataset == "nyra":
                limitations.append("Dataset license unknown; no source audio or text is published. Annotation adaptation is versioned.")
            if run["task"] == "asr":
                limitations.append("Fresh process includes startup and audio preparation; no clipboard insertion.")
            elif run["task"] == "chain":
                limitations.append("Latency sums retained ASR and loaded cleanup stages; excludes capture and insertion.")
            else:
                limitations.append("Loaded-model reply latency; excludes model startup and upstream ASR.")
            result.append({"id": digest([run["id"], dataset, split])[:20], "label": label, "task": run["task"],
                           "dataset": dataset, "split": split, "model": model,
                           "prompt_id": run["prompt_id"] if run["prompt"] else None, "samples": stats["samples"],
                           "wer": stats["wer"], "cer": stats["cer"], "exact_match_rate": stats["exact_match_rate"],
                           "latency_median_s": stats["latency_median_s"], "reference_status": "dataset_reference",
                           "limitations": limitations, "configuration": safe, "examples": []})
    payload = {"schema_version": 1, "runs": result}
    history_path = ROOT / "results/optimization.json"
    if history_path.exists():
        payload["optimization"] = [{"id": e["id"], "initial_wer": e["history"][0]["validation_wer"],
            "best_wer": min(h["validation_wer"] for h in e["history"]), "candidates": len(e["history"]) - 1,
            "accepted": sum(h["accepted"] for h in e["history"][1:])}
            for e in read_json(history_path)["experiments"]]
    return validate_public(payload)


def build(paths, destination, approved_results=None):
    if Path(destination).is_symlink():
        raise ValueError("Publication destination must not be a symlink")
    destination = Path(destination).resolve()
    if destination == ROOT.resolve() or ROOT.resolve().is_relative_to(destination):
        raise ValueError("Destination must not contain the source benchmark directory")
    allowed = {"index.html", "style.css", "app.js", "results.json", ".nojekyll"}
    if destination.exists() and any(child.is_symlink() or child.name not in allowed or not child.is_file() for child in destination.iterdir()):
        raise ValueError("Publication destination contains unapproved files; use a fresh directory")
    destination.mkdir(parents=True, exist_ok=True)
    # Copy individual audited assets; never recursively copy site or data.
    for name in ["index.html", "style.css", "app.js"]:
        shutil.copyfile(ROOT / "site" / name, destination / name)
    if approved_results is not None and paths:
        raise ValueError("Select raw runs or reviewed aggregate results, not both")
    payload = read_json(approved_results) if approved_results is not None else public_runs(paths)
    write_json(destination / "results.json", validate_public(payload))
    (destination / "results.json").chmod(0o644)
    (destination / ".nojekyll").touch()
    return destination


def validate_public(data):
    import math
    if data.get("schema_version") != 1 or not isinstance(data.get("runs"), list):
        raise ValueError("Unsupported public results schema")
    if set(data) - {"schema_version", "runs", "optimization"}:
        raise ValueError("Unexpected public result fields")
    for entry in data.get("optimization", []):
        if set(entry) != {"id", "initial_wer", "best_wer", "candidates", "accepted"}:
            raise ValueError("Invalid optimization summary")
        for key in ["initial_wer", "best_wer", "candidates", "accepted"]:
            if not isinstance(entry[key], (int, float)) or not math.isfinite(entry[key]) or entry[key] < 0:
                raise ValueError("Invalid optimization metric")
    ids = set()
    required = {"id", "label", "task", "dataset", "split", "model", "prompt_id", "samples", "wer", "cer",
                "exact_match_rate", "latency_median_s", "reference_status", "limitations", "configuration", "examples"}
    registry = read_json(ROOT / "configs/publication.json")
    for run in data["runs"]:
        if set(run) != required or run["id"] in ids:
            raise ValueError("Invalid or duplicate public run")
        ids.add(run["id"])
        if run["model"] not in registry["models"] or run["dataset"] not in registry["datasets"] or run["examples"] != []:
            raise ValueError("Unapproved public content")
        if run["split"] not in {"train", "val", "test", "exploratory"}:
            raise ValueError("Invalid public split")
        allowed_config = set(registry["models"][run["model"]].get("configuration", {})) | {"temperature", "max_tokens", "weights_sha256", "projector_sha256", "binary_sha256", "upstream_model", "upstream_identity_status", "identity_status", "dataset_revisions", "prompt", "normalization", "sample_manifest_sha256", "flags", "unknown_flags", "failed_samples", "scored_samples"}
        if not isinstance(run["configuration"], dict) or set(run["configuration"]) - allowed_config:
            raise ValueError("Unapproved public configuration fields")
        if run["reference_status"] != "dataset_reference" or run["task"] not in {"asr", "cleanup", "chain"}:
            raise ValueError("Invalid public reference or task")
        if not isinstance(run["samples"], int) or run["samples"] < 1:
            raise ValueError("Invalid sample count")
        for key in ["wer", "cer", "exact_match_rate", "latency_median_s"]:
            value = run[key]
            if value is not None and (not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) or value < 0):
                raise ValueError("Invalid public metric")
        if run["exact_match_rate"] is not None and run["exact_match_rate"] > 1:
            raise ValueError("Invalid exact-match rate")
    return data
