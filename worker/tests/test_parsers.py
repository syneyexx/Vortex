"""W10.02/W10.01: parsers met exacte locators + ingest via gRPC (incl. annuleren)."""

import json
import zipfile
from pathlib import Path

import grpc
import pytest

from vortex_worker.gen.vortex.common.v1 import common_pb2
from vortex_worker.gen.vortex.worker.v1 import worker_pb2, worker_pb2_grpc
from vortex_worker.ingest import ingest
from vortex_worker.parsers import parse
from vortex_worker.server import build_server


def make_epub(p: Path) -> Path:
    with zipfile.ZipFile(p, "w") as z:
        z.writestr("mimetype", "application/epub+zip")
        z.writestr(
            "META-INF/container.xml",
            '<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">'
            '<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
        )
        z.writestr(
            "OEBPS/content.opf",
            '<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0">'
            '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Het Vortex Boek</dc:title><dc:creator>Syneye</dc:creator></metadata>'
            '<manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/></manifest>'
            '<spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>',
        )
        z.writestr(
            "OEBPS/ch1.xhtml",
            "<html><head><title>Begin</title></head><body><h1>Begin</h1><p>De kern is in Rust geschreven.</p></body></html>",
        )
        z.writestr(
            "OEBPS/text/ch2.xhtml",
            "<html><body><h1>Geheugen</h1><p>Intro.</p><h2>Vectoren</h2><p>Vortex gebruikt int8-vectoren voor snel zoeken.</p></body></html>",
        )
    return p


def make_docx(p: Path) -> Path:
    w = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
    body = (
        f"<w:document {w}><w:body>"
        '<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Plan</w:t></w:r></w:p>'
        "<w:p><w:r><w:t>Eerst de kern bouwen.</w:t></w:r></w:p>"
        '<w:p><w:pPr><w:pStyle w:val="Kop2"/></w:pPr><w:r><w:t>Risico</w:t></w:r></w:p>'
        "<w:p><w:r><w:t>Te weinig VRAM.</w:t></w:r></w:p>"
        "</w:body></w:document>"
    )
    with zipfile.ZipFile(p, "w") as z:
        z.writestr("word/document.xml", body)
    return p


def test_epub_chapters_with_locators(tmp_path):
    p = parse(make_epub(tmp_path / "boek.epub"))
    assert p.metadata["title"] == "Het Vortex Boek" and p.metadata["author"] == "Syneye"
    vec = next(s for s in p.sections if "int8" in s.text)
    assert vec.title == "Vectoren" and vec.locator == "hoofdstuk 2 > Vectoren"
    assert any(s.locator.startswith("hoofdstuk 1") for s in p.sections)
    assert len(p.content_hash) == 64


def test_docx_headings(tmp_path):
    p = parse(make_docx(tmp_path / "plan.docx"))
    titles = [s.title for s in p.sections]
    assert titles == ["Plan", "Risico"]
    assert "VRAM" in p.sections[1].text and p.sections[1].locator.startswith("§ Risico")


def test_pdf_pages_and_headings(tmp_path):
    fitz = pytest.importorskip("fitz")
    doc = fitz.open()
    pg = doc.new_page()
    pg.insert_text((72, 72), "Inleiding", fontsize=22)
    pg.insert_text((72, 110), "Dit is gewone tekst op pagina een.", fontsize=11)
    pg2 = doc.new_page()
    pg2.insert_text((72, 72), "Resultaten", fontsize=22)
    pg2.insert_text((72, 110), "De score steeg naar 87 procent.", fontsize=11)
    path = tmp_path / "r.pdf"
    doc.save(str(path))
    p = parse(path)
    res = next(s for s in p.sections if s.title == "Resultaten")
    assert "87 procent" in res.text and res.locator == "p. 2"
    assert p.metadata["pages"] == "2"


def test_markdown_html_code(tmp_path):
    md = tmp_path / "n.md"
    md.write_text("# Titel\nintro\n## Sub\ntekst\n```\n# geen kop\n```\n")
    p = parse(md)
    assert [s.title for s in p.sections] == ["Titel", "Sub"]
    assert p.sections[1].locator == "regel 3 > Sub" and "# geen kop" in p.sections[1].text
    h = tmp_path / "p.html"
    h.write_text(
        "<html><head><title>T</title><script>x=1</script></head><body><nav>menu</nav><h2>Kop</h2><p>Inhoud &amp; meer</p></body></html>"
    )
    p = parse(h)
    assert p.sections[0].title == "Kop" and p.sections[0].text == "Inhoud & meer"
    c = tmp_path / "m.py"
    c.write_text("import os\n\n\ndef a():\n    return 1\n\n\nclass B:\n    pass\n")
    p = parse(c)
    assert [(s.title, s.locator) for s in p.sections] == [
        ("kop", "regels 1-3"),
        ("a", "regels 4-5"),
        ("B", "regels 8-9"),
    ]
    r = tmp_path / "x.rs"
    r.write_text("use std::io;\n\npub fn main() {\n}\n\nstruct S;\n")
    p = parse(r)
    assert [s.title for s in p.sections] == ["kop", "main", "S"]


def test_ingest_incremental_and_jsonl(tmp_path):
    src = tmp_path / "docs"
    src.mkdir()
    make_epub(src / "boek.epub")
    (src / "notes.md").write_text("# A\nx\n")
    (src / "plaatje.png").write_bytes(b"\x89PNG")
    out = tmp_path / "out"
    ups = list(ingest([str(src)], out))
    batches = [u for u in ups if u.kind == "batch"]
    assert len(batches) == 2 and ups[-1].kind == "done"
    first = Path(batches[0].path).read_text().splitlines()
    head = json.loads(first[0])
    assert head["type"] == "doc" and len(head["content_hash"]) == 64
    assert all(json.loads(line)["type"] == "section" for line in first[1:])
    known = {b.content_hash for b in batches}
    again = list(ingest([str(src)], out, known=lambda h: h in known))
    assert not [u for u in again if u.kind == "batch"], "ongewijzigd = overslaan"


def test_grpc_parse_and_ingest_stream(tmp_path):
    server, port = build_server("127.0.0.1:0", out_dir=tmp_path / "cold")
    server.start()
    try:
        with grpc.insecure_channel(f"127.0.0.1:{port}") as ch:
            stub = worker_pb2_grpc.WorkerServiceStub(ch)
            book = make_epub(tmp_path / "b.epub")
            r = stub.Parse(worker_pb2.ParseRequest(path=str(book)), timeout=10)
            assert any(s.locator == "hoofdstuk 2 > Vectoren" for s in r.sections)
            with pytest.raises(grpc.RpcError) as e:
                stub.Parse(worker_pb2.ParseRequest(path=str(tmp_path / "weg.pdf")), timeout=10)
            assert e.value.code() == grpc.StatusCode.NOT_FOUND
            ups = list(
                stub.Ingest(
                    worker_pb2.IngestRequest(paths=[str(book)], trace=common_pb2.TraceContext(trace_id="trc_1")),
                    timeout=20,
                )
            )
            kinds = [u.WhichOneof("update") for u in ups]
            assert "chunk_batch" in kinds and kinds[-1] == "done"
            ref = next(u.chunk_batch for u in ups if u.WhichOneof("update") == "chunk_batch")
            assert Path(ref.path).exists() and ref.path.startswith(str(tmp_path / "cold"))
    finally:
        server.stop(0)
