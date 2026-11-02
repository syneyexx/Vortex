"""Parsers (W10.02): EPUB, PDF, DOCX, Markdown, HTML, code en platte tekst → secties met een
exacte bronverwijzing (`locator`). Zware libraries (PyMuPDF, trafilatura) zijn optioneel en
worden pas bij gebruik geladen; EPUB en DOCX lezen we zelf (zip + XML)."""

# VX:W10.02
from __future__ import annotations

import ast
import hashlib
import html
import posixpath
import re
import zipfile
from dataclasses import dataclass, field
from html.parser import HTMLParser
from pathlib import Path
from xml.etree import ElementTree as ET


@dataclass
class Section:
    title: str
    level: int
    text: str
    locator: str


@dataclass
class Parsed:
    content_hash: str
    sections: list[Section]
    metadata: dict[str, str] = field(default_factory=dict)


CODE_EXT = {
    ".py": "python",
    ".rs": "rust",
    ".ts": "typescript",
    ".tsx": "typescript",
    ".js": "javascript",
    ".jsx": "javascript",
    ".go": "go",
    ".java": "java",
    ".c": "c",
    ".h": "c",
    ".cpp": "cpp",
    ".hpp": "cpp",
    ".cs": "csharp",
    ".rb": "ruby",
    ".php": "php",
    ".kt": "kotlin",
    ".swift": "swift",
    ".sh": "shell",
    ".ps1": "powershell",
    ".sql": "sql",
    ".lua": "lua",
}
TEXT_EXT = {".txt", ".text", ".log", ".csv", ".tsv", ".json", ".toml", ".yaml", ".yml", ".ini", ".cfg"}
SUPPORTED = {".epub", ".pdf", ".docx", ".md", ".markdown", ".html", ".htm", *CODE_EXT, *TEXT_EXT}


