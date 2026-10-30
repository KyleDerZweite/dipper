"""Use upstream SkillOpt reflection, patch application and validation gating.

This is a bounded external environment, not a fork of the upstream trainer.
The test split is deliberately absent from the adapter's available items.
"""
from __future__ import annotations

import dataclasses
import json
import random
import time
from pathlib import Path

from .common import digest, write_json
from .metrics import flags, score


def select_pools(samples, limit=12):
    pools = {}
    for split in ["train", "val"]:
        eligible = sorted([s for s in samples if s["split"] == split and s.get("intended_reference") is not None
                           and s.get("input_text") is not None and s["privacy"] == "public"], key=lambda s: s["id"])
        grouped = {}
        for sample in eligible:
            grouped.setdefault(sample["dataset"], []).append(sample)
        selected = []
        while grouped and len(selected) < limit:
            for key in list(grouped):
                selected.append(grouped[key].pop(0))
                if not grouped[key]:
                    del grouped[key]
                if len(selected) >= limit:
                    break
        pools[split] = selected
    if not pools["train"] or not pools["val"]:
        raise ValueError("Optimization requires nonempty public train and validation pools")
    if {s["group_id"] for s in pools["train"]} & {s["group_id"] for s in pools["val"]}:
        raise ValueError("Train/validation content overlap")
    return pools


def optimize(samples, seed_prompt, config, output, rounds=3, limit=12):
    from skillopt.envs.base import EnvAdapter
    from skillopt.evaluation.gate import evaluate_gate
    from skillopt.model import chat_target_messages, configure_qwen_chat, set_optimizer_backend, set_target_backend, set_optimizer_deployment, set_target_deployment
    from skillopt.optimizer.skill import apply_patch_with_report

    output = Path(output)
    if output.exists():
        raise FileExistsError(output)
    output.mkdir(parents=True, mode=0o700)
    pools = select_pools(samples, limit)
    write_json(output / "selection.json", {k: [s["id"] for s in v] for k, v in pools.items()})
    write_json(output / "config.json", {**config, "rounds": rounds, "limit": limit, "seed_prompt_sha256": digest(seed_prompt),
               "effective_generation": {"target_temperature": 0.0, "optimizer_temperature": 0.3, "target_max_tokens": 1024, "optimizer_max_tokens": 4096, "thinking": "disabled", "reflection_minibatch": 4, "edit_budget_per_minibatch": 3}, "objective": "strict reduction of validation corpus WER; raw prediction, no semantic-density bonus"})
    configure_qwen_chat(target_base_url=config["target_endpoint"], optimizer_base_url=config["optimizer_endpoint"],
                        target_temperature=0.0, optimizer_temperature=0.3, target_max_tokens=1024,
                        optimizer_max_tokens=4096, target_thinking_mode="disabled", optimizer_thinking_mode="disabled",
                        timeout_seconds=120)
    set_target_backend("qwen_chat")
    set_optimizer_backend("qwen_chat")
    set_target_deployment(config["target_model"])
    set_optimizer_deployment(config["optimizer_model"])

    class CleanupAdapter(EnvAdapter):
        analyst_workers = 1
        failure_only = True
        minibatch_size = 4
        edit_budget = 3

        def build_train_env(self, batch_size, seed, **kwargs):
            return random.Random(seed).sample(pools["train"], min(batch_size, len(pools["train"])))

        def build_eval_env(self, env_num, split, seed, **kwargs):
            if split not in {"val", "selection", "valid_seen"}:
                raise ValueError("Held-out test is unavailable during optimization")
            return pools["val"]

        def get_task_types(self):
            return ["dictation_cleanup"]

        def rollout(self, env_manager, skill_content, out_dir, **kwargs):
            results = []
            for sample in env_manager:
                user = json.dumps({"dictation": sample["input_text"]}, ensure_ascii=False)
                message, usage = chat_target_messages(messages=[{"role": "system", "content": skill_content}, {"role": "user", "content": user}], max_completion_tokens=1024, retries=1, return_message=True)
                prediction = message.content
                finish_reason = message.metadata.get("finish_reason", "unknown")
                metrics = score(sample["intended_reference"], prediction)
                result = {"id": sample["id"], "hard": int(metrics["exact"]), "soft": 1 / (1 + metrics["wer"]),
                          "predicted_answer": prediction, "task_description": sample["input_text"],
                          "reference_text": sample["intended_reference"], "task_type": "dictation_cleanup",
                          "target_system_prompt": skill_content, "target_user_prompt": user, "n_turns": 1,
                          "score": metrics, "usage": usage, "finish_reason": finish_reason, "flags": flags(sample["input_text"], prediction, finish_reason)}
                path = Path(out_dir) / "predictions" / sample["id"]
                write_json(path / "conversation.json", [{"role": "system", "content": skill_content},
                    {"role": "user", "content": user}, {"role": "assistant", "content": prediction}])
                results.append(result)
            write_json(Path(out_dir) / "rollouts.json", results)
            return results

    adapter = CleanupAdapter()
    adapter.setup({"skill_update_mode": "patch"})
    def validation(prompt, folder):
        results = adapter.rollout(pools["val"], prompt, str(folder))
        total_words = sum(r["score"]["reference_words"] for r in results)
        wer = sum(r["score"]["word_errors"] for r in results) / total_words
        return wer, results

    best, best_step = seed_prompt, 0
    best_wer, _ = validation(best, output / "baseline-val")
    history = [{"round": 0, "validation_wer": best_wer, "accepted": True, "prompt_sha256": digest(best)}]
    (output / "best_skill.md").write_text(best)
    for step in range(1, rounds + 1):
        step_dir = output / f"round-{step:02}"
        train = adapter.rollout(pools["train"], best, str(step_dir / "train"))
        # Upstream reflection consumes full trajectories and hidden reference text.
        patches = adapter.reflect(train, best, str(step_dir / "train"), random_seed=42 + step)
        write_json(step_dir / "patches.json", patches)
        candidate = best
        reports = []
        for raw in patches:
            if isinstance(raw, dict):
                candidate, report = apply_patch_with_report(candidate, raw.get("patch", raw))
                reports.extend(report)
        write_json(step_dir / "patch-report.json", reports)
        (step_dir / "candidate.md").write_text(candidate)
        candidate_wer, _ = validation(candidate, step_dir / "validation")
        gate = evaluate_gate(candidate_skill=candidate, cand_hard=1 / (1 + candidate_wer), current_skill=best,
            current_score=1 / (1 + best_wer), best_skill=best, best_score=1 / (1 + best_wer),
            best_step=best_step, global_step=step)
        write_json(step_dir / "gate.json", dataclasses.asdict(gate))
        accepted = gate.action in {"accept", "accept_new_best"}
        if accepted:
            best, best_wer, best_step = candidate, candidate_wer, step
            (output / "best_skill.md").write_text(best)
        history.append({"round": step, "validation_wer": candidate_wer, "best_validation_wer": best_wer,
                        "accepted": accepted, "prompt_sha256": digest(candidate), "patches": len(patches)})
        write_json(output / "history.json", history)
        print("SkillOpt", step, "validation WER", round(candidate_wer, 4), "accepted", accepted, flush=True)
    return history
