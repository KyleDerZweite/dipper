"""Sequential, configuration-driven evaluation matrices."""
from pathlib import Path

from .common import ROOT, read_json, write_json
from .evaluate import evaluate


def run_suite(config_path, output):
    cfg = read_json(config_path)
    output = Path(output)
    if output.exists():
        raise FileExistsError(output)
    output.mkdir(parents=True, mode=0o700)
    write_json(output / "suite.json", cfg)
    runs = {}
    for entry in cfg["evaluations"]:
        name = entry["id"]
        if name in runs or not name.replace("-", "").replace("_", "").isalnum():
            raise ValueError("Evaluation IDs must be unique simple names")
        path = ROOT / entry.get("samples", "data/samples.json")
        samples = [s for s in read_json(path)["samples"] if s["split"] == entry.get("split", "test")
                   and s["dataset"] == entry["dataset"] and (not entry.get("language") or s["language"] == entry["language"])]
        samples.sort(key=lambda s: s["id"])
        if entry.get("limit"):
            samples = samples[:entry["limit"]]
        prompt_path = entry.get("prompt")
        prompt = (ROOT / prompt_path).read_text() if prompt_path else ""
        upstream = runs.get(entry.get("upstream"))
        if entry["task"] == "chain" and upstream is None:
            raise ValueError("Chain requires an earlier named ASR evaluation")
        evaluate(samples, entry["model"], entry["task"], prompt, output / name, entry.get("label", name), upstream)
        runs[name] = output / name / "run.json"
    return runs
