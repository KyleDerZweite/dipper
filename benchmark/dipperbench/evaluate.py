from __future__ import annotations

import json
import os
import re
import statistics
import subprocess
import time
import urllib.request
import urllib.error
from pathlib import Path

from .common import ROOT, RUNS, digest, file_digest, read_json, write_json
from .metrics import VERSION, flags, score


def provenance(config):
    result = {"identity_status": "unverified"}
    if config["kind"] == "identity":
        return {"identity_status": "not_applicable"}
    if config["kind"] == "chat":
        try:
            endpoint = config["endpoint"].rstrip("/").removesuffix("/v1") + "/props"
            with urllib.request.urlopen(endpoint, timeout=5) as response:
                properties = json.load(response)
            result["server_properties"] = properties
            path = Path(properties.get("model_path", ""))
            if path.is_file():
                result["weights_sha256"] = file_digest(path)
        except Exception as exc:
            if isinstance(exc, urllib.error.HTTPError):
                exc.close()
            result["identity_error"] = type(exc).__name__
    else:
        for field in ["weights", "projector", "binary"]:
            if field in config:
                result[field + "_sha256"] = file_digest(Path(os.path.expandvars(config[field])))
    if "weights_sha256" in result:
        if config.get("weights_sha256") and config["weights_sha256"] != result["weights_sha256"]:
            raise ValueError("Model weights do not match the configured digest")
        result["identity_status"] = "verified_local_file"
    if config.get("projector_sha256") and config["projector_sha256"] != result.get("projector_sha256"):
        raise ValueError("Audio projector does not match the configured digest")
    return result


def chat(config, prompt, text):
    payload = {"model": config["model"], "messages": [{"role": "system", "content": prompt},
               {"role": "user", "content": json.dumps({"dictation": text}, ensure_ascii=False)}],
               "temperature": config.get("temperature", 0.1), "seed": 42,
               "max_tokens": config.get("max_tokens", 2048), "stream": False,
               "chat_template_kwargs": {"enable_thinking": False}}
    request = urllib.request.Request(config["endpoint"].rstrip("/") + "/chat/completions",
        data=json.dumps(payload).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=config.get("timeout_s", 120)) as response:
        result = json.load(response)
    choice = result["choices"][0]
    return choice["message"].get("content") or "", {"finish_reason": choice["finish_reason"], "usage": result.get("usage")}


def transcribe(config, sample, work):
    source = Path(sample["audio_path"])
    if not source.is_absolute():
        source = ROOT / source
    if sample.get("audio_sha256") and file_digest(source) != sample["audio_sha256"]:
        raise ValueError("Audio bytes no longer match the frozen sample manifest")
    wav = work / "input.wav"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(source), "-ar", "16000", "-ac", "1", str(wav)],
                   check=True, capture_output=True, timeout=60)
    config = {k: os.path.expandvars(str(v)) if isinstance(v, str) else v for k, v in config.items()}
    if config["kind"] == "whisper":
        output = work / "whisper"
        command = [config["binary"], "-m", config["weights"], "-f", str(wav), "-l", "auto", "-t", str(config.get("threads", 8)),
                   "-bs", "5", "-otxt", "-of", str(output), "-dev", str(config.get("gpu_device", 1))]
    else:
        command = [config["binary"], "-m", config["weights"], "-mm", config["projector"], "--audio", str(wav),
                   "-sys", "Perform ASR.", "-p", "<__media__>", "--temp", "0", "--device", config.get("device", "Vulkan1"), "-ngl", "99",
                   "-t", "8", "-c", "8192", "-n", "2048", "--single-turn", "--no-display-prompt", "--simple-io", "--no-show-timings"]
    proc = subprocess.run(command, capture_output=True, text=True, timeout=config.get("timeout_s", 300))
    (work / "stdout.txt").write_text(proc.stdout)
    (work / "stderr.txt").write_text(proc.stderr)
    if proc.returncode:
        raise RuntimeError(f"ASR exited {proc.returncode}; see local stderr.txt")
    if config["kind"] == "whisper":
        text = output.with_suffix(".txt").read_text().strip()
    else:
        marker = "> <__media__>\n"
        if marker not in proc.stdout or "\n\nExiting..." not in proc.stdout:
            raise RuntimeError("Unrecognized llama-cli output; raw output retained")
        text = proc.stdout.split(marker, 1)[1].split("\n\nExiting...", 1)[0].strip()
    return text, {"finish_reason": "unknown", "command": command}


