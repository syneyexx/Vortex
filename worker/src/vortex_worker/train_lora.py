"""QLoRA/LoRA-training (W13.02): SFT of DPO op het basismodel (safetensors), daarna adapter →
GGUF (llama.cpp `convert_lora_to_gguf.py`) zodat llama-server hem met `--lora` laadt en via
`/lora-adapters` kan hot-swappen. Optioneel samenvoegen + volledig GGUF.

- Unsloth (snelst, 4-bit) als het geïnstalleerd is, anders transformers + PEFT (+ bitsandbytes
  voor 4-bit), met TRL `SFTTrainer` / `DPOTrainer`.
- Annuleren (GPU vrijgeven als de gebruiker iets doet): een `threading.Event`; de callback
  bewaart dan een checkpoint en stopt. Hervatten met `resume = true`.
- `validate()` controleert dataset en instellingen zonder torch (droogloop; ook in tests).
"""

# VX:W13.02
from __future__ import annotations

import json
import subprocess
import sys
import threading
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class LoraCfg:
    kind: str  # "sft" | "dpo"
    base_model: str
    dataset: str
    out_dir: str
    llama_cpp: str = ""
    resume: bool = False
    method: str = "qlora"
    r: int = 16
    alpha: int = 32
    dropout: float = 0.05
    epochs: int = 2
    lr: float = 2e-4
    max_seq: int = 2048
    batch: int = 2
    grad_accum: int = 8
    merge: bool = False
    extra: dict = field(default_factory=dict)

    @staticmethod
    def from_dict(d: dict) -> LoraCfg:
        known = {k: d[k] for k in LoraCfg.__dataclass_fields__ if k in d and k != "extra"}
        return LoraCfg(**known, extra={k: v for k, v in d.items() if k not in LoraCfg.__dataclass_fields__})


