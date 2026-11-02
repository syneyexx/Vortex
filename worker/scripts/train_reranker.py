"""Reranker fine-tunen op eigen feedback (W11.06).

Invoer: `data/layers/reranker/pairs.jsonl` (vraag, passage, label 0/1) — maak hem met
`POST /api/neural/reranker/dataset` (of de knop op het dashboard). Model: Qwen3-Reranker-0.6B
(zoals in config/models.toml). Methode: LoRA op het causale LM; de score is P("yes") na de
standaard Qwen3-Reranker-prompt. Daarna: samenvoegen + omzetten naar GGUF met llama.cpp
(`convert_hf_to_gguf.py`) en in models.toml als nieuwe reranker-versie zetten; de eval
(retrieval-eval) beslist of hij blijft.

Vereist de extra `train` (PyTorch, transformers, peft) — draait op de PC (5060 Ti), niet in de
cloud-tests.

Gebruik:
    uv run --extra train python scripts/train_reranker.py --pairs data/layers/reranker/pairs.jsonl \
        --model Qwen/Qwen3-Reranker-0.6B --out data/layers/reranker/lora --epochs 2
"""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path

PREFIX = (
    "<|im_start|>system\nJudge whether the Document meets the requirements based on the Query and the Instruct "
    'provided. Note that the answer can only be "yes" or "no".<|im_end|>\n<|im_start|>user\n'
)
SUFFIX = "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
INSTRUCT = "Given a user question, judge whether the memory passage helps answer it"


def fmt(q: str, d: str) -> str:
    return f"{PREFIX}<Instruct>: {INSTRUCT}\n<Query>: {q}\n<Document>: {d}{SUFFIX}"


def main() -> None:
    import torch  # type: ignore
    from peft import LoraConfig, get_peft_model  # type: ignore
    from transformers import AutoModelForCausalLM, AutoTokenizer  # type: ignore

    p = argparse.ArgumentParser()
    p.add_argument("--pairs", required=True)
    p.add_argument("--model", default="Qwen/Qwen3-Reranker-0.6B")
    p.add_argument("--out", required=True)
    p.add_argument("--epochs", type=int, default=2)
    p.add_argument("--lr", type=float, default=1e-4)
    p.add_argument("--batch", type=int, default=4)
    a = p.parse_args()

    rows = [json.loads(line) for line in Path(a.pairs).read_text(encoding="utf-8").splitlines() if line.strip()]
    random.Random(7).shuffle(rows)
    cut = max(1, len(rows) // 6)
    test, train = rows[:cut], rows[cut:]
    dev = "cuda" if torch.cuda.is_available() else "cpu"
    tok = AutoTokenizer.from_pretrained(a.model, padding_side="left")
    model = AutoModelForCausalLM.from_pretrained(
        a.model, torch_dtype=torch.bfloat16 if dev == "cuda" else torch.float32
    ).to(dev)
    model = get_peft_model(
        model,
        LoraConfig(r=16, lora_alpha=32, lora_dropout=0.05, target_modules=["q_proj", "k_proj", "v_proj", "o_proj"]),
    )
    yes, no = tok.convert_tokens_to_ids("yes"), tok.convert_tokens_to_ids("no")
    opt = torch.optim.AdamW(model.parameters(), lr=a.lr)

    def scores(batch):
        enc = tok(
            [fmt(r["query"], r["passage"]) for r in batch],
            padding=True,
            truncation=True,
            max_length=1024,
            return_tensors="pt",
        ).to(dev)
        logits = model(**enc).logits[:, -1, :]
        return torch.log_softmax(torch.stack([logits[:, no], logits[:, yes]], dim=1), dim=1)

    def evaluate() -> float:
        model.eval()
        ok = 0
        with torch.no_grad():
            for i in range(0, len(test), a.batch):
                b = test[i : i + a.batch]
                pred = scores(b).argmax(dim=1).tolist()
                ok += sum(int(p == int(r["label"] >= 0.5)) for p, r in zip(pred, b, strict=True))
        model.train()
        return ok / max(len(test), 1)

    before = evaluate()
    for _ in range(a.epochs):
        random.Random(1).shuffle(train)
        for i in range(0, len(train), a.batch):
            b = train[i : i + a.batch]
            y = torch.tensor([int(r["label"] >= 0.5) for r in b], device=dev)
            loss = torch.nn.functional.nll_loss(scores(b), y)
            loss.backward()
            opt.step()
            opt.zero_grad()
    after = evaluate()
    Path(a.out).mkdir(parents=True, exist_ok=True)
    model.save_pretrained(a.out)
    (Path(a.out) / "metrics.json").write_text(
        json.dumps({"acc_before": before, "acc_after": after, "n_train": len(train), "n_test": len(test)})
    )
    print(json.dumps({"acc_before": before, "acc_after": after, "out": a.out}))


if __name__ == "__main__":
    main()
