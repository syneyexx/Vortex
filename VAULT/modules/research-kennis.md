---
type: module
crate: worker/ingest + vortex-memory
waves: [W09, W13]
updated: 2026-10-31
---
# Research en kennis

## Gedeelde ingest-pipeline
Ruwe data (HDD) → parsen → structuur-bewust chunken → contextbeschrijving → embedden → entiteiten + claims → graph → dedup → conflictcheck → hiërarchische samenvattingen → geheugen (SSD). Achtergrond, voortgang in GUI, extractie op 1060. Content-hashes → incrementeel. Herkomstafhankelijkheden.

## Bronnen
| Bron | Werkwijze | Wave |
| --- | --- | --- |
| Onderwerp-research | plan + deelvragen → zoeken → lezen → claims → kruiscontrole → rapport met bronverwijzingen (brongebonden: passage ↔ claim); iteratief, adaptief bijsturen; onderzoek naar ontbrekend bewijs | W9 |
| Bestanden | ebooks (EPUB, PDF), docs, code, lokale mappen doorzoekbaar | W9 |
| Tabellen | structuurbehoud (samengevoegde cellen, kopregels, eenheden, voetnoten) | W15 |
| Visuele documenten | ColPali-achtige retrieval, coördinaat-bronverwijzing, OCR-onzekerheid doorgeven | W15 |
| Wereldmonitor | GDELT, RSS (meerdere landen/richtingen), Wikipedia Current Events + Wikidata, ReliefWeb, USGS, NASA EONET, World Bank/Eurostat/CBS, arXiv, Hacker News | W13 |
| Grote spelers per sector | interviews, boeken, aandeelhoudersbrieven, jaarverslagen, filings → beslissingen, context, uitkomst, principes; ook mislukkingen (survivorship bias); "wat zou X doen"-voorspellingen toetsen | W13 |
| Hugging Face-datasets | zie hieronder | W13 |

## Eigen webcrawler-team (besloten 2026-10-31)
Vortex zoekt niet alleen via SearXNG, maar crawlt zelf gericht door.
| Rol | Taak |
| --- | --- |
| Scout | zoekvragen maken (SearXNG), startpunten kiezen |
| Crawler | ophalen (HTTP; headless browser voor JS-pagina's), links volgen op relevantie, diepte- en tijdbudget |
| Reader/Extractor | hoofdtekst, metadata, tabellen eruit (trafilatura-achtig), PDF's doorsturen naar parsers |
| Ranker | relevantie voor de onderzoeksvraag, value-of-information: welke link is de volgende waard |
| Curator | dedup (content-hash + near-duplicate), kwaliteit, bronbetrouwbaarheid → ingest-pipeline |
- **Focused crawling**: volgt alleen links die bij de vraag passen; stopt als nieuwe pagina's niets toevoegen.
- **Frontier/queue** per taak in SQLite; ruwe HTML op HDD; extracten via ingest naar geheugen (SSD).
- **Recrawl-schema** voor gevolgde sites (versheid), gekoppeld aan wereldmonitor en leeragenda.
- **Per-domein tempo + robots.txt** respecteren: nodig om niet geblokkeerd te worden en sites niet te overbelasten.
- Rust (reqwest + HTML-parser) voor ophalen, Python voor extractie; draait op CPU (Ryzen 9), parallel per domein.
- Cache + negative caching; alles als events → tracing en trainingsdata (Ranker wordt later een neurale layer).

## Wereldmonitor-details
Veel artikelen → één event; "gemeld" vs "bevestigd" (onafhankelijke bronnen); temporal KG (wie, wat, waar, wanneer, geldigheid); causale hypotheses met vertrouwen; **voorspellingen doen en later toetsen** (kalibratie); dag/week/maand-samenvattingen per thema/regio; filter op belang; bronbias bijhouden; ruwe artikelen HDD; gepland ophalen + inhaal-modus bij opstarten.

## Hugging Face-datasets
`datasets` + `huggingface_hub`; streamen (32 GB RAM); schema herkennen (chat, Q&A, preference, code); LM stelt mapping voor, gebruiker bevestigt; kwaliteitsfilter; dedup exact + MinHash (ook tegen bestaande data); decontaminatie t.o.v. eval; licentie + herkomst; bestemming: kennis / LM-training (SFT, DPO) / training neurale layers / eval-set; datasetcatalogus met versie.

## Uitvoer
Rapporten, tabellen, datasets, artefacten; bewijsoverzicht; global corpus synthesis voor vragen over hele verzamelingen.