def summarize(rows):
    scored = [r["score"] for r in rows if r.get("score") is not None]
    words = sum(s["reference_words"] for s in scored)
    chars = sum(s["reference_characters"] for s in scored)
    return {"samples": len(rows), "scored_samples": len(scored), "failed_samples": sum(bool(r.get("error")) for r in rows),
            "wer": sum(s["word_errors"] for s in scored) / words if words else None,
            "cer": sum(s["character_errors"] for s in scored) / chars if chars else None,
            "exact_match_rate": sum(s["exact"] for s in scored) / len(scored) if scored else None,
            "latency_median_s": statistics.median([r["elapsed_s"] for r in rows]) if rows else None,
            "word_errors": sum(s["word_errors"] for s in scored), "reference_words": words,
            "flags": {key: sum(bool(r["flags"][key]) for r in rows) for key in rows[0]["flags"]} if rows else {},
            "unknown_flags": {key: sum(r["flags"][key] is None for r in rows) for key in rows[0]["flags"]} if rows else {}}


def evaluate(samples, config, task, prompt, output, label="", upstream=None):
    if task not in {"asr", "cleanup", "chain"}:
        raise ValueError("Unknown task")
    ids = [sample["id"] for sample in samples]
    if len(ids) != len(set(ids)) or any(not re.fullmatch(r"[A-Za-z0-9._-]+", value) or value in {".", ".."} for value in ids):
        raise ValueError("Sample IDs must be unique safe filenames")
    output = Path(output)
    if output.exists():
        raise FileExistsError(f"Run directory already exists: {output}")
    output.mkdir(parents=True, mode=0o700)
    prior_run = read_json(upstream) if upstream else None
    if task == "chain" and (prior_run is None or prior_run.get("task") != "asr"):
        raise ValueError("Chain upstream must be an ASR run")
    previous = {r["sample_id"]: r for r in prior_run["rows"]} if prior_run else {}
    if prior_run and len(previous) != len(prior_run["rows"]):
        raise ValueError("Duplicate upstream sample IDs")
    if task == "chain" and any(previous.get(s["id"], {}).get("sample_sha256") != digest(s) for s in samples):
        raise ValueError("Upstream ASR sample identity does not match the selected manifest")
    manifest = {"schema_version": 1, "id": output.name, "label": label or output.name, "task": task,
                "configuration": config, "provenance": provenance(config), "prompt": prompt, "prompt_id": digest(prompt)[:16], "normalization": VERSION,
                "sample_ids": [s["id"] for s in samples], "sample_manifest_sha256": digest(samples),
                "upstream_run_sha256": file_digest(upstream) if upstream else None,
                "upstream_model": prior_run["configuration"].get("model") if prior_run else None,
                "upstream_identity_status": prior_run.get("provenance", {}).get("identity_status", "unverified") if prior_run else None, "rows": []}
    for sample in samples:
        if task == "asr" and not sample.get("audio_path"):
            continue
        if task == "cleanup" and sample.get("input_text") is None:
            continue
        started = time.monotonic()
        text, metadata, error = "", {}, None
        source = sample.get("input_text") or ""
        work = output / sample["id"]
        work.mkdir()
        try:
            if task == "asr":
                text, metadata = transcribe(config, sample, work)
            else:
                if task == "chain":
                    prior = previous.get(sample["id"])
                    if not prior or prior.get("error"):
                        raise ValueError("Missing or failed upstream ASR output")
                    source = prior["output"]
                if config["kind"] == "identity":
                    text, metadata = source, {"finish_reason": "stop"}
                else:
                    text, metadata = chat(config, prompt, source)
        except Exception as exc:
            if isinstance(exc, urllib.error.HTTPError):
                exc.close()
            error = f"{type(exc).__name__}: {exc}"
        stage_elapsed = time.monotonic() - started
        elapsed = stage_elapsed + (previous.get(sample["id"], {}).get("elapsed_s", 0) if task == "chain" else 0)
        reference = sample.get("literal_reference" if task == "asr" else "intended_reference")
        row = {"sample_id": sample["id"], "sample_sha256": digest(sample), "dataset": sample["dataset"], "language": sample["language"], "split": sample["split"],
               "privacy": sample["privacy"], "license": sample["license"] if "license" in sample else "private",
               "reference_status": sample["reference_status"], "source_revision": sample.get("source_revision"), "input": source, "reference": reference, "output": text,
               "elapsed_s": elapsed, "stage_elapsed_s": stage_elapsed, "metadata": metadata, "error": error,
               "score": score(reference, text) if reference is not None else None,
               "flags": flags(reference if task == "asr" and reference is not None else source, text, metadata.get("finish_reason"))}
        manifest["rows"].append(row)
        manifest["summary"] = summarize(manifest["rows"])
        write_json(output / "run.json", manifest)
        print(output.name, len(manifest["rows"]), sample["dataset"], "failed" if error else "ok", flush=True)
    if not manifest["rows"]:
        raise ValueError("No eligible samples for this task")
    return manifest