def validate(cfg: LoraCfg) -> dict:
    """Controleer dataset + instellingen (zonder torch). Geeft statistiek of gooit ValueError."""
    if cfg.kind not in ("sft", "dpo"):
        raise ValueError(f"onbekende soort {cfg.kind}")
    p = Path(cfg.dataset)
    if not p.exists():
        raise ValueError(f"dataset {p} bestaat niet")
    n, chars, bad = 0, 0, 0
    for line in p.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        if cfg.kind == "sft":
            msgs = r.get("messages") or []
            ok = (
                len(msgs) >= 2
                and msgs[-1].get("role") == "assistant"
                and all(m.get("content") for m in msgs if m.get("role") != "system")
            )
            chars += sum(len(m.get("content") or "") for m in msgs)
        else:
            ok = (
                all(isinstance(r.get(k), str) and r.get(k) for k in ("prompt", "chosen", "rejected"))
                and r["chosen"] != r["rejected"]
            )
            chars += len(r.get("prompt", "")) + len(r.get("chosen", "")) + len(r.get("rejected", ""))
        n += 1
        bad += 0 if ok else 1
    if n == 0:
        raise ValueError("lege dataset")
    if bad:
        raise ValueError(f"{bad} van {n} voorbeelden ongeldig voor {cfg.kind}")
    if cfg.r <= 0 or cfg.alpha <= 0 or not (0 < cfg.lr < 0.1) or cfg.epochs < 1:
        raise ValueError("ongeldige LoRA-instellingen")
    return {"n": n, "approx_tokens": chars // 4, "steps": max(1, n * cfg.epochs // max(1, cfg.batch * cfg.grad_accum))}


def _load(cfg: LoraCfg):  # pragma: no cover - vereist torch + GPU
    try:
        from unsloth import FastLanguageModel  # type: ignore

        model, tok = FastLanguageModel.from_pretrained(
            cfg.base_model, max_seq_length=cfg.max_seq, load_in_4bit=cfg.method == "qlora"
        )
        model = FastLanguageModel.get_peft_model(
            model,
            r=cfg.r,
            lora_alpha=cfg.alpha,
            lora_dropout=cfg.dropout,
            target_modules=["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
        )
        return model, tok, "unsloth"
    except ImportError:
        import torch  # type: ignore
        from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training  # type: ignore
        from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig  # type: ignore

        q = None
        if cfg.method == "qlora":
            q = BitsAndBytesConfig(
                load_in_4bit=True,
                bnb_4bit_quant_type="nf4",
                bnb_4bit_compute_dtype=torch.bfloat16,
                bnb_4bit_use_double_quant=True,
            )
        tok = AutoTokenizer.from_pretrained(cfg.base_model)
        model = AutoModelForCausalLM.from_pretrained(
            cfg.base_model, quantization_config=q, device_map="auto", torch_dtype=torch.bfloat16
        )
        if q is not None:
            model = prepare_model_for_kbit_training(model)
        model = get_peft_model(
            model,
            LoraConfig(
                r=cfg.r,
                lora_alpha=cfg.alpha,
                lora_dropout=cfg.dropout,
                task_type="CAUSAL_LM",
                target_modules=["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
            ),
        )
        return model, tok, "peft"


def to_gguf(cfg: LoraCfg, adapter_dir: Path) -> Path:
    """Adapter → GGUF met llama.cpp (convert_lora_to_gguf.py)."""
    out = adapter_dir / "adapter.gguf"
    script = Path(cfg.llama_cpp) / "convert_lora_to_gguf.py"
    if not script.exists():
        raise ValueError(f"{script} niet gevonden (zet llama_cpp in config/training.toml)")
    subprocess.run(
        [sys.executable, str(script), "--base", cfg.base_model, "--outfile", str(out), str(adapter_dir)], check=True
    )
    if cfg.merge:
        merged = adapter_dir / "merged"
        subprocess.run(
            [
                sys.executable,
                "-c",
                "from peft import AutoPeftModelForCausalLM as M; import sys; "
                "m=M.from_pretrained(sys.argv[1]).merge_and_unload(); m.save_pretrained(sys.argv[2])",
                str(adapter_dir),
                str(merged),
            ],
            check=True,
        )
        subprocess.run(
            [
                sys.executable,
                str(Path(cfg.llama_cpp) / "convert_hf_to_gguf.py"),
                str(merged),
                "--outfile",
                str(adapter_dir / "merged.gguf"),
            ],
            check=True,
        )
    return out


def train(
    cfg: LoraCfg, cancel: threading.Event | None = None, progress: Callable[[float, str], None] | None = None
) -> dict:  # pragma: no cover - vereist torch
    stats = validate(cfg)
    from datasets import load_dataset  # type: ignore
    from transformers import TrainerCallback  # type: ignore

    out = Path(cfg.out_dir)
    ckpt = out / "checkpoints"
    model, tok, backend = _load(cfg)
    ds = load_dataset("json", data_files=cfg.dataset, split="train")

    class Watch(TrainerCallback):
        def on_step_end(self, args, state, control, **kw):
            if progress and state.max_steps:
                progress(state.global_step / state.max_steps, f"stap {state.global_step}/{state.max_steps}")
            if cancel is not None and cancel.is_set():
                control.should_save = True
                control.should_training_stop = True
            return control

    common = dict(
        output_dir=str(ckpt),
        num_train_epochs=cfg.epochs,
        learning_rate=cfg.lr,
        per_device_train_batch_size=cfg.batch,
        gradient_accumulation_steps=cfg.grad_accum,
        logging_steps=10,
        save_steps=50,
        save_total_limit=2,
        bf16=True,
        report_to=[],
    )
    if cfg.kind == "sft":
        from trl import SFTConfig, SFTTrainer  # type: ignore

        trainer = SFTTrainer(
            model=model,
            processing_class=tok,
            train_dataset=ds,
            args=SFTConfig(max_length=cfg.max_seq, **common),
            callbacks=[Watch()],
        )
    else:
        from trl import DPOConfig, DPOTrainer  # type: ignore

        trainer = DPOTrainer(
            model=model,
            processing_class=tok,
            train_dataset=ds,
            args=DPOConfig(max_length=cfg.max_seq, beta=0.1, **common),
            callbacks=[Watch()],
        )
    result = trainer.train(resume_from_checkpoint=cfg.resume and ckpt.exists() and any(ckpt.iterdir()))
    if cancel is not None and cancel.is_set():
        return {"paused": True, "checkpoint": str(ckpt), **stats}
    adapter_dir = out / "adapter"
    trainer.model.save_pretrained(str(adapter_dir))
    tok.save_pretrained(str(adapter_dir))
    gguf = to_gguf(cfg, adapter_dir)
    return {
        "gguf": str(gguf),
        "adapter_dir": str(adapter_dir),
        "backend": backend,
        "train_loss": float(result.training_loss),
        **stats,
    }


if __name__ == "__main__":  # handmatig: python -m vortex_worker.train_lora config.json [--validate]
    c = LoraCfg.from_dict(json.loads(Path(sys.argv[1]).read_text()))
    print(json.dumps(validate(c) if "--validate" in sys.argv else train(c)))