def file_hash(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def detect(path: Path, media_type: str = "") -> str:
    mt = media_type.lower()
    if "epub" in mt:
        return "epub"
    if "pdf" in mt:
        return "pdf"
    if "wordprocessingml" in mt:
        return "docx"
    if "markdown" in mt:
        return "md"
    if "html" in mt:
        return "html"
    ext = path.suffix.lower()
    return {
        ".epub": "epub",
        ".pdf": "pdf",
        ".docx": "docx",
        ".md": "md",
        ".markdown": "md",
        ".html": "html",
        ".htm": "html",
    }.get(ext, "code" if ext in CODE_EXT else "text")


# ---------------- HTML (ook binnen EPUB) ----------------


class _HtmlSections(HTMLParser):
    """Splitst HTML op koppen h1-h6; negeert script/style/nav; behoudt lijsten en tabellen als tekst."""

    SKIP: frozenset[str] = frozenset({"script", "style", "noscript", "nav", "footer", "head", "svg", "template"})

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.sections: list[tuple[str, int, list[str]]] = [("", 0, [])]
        self.skip = 0
        self.in_head: int | None = None
        self.head_buf: list[str] = []
        self.title = ""
        self._in_title = False

    def handle_starttag(self, tag, attrs):
        if tag in self.SKIP:
            self.skip += 1
        if tag == "title":
            self._in_title = True
        if self.skip:
            return
        if re.fullmatch(r"h[1-6]", tag):
            self.in_head = int(tag[1])
            self.head_buf = []
        elif tag in ("p", "div", "br", "tr", "section", "article", "blockquote", "pre"):
            self.sections[-1][2].append("\n")
        elif tag == "li":
            self.sections[-1][2].append("\n- ")
        elif tag in ("td", "th"):
            self.sections[-1][2].append(" | ")

    def handle_endtag(self, tag):
        if tag in self.SKIP and self.skip:
            self.skip -= 1
        if tag == "title":
            self._in_title = False
        if self.in_head is not None and re.fullmatch(r"h[1-6]", tag):
            title = " ".join("".join(self.head_buf).split())
            self.sections.append((title, self.in_head, []))
            self.in_head = None

    def handle_data(self, data):
        if self._in_title:
            self.title += data
        if self.skip:
            return
        if self.in_head is not None:
            self.head_buf.append(data)
        else:
            self.sections[-1][2].append(data)


def _clean(text: str) -> str:
    lines = [" ".join(line.split()) for line in text.splitlines()]
    out: list[str] = []
    for line in lines:
        if line or (out and out[-1]):
            out.append(line)
    return "\n".join(out).strip()


def html_sections(src: str, base_locator: str = "") -> tuple[str, list[Section]]:
    p = _HtmlSections()
    p.feed(src)
    out = []
    for i, (title, level, buf) in enumerate(p.sections):
        text = _clean("".join(buf))
        if not text and not title:
            continue
        loc = f"{base_locator} > {title}" if base_locator and title else (base_locator or title or f"deel {i}")
        out.append(Section(title=title, level=level, text=text, locator=loc.strip()))
    return html.unescape(p.title.strip()), out


def parse_html(path: Path) -> Parsed:
    raw = path.read_text(encoding="utf-8", errors="replace")
    title, secs = html_sections(raw)
    meta = {"title": title, "format": "html"}
    # trafilatura levert betere hoofdtekst als hij er is; koppen-structuur houden we zelf.
    if not any(s.text for s in secs):
        try:
            import trafilatura  # type: ignore

            txt = trafilatura.extract(raw) or ""
            secs = [Section(title=title, level=1, text=txt, locator=title or path.name)]
        except Exception:
            pass
    return Parsed(file_hash(path), secs, meta)


# ---------------- EPUB ----------------


def parse_epub(path: Path) -> Parsed:
    """EPUB = zip met OPF (spine = leesvolgorde) + XHTML-hoofdstukken. Locator: 'hoofdstuk N > kop'."""
    ns = {
        "c": "urn:oasis:names:tc:opendocument:xmlns:container",
        "o": "http://www.idpf.org/2007/opf",
        "dc": "http://purl.org/dc/elements/1.1/",
    }
    with zipfile.ZipFile(path) as z:
        cont = ET.fromstring(z.read("META-INF/container.xml"))
        rootfile = cont.find(".//c:rootfile", ns)
        if rootfile is None:
            raise ValueError("geen OPF in EPUB")
        opf_path = rootfile.get("full-path", "")
        opf = ET.fromstring(z.read(opf_path))
        base = posixpath.dirname(opf_path)
        manifest = {i.get("id"): i.get("href") for i in opf.findall(".//o:manifest/o:item", ns)}
        spine = [r.get("idref") for r in opf.findall(".//o:spine/o:itemref", ns)]
        meta = {"format": "epub"}
        for key in ("title", "creator", "language", "date", "publisher"):
            el = opf.find(f".//dc:{key}", ns)
            if el is not None and el.text:
                meta["author" if key == "creator" else key] = el.text.strip()
        sections: list[Section] = []
        for n, idref in enumerate(spine, start=1):
            href = manifest.get(idref)
            if not href:
                continue
            name = posixpath.normpath(posixpath.join(base, href))
            try:
                xhtml = z.read(name).decode("utf-8", errors="replace")
            except KeyError:
                continue
            ch_title, secs = html_sections(xhtml, base_locator=f"hoofdstuk {n}")
            for s in secs:
                if s.level == 0 and ch_title and not s.title:
                    s.title = ch_title
                sections.append(s)
    return Parsed(file_hash(path), [s for s in sections if s.text], meta)


# ---------------- DOCX ----------------


def parse_docx(path: Path) -> Parsed:
    """DOCX zonder extra library: word/document.xml, koppen via stijl Heading N / Kop N."""
    w = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
    with zipfile.ZipFile(path) as z:
        doc = ET.fromstring(z.read("word/document.xml"))
        meta = {"format": "docx"}
        try:
            core = ET.fromstring(z.read("docProps/core.xml"))
            for el in core:
                tag = el.tag.split("}")[-1]
                if tag in ("title", "creator") and el.text:
                    meta["author" if tag == "creator" else tag] = el.text
        except KeyError:
            pass
    sections = [Section("", 0, "", "begin")]
    for para_no, p in enumerate(doc.iter(f"{w}p"), start=1):
        style = p.find(f"{w}pPr/{w}pStyle")
        sval = (style.get(f"{w}val") if style is not None else "") or ""
        text = "".join(t.text or "" for t in p.iter(f"{w}t"))
        m = re.match(r"(?i)(heading|kop)\s*(\d)", sval)
        if m and text.strip():
            sections.append(Section(text.strip(), int(m.group(2)), "", f"§ {text.strip()} (alinea {para_no})"))
        elif text.strip():
            sections[-1].text += text + "\n"
    return Parsed(file_hash(path), [s for s in sections if s.text.strip()], meta)


# ---------------- PDF ----------------


def parse_pdf(path: Path) -> Parsed:
    """PDF via PyMuPDF: tekst per pagina; koppen herkend aan lettergrootte. Locator 'p. N'."""
    import fitz  # type: ignore  # PyMuPDF (extra 'ingest')

    doc = fitz.open(str(path))
    meta = {"format": "pdf", "pages": str(doc.page_count)}
    for k in ("title", "author", "creationDate"):
        v = (doc.metadata or {}).get(k)
        if v:
            meta[k] = v
    spans: list[tuple[int, float, str]] = []
    for pno, page in enumerate(doc, start=1):
        for b in page.get_text("dict").get("blocks", []):
            for line in b.get("lines", []):
                txt = "".join(s.get("text", "") for s in line.get("spans", [])).strip()
                if not txt:
                    continue
                size = max((s.get("size", 0.0) for s in line.get("spans", [])), default=0.0)
                spans.append((pno, size, txt))
    # Broodtekst = lettergrootte met de meeste tekens (robuuster dan de mediaan bij weinig regels).
    weight: dict[float, int] = {}
    for _, size, txt in spans:
        weight[round(size, 1)] = weight.get(round(size, 1), 0) + len(txt)
    body = max(weight, key=lambda k: weight[k]) if weight else 0.0
    sections = [Section("", 0, "", "p. 1")]
    start_page = 1
    for pno, size, txt in spans:
        is_head = body and size >= body * 1.25 and len(txt) < 120
        if is_head:
            if sections[-1].text.strip():
                sections[-1].locator = f"p. {start_page}" + (f"-{pno}" if pno > start_page else "")
            start_page = pno
            level = 1 if size >= body * 1.6 else 2
            sections.append(Section(txt, level, "", f"p. {pno}"))
        else:
            if not sections[-1].text:
                start_page = pno
            sections[-1].text += txt + "\n"
            last = sections[-1]
            first = last.locator.split("-")[0]
            last.locator = first if f"p. {pno}" == first else f"{first}-{pno}"
    doc.close()
    return Parsed(file_hash(path), [s for s in sections if s.text.strip()], meta)


# ---------------- Markdown ----------------


def parse_markdown(path: Path) -> Parsed:
    text = path.read_text(encoding="utf-8", errors="replace")
    sections = [Section("", 0, "", "regel 1")]
    in_code = False
    for n, line in enumerate(text.splitlines(), start=1):
        if line.startswith("```"):
            in_code = not in_code
        m = None if in_code else re.match(r"^(#{1,6})\s+(.*)", line)
        if m:
            sections.append(Section(m.group(2).strip(), len(m.group(1)), "", f"regel {n} > {m.group(2).strip()}"))
        else:
            sections[-1].text += line + "\n"
    title = next((s.title for s in sections if s.level == 1), path.stem)
    return Parsed(file_hash(path), [s for s in sections if s.text.strip()], {"format": "markdown", "title": title})


# ---------------- code ----------------


def parse_code(path: Path) -> Parsed:
    """Code per functie/klasse met regelbereik (Python via ast, rest via patronen)."""
    lang = CODE_EXT.get(path.suffix.lower(), "text")
    src = path.read_text(encoding="utf-8", errors="replace")
    lines = src.splitlines()
    spans: list[tuple[str, int, int]] = []
    if lang == "python":
        try:
            tree = ast.parse(src)
            for node in tree.body:
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                    end = getattr(node, "end_lineno", node.lineno) or node.lineno
                    spans.append((node.name, node.lineno, end))
        except SyntaxError:
            pass
    else:
        pat = re.compile(
            r"^\s*(pub(\([^)]*\))?\s+)?(async\s+)?(fn|impl|struct|enum|trait|class|interface|function|func|def|type)\s+([A-Za-z_][\w<>]*)"
        )
        starts = [(n, m.group(5)) for n, line in enumerate(lines, start=1) if (m := pat.match(line))]
        for i, (n, name) in enumerate(starts):
            end = (starts[i + 1][0] - 1) if i + 1 < len(starts) else len(lines)
            spans.append((name, n, end))
    sections = []
    covered = 0
    if spans and spans[0][1] > 1:
        head = "\n".join(lines[: spans[0][1] - 1])
        if head.strip():
            sections.append(Section("kop", 1, head, f"regels 1-{spans[0][1] - 1}"))
    for name, a, b in spans:
        sections.append(Section(name, 2, "\n".join(lines[a - 1 : b]), f"regels {a}-{b}"))
        covered = max(covered, b)
    if not spans:
        sections.append(Section(path.name, 1, src, f"regels 1-{len(lines)}"))
    return Parsed(file_hash(path), [s for s in sections if s.text.strip()], {"format": "code", "language": lang})


def parse_text(path: Path) -> Parsed:
    text = path.read_text(encoding="utf-8", errors="replace")
    # Blokken van ~60 regels met regelbereik.
    lines = text.splitlines()
    secs = []
    for i in range(0, max(len(lines), 1), 60):
        chunk = "\n".join(lines[i : i + 60])
        if chunk.strip():
            secs.append(Section("", 0, chunk, f"regels {i + 1}-{min(i + 60, len(lines))}"))
    return Parsed(file_hash(path), secs, {"format": "text"})


def parse(path: str | Path, media_type: str = "") -> Parsed:
    p = Path(path)
    kind = detect(p, media_type)
    fn = {
        "epub": parse_epub,
        "pdf": parse_pdf,
        "docx": parse_docx,
        "md": parse_markdown,
        "html": parse_html,
        "code": parse_code,
    }.get(kind, parse_text)
    out = fn(p)
    out.metadata.setdefault("title", p.stem)
    out.metadata["path"] = str(p)
    return out
