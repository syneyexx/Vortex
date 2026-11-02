import json

from vortex_worker.hf_datasets import (
    ImportCfg,
    MinHashLSH,
    apply_mapping,
    detect_schema,
    inspect,
    propose_mapping,
    run_import,
)


def _write(p, rows):
    p.write_text("\n".join(json.dumps(r) for r in rows), encoding="utf-8")


def test_schema_and_mapping():
    pref = [{"question": "Wat is 2+2?", "chosen": "4", "rejected": "5"}]
    s = detect_schema(pref)
    assert s["kind"] == "preference"
    m = propose_mapping(s)
    assert m == {"prompt": "question", "chosen": "chosen", "rejected": "rejected"}
    chat = [{"conversations": [{"from": "human", "value": "hoi"}, {"from": "gpt", "value": "hallo"}]}]
    s = detect_schema(chat)
    assert s["kind"] == "chat"
    r = apply_mapping(chat[0], propose_mapping(s), "chat")
    assert r["messages"][1] == {"role": "assistant", "content": "hallo"}


def test_minhash_near_duplicates():
    lsh = MinHashLSH(threshold=0.7)
    a = "de snelle bruine vos springt over de luie hond in het park vandaag en morgen weer"
    assert not lsh.query_add(a)
    assert lsh.query_add(a + " opnieuw")
    assert not lsh.query_add("een totaal andere zin over quantumcomputers en foutcorrectie in supergeleiders")


def test_import_end_to_end_all_destinations(tmp_path):
    rows = [
        {"question": f"Hoeveel is {i} plus {i}? Leg kort uit hoe je rekent.", "answer": str(2 * i)} for i in range(30)
    ]
    rows.append(rows[0])  # exact dubbel
    rows.append({"question": "x", "answer": "y"})  # te kort
    rows.append(
        {
            "question": "Een knuppel en een bal kosten samen €1,10. De knuppel kost €1,00 meer dan de bal. Hoeveel cent kost de bal?",
            "answer": "5",
        }
    )
    src = tmp_path / "qa.jsonl"
    _write(src, rows)
    evals = tmp_path / "evals"
    evals.mkdir()
    _write(
        evals / "s.jsonl",
        [
            {
                "id": "h01",
                "prompt": "Een knuppel en een bal kosten samen €1,10. De knuppel kost €1,00 meer dan de bal. Hoeveel cent kost de bal? Geef alleen het getal als laatste.",
            }
        ],
    )
    info = inspect(str(src), license_="cc-by-4.0")
    assert info["schema"]["kind"] == "qa"
    assert set(info["destinations"]) >= {"knowledge", "sft", "eval"}
    assert "dpo" not in info["destinations"]
    for dest in ("knowledge", "sft", "eval", "layer"):
        out = tmp_path / f"{dest}.jsonl"
        r = run_import(
            ImportCfg.from_dict(
                {
                    "source": str(src),
                    "destination": dest,
                    "out": str(out),
                    "evals_dir": str(evals),
                    "license": "cc-by-4.0",
                    "near_dup": 0.95,
                }
            )
        )
        assert r["exact_dup"] >= 1 and r["low_quality"] == 1 and r["decontaminated"] == 1, (dest, r)
        if dest == "layer":
            assert r["kept"] == 0 and r["wrong_destination"] > 0
            continue
        assert r["kept"] >= 20, (dest, r)
        first = json.loads(out.read_text(encoding="utf-8").splitlines()[0])
        if dest == "sft":
            assert first["messages"][0]["role"] == "user" and first["license"] == "cc-by-4.0"
        if dest == "eval":
            assert first["checks"][0]["type"] == "number"
        if dest == "knowledge":
            assert first["source_uri"].startswith("hf:")
    # Dedup tegen bestaande data: tweede import naar dezelfde set bewaart niets nieuws.
    r2 = run_import(
        ImportCfg(
            source=str(src),
            destination="sft",
            out=str(tmp_path / "sft2.jsonl"),
            existing=[str(tmp_path / "sft.jsonl")],
            evals_dir=str(evals),
        )
    )
    assert r2["kept"] == 0
    # DPO
    p = tmp_path / "pref.jsonl"
    _write(
        p,
        [
            {
                "prompt": f"Vraag nummer {i} over het onderwerp",
                "chosen": f"goed antwoord {i} met uitleg",
                "rejected": f"slecht {i}",
            }
            for i in range(5)
        ],
    )
    r3 = run_import(ImportCfg(source=str(p), destination="dpo", out=str(tmp_path / "dpo.jsonl")))
    assert r3["kept"] == 5 and r3["kind"] == "preference"
