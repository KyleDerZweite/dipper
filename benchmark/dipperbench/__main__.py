from __future__ import annotations

import argparse
from pathlib import Path

from .common import DATA, ROOT, read_json


def main():
    parser = argparse.ArgumentParser(description="Local Dipper ASR and cleanup benchmark")
    commands = parser.add_subparsers(dest="command", required=True)
    download = commands.add_parser("download")
    download.add_argument("datasets", nargs="*", default=["nyra", "sotto", "fleurs"], choices=["nyra", "sotto", "fleurs"])
    prepare = commands.add_parser("prepare")
    prepare.add_argument("--per-split", type=int, default=40)
    journal = commands.add_parser("import-journal")
    journal.add_argument("--database", type=Path, required=True)
    journal.add_argument("--audio-root", type=Path, required=True)
    references = commands.add_parser("attach-references")
    references.add_argument("--samples", type=Path, required=True)
    references.add_argument("--references", type=Path, required=True)
    references.add_argument("--output", type=Path, required=True)
    evaluate = commands.add_parser("eval")
    evaluate.add_argument("--samples", type=Path, default=DATA / "samples.json")
    evaluate.add_argument("--config", type=Path, required=True)
    evaluate.add_argument("--task", choices=["asr", "cleanup", "chain"], required=True)
    evaluate.add_argument("--dataset")
    evaluate.add_argument("--split", default="test")
    evaluate.add_argument("--limit", type=int, default=0)
    evaluate.add_argument("--prompt", type=Path)
    evaluate.add_argument("--upstream", type=Path)
    evaluate.add_argument("--output", type=Path, required=True)
    evaluate.add_argument("--label", default="")
    optimize = commands.add_parser("optimize")
    optimize.add_argument("--samples", type=Path, default=DATA / "samples.json")
    optimize.add_argument("--config", type=Path, required=True)
    optimize.add_argument("--seed-prompt", type=Path, default=ROOT / "prompts/cleanup-seed.md")
    optimize.add_argument("--rounds", type=int, default=3)
    optimize.add_argument("--limit", type=int, default=12)
    optimize.add_argument("--output", type=Path, required=True)
    suite = commands.add_parser("run-suite")
    suite.add_argument("--config", type=Path, required=True)
    suite.add_argument("--output", type=Path, required=True)
    publish = commands.add_parser("build-site")
    publish.add_argument("runs", nargs="*", type=Path)
    publish.add_argument("--approved-results", type=Path)
    publish.add_argument("--output", type=Path, default=ROOT / "runs/site")
    args = parser.parse_args()
    if args.command == "download":
        from .datasets import download
        download(args.datasets)
    elif args.command == "prepare":
        from .datasets import prepare
        prepare(["nyra", "sotto", "fleurs"], args.per_split)
    elif args.command == "import-journal":
        from .datasets import import_journal
        import_journal(args.database, args.audio_root)
    elif args.command == "attach-references":
        from .datasets import attach_references
        attach_references(args.samples, args.references, args.output)
    elif args.command == "eval":
        from .evaluate import evaluate
        samples = [s for s in read_json(args.samples)["samples"] if s["split"] == args.split
                   and (args.dataset is None or s["dataset"] == args.dataset)]
        if args.limit:
            samples = samples[:args.limit]
        if args.task == "chain" and args.upstream is None:
            parser.error("chain requires --upstream with retained ASR run.json")
        evaluate(samples, read_json(args.config), args.task, args.prompt.read_text() if args.prompt else "",
                 args.output, args.label, args.upstream)
    elif args.command == "optimize":
        from .optimize import optimize
        if args.rounds < 1 or args.limit < 1:
            parser.error("rounds and limit must be positive")
        optimize(read_json(args.samples)["samples"], args.seed_prompt.read_text(), read_json(args.config),
                 args.output, args.rounds, args.limit)
    elif args.command == "run-suite":
        from .suite import run_suite
        run_suite(args.config, args.output)
    else:
        from .publish import build
        build(args.runs, args.output, args.approved_results)


if __name__ == "__main__":
    main()
