from __future__ import annotations

import re
import unicodedata

VERSION = "nfkc-unicode-words-v1"


def normalize(text):
    return " ".join(re.findall(r"\w+", unicodedata.normalize("NFKC", text).lower()))


def distance(reference, prediction):
    previous = list(range(len(prediction) + 1))
    for i, expected in enumerate(reference, 1):
        current = [i]
        for j, actual in enumerate(prediction, 1):
            current.append(min(current[-1] + 1, previous[j] + 1, previous[j - 1] + (expected != actual)))
        previous = current
    return previous[-1]


def score(reference, prediction):
    ref, pred = normalize(reference), normalize(prediction)
    ref_words, pred_words = ref.split(), pred.split()
    errors = distance(ref_words, pred_words)
    chars, char_errors = len(ref.replace(" ", "")), distance(ref.replace(" ", ""), pred.replace(" ", ""))
    return {"word_errors": errors, "reference_words": len(ref_words), "character_errors": char_errors,
            "reference_characters": chars, "wer": errors / len(ref_words) if ref_words else None,
            "cer": char_errors / chars if chars else None, "exact": ref == pred,
            "empty_reference": not ref_words}


def flags(source, output, finish_reason="stop"):
    numbers = lambda s: re.findall(r"\b\d+(?:[.,]\d+)?\b", s)
    negation = lambda s: re.findall(r"\b(?:nicht|kein\w*|not|never|no)\b", s.lower())
    return {"numbers_changed": numbers(source) != numbers(output),
            "negation_changed": negation(source) != negation(output),
            "thinking_markup": bool(re.search(r"</?think>", output)),
            "truncated": (finish_reason == "length") if finish_reason in {"stop", "length"} else None, "empty_output": not output.strip()}
