---
wave: W16
status: af  # pijplijnen gebouwd + end-to-end getest met test-engines; echte modellen (visie, OCR, Whisper, TTS, diffusion, ColPali) op de PC
modules: [inference-modelbeheer, research-kennis, gui]
---
# W16 — Multimodaal

- [~] `W16.01` Visie (Qwen3.5-9B): afbeeldingen, screenshots, grafieken; ROI-verwerking (overzicht → uitsnede) — model met rol `vision` + `mmproj` (llama-server `--mmproj`), `Message.images` → OpenAI-contentdelen; `look`: overzicht (1024 px) → model kiest ≤ 3 gebieden → uitsneden op volle resolutie → nauwkeurige beschrijving (tekst letterlijk, assen/legenda's); beelden ook naar het eindmodel als dat visie kan; getest met nep-model → **echt model op de PC**
- [~] `W16.02` OCR (GLM-OCR / PaddleOCR-VL) met onzekerheid doorgeven — worker `ocr` (PaddleOCR → Tesseract; regels met bbox + zekerheid); gemiddelde zekerheid → `confidence` van de herinnering, regels < `ocr_low` als "ONZEKER gelezen" in de prompt (getest: komt bij het model aan); gescande PDF-pagina's → renderen → OCR, anders visie → **engines op de PC**
- [x] `W16.03` Documentverwerking: PDF-lay-out, DOCX, spreadsheets; tabellen met structuurbehoud — PyMuPDF-blokken met coördinaten, leesvolgorde (kolommen), tabellen (`find_tables` → markdown, tabeltekst niet dubbel), scanpagina-detectie; DOCX in documentvolgorde met tabellen; XLSX/CSV als tabellen met herhaalde kop per 50 rijen en cel-locator (`Blad!A1:B51`); ingest gebruikt dit
- [~] `W16.04` Visuele documentretrieval (ColPali-achtig) + coördinaat-bronverwijzingen — regio's (blokken, tabellen) als herinneringen met locator `p. N [x0, y0, x1, y1]`; GUI rendert de pagina met het gebied gemarkeerd; ColQwen2-pagina-embeddings (`colpali_index`/`colpali_query`) in de worker → **GPU op de PC**
- [~] `W16.05` Spraak in (Whisper-klasse, CPU) + uit (TTS), onderbreekbaar, voice-modus in GUI — faster-whisper (CPU int8, segmenten met tijden + zekerheid) en Piper/Kokoro per zin; GUI: 🎙 opnemen → transcriberen → versturen, antwoorden voorlezen, onderbreken (stil/uit, nieuwe opname stopt het afspelen), 🔊 per bericht; audio als bijlage → transcript met tijdstempel-locators in het geheugen → **engines op de PC**
- [~] `W16.06` Beeldgeneratie/-bewerking (na ontladen LLM) — diffusers (tekst → beeld en beeld → beeld); eerst GPU-lease P1 + beheerde llama-servers ontladen, daarna terugladen; `/beeld <prompt>` in de chat (met beeldbijlage = bewerken), Studio-pagina → **diffusion op de PC**
- [x] `W16.07` Video: adaptieve frameselectie + tijdsuitlijning met audio/transcript — ffmpeg-scènewissels + min/max-interval + uitdunnen; audiospoor → transcript; elk frame krijgt de spraak tot het volgende frame; frames → visie; tijdlijn met locators `mm:ss–mm:ss` in het geheugen (getest met echte ffmpeg-video)
- [x] `W16.08` Meerdere modaliteiten combineren in één vraag; multimodale output (beeld, audio, bestanden) — meerdere bijlagen per bericht (uploads; ook afbeeldingen via de OpenAI-API) worden vóór de cyclus verwerkt en samen in de prompt gezet; uitvoer: beeld (`/beeld`, markdown-afbeelding), spraak (TTS), bestanden (tools); event `chat.attachments.processed`

## Gate
Elke modaliteit werkt in chat, gaat het geheugen in en wordt correct teruggevonden.
