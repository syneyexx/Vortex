---
wave: W14
status: af  # alles gebouwd + getest tegen nepbronnen; live bronnen en HF Hub zijn geblokkeerd in de cloud → live-check op de PC; meerdaagse gate 🔒
modules: [research-kennis, cognitie]
---
# W14 — Wereldkennis
Zie [[modules/research-kennis]].

- [~] `W14.01` Connectors: GDELT, RSS (bronnenlijst meerdere landen/richtingen), Wikipedia Current Events, Wikidata, ReliefWeb, USGS, NASA EONET, World Bank, Eurostat, CBS, arXiv, Hacker News — rate limits + voorwaarden per bron — `vortex-crawler::world::sources` (12 soorten, RSS/RDF/Atom via quick-xml, URL's instelbaar), `terms()` = voorwaarden + minimale tijd per soort (tempo per soort), `config/world.toml` (22 bronnen: NL/UK/US/DE/FR/QA/IN/AU/JP, publiek/links/rechts/staats/commercieel), Wikidata-koppeling (Q-ID per event); getest tegen nepservers → **live-check op de PC** (cloud-proxy blokkeert de hosts)
- [x] `W14.02` Gepland ophalen + inhaal-modus — planner (`world_api::spawn_world`, elke minuut): bron aan de beurt per interval; venster = sinds laatste geslaagde keer (max 7 dagen); periodieke stappen (`__resolve`, `__hypothesize`, `__predict`, `__summary_*`) ook persistent → inhalen na herstart; elke cyclus = P3-run via de Dirigent; LM-stappen alleen als de gebruiker niet actief is
- [x] `W14.03` Crawler: headless browser voor JS-pagina's, recrawl-schema voor gevolgde sites, Ranker als neurale layer (data uit W10) — `--headless --dump-dom` (Chrome/Edge/Chromium, `paths.browser` of automatisch) als de HTML vrijwel geen tekst maar wel scripts heeft (getest met echte Chromium); `followed_sites` + `recrawl_due` (wijziging → item); layer `crawl_rank` (9 kenmerken, label = pagina door de Curator bewaard, uit frontier + `crawl.page.fetched`)
- [x] `W14.04` Event-clustering (artikelen → één event), gemeld vs bevestigd — tokens + Dice-gelijkenis in een tijdvenster, toewijzen aan bestaand event of nieuw; **bevestigd** bij ≥ 2 onafhankelijke brongroepen (domein), anders gemeld; thema + regio; idempotent
- [x] `W14.05` Temporal KG-integratie, bronbias — bevestigde events → herinnering (kind World, `valid_from` = eerste melding, bron + Wikidata-entiteiten) → entiteiten/claims in de graaf; bronbias per bron (aandeel in bevestigde events, landen, richting)
- [x] `W14.06` Causale hypotheses + voorspellingen + later toetsen (kalibratie wereldmodel) — hypotheses (steun/contra, vertrouwen), voorspellingen met kans + toetscriteria + datum, automatisch toetsen tegen binnengekomen items (waar/onwaar/niet te beslissen + bewijs), kalibratie (Brier, ECE, per herkomst/speler, Platt bij ≥ 20)
- [x] `W14.07` Dag/week/maand-samenvattingen per thema/regio — per groep, bevestigd vs gemeld gescheiden, extractieve terugval zonder model; ook als herinnering (Summary)
- [x] `W14.08` Spelers per sector: bronnen verzamelen, beslissingen/principes/uitkomsten extraheren, mislukkingen, voorspellingen toetsen — alleen organisaties en publieke figuren met publieke rol; bronnen = wereldmonitor + ingelezen documenten; records (decision/principle/failure/outcome) met bron + locator in het geheugen; "wat zou X doen?" = voorspelling met `actor`, zelfde toetsing + kalibratie per speler
- [~] `W14.09` HF-datasets: streamen, schema, mapping-voorstel + bevestiging, filter, dedup (MinHash), decontaminatie, licentie, bestemming kiezen, catalogus — `worker/hf_datasets.py` (streaming, schema chat/qa/preference/code/text, mapping-voorstel + LM-verfijning, kwaliteitsfilter, exact + MinHash-LSH ook tegen eerdere imports, 8-gram-decontaminatie, licentie uit de card); bestemmingen kennis / SFT / DPO / layer (router) / eval; `hf_catalog`; GUI: bevestigen vóór import; end-to-end getest met lokale bestanden → **HF Hub streamen op de PC** (cloud blokkeert huggingface.co)

## Gate
🔒 (meerdaagse run op commando) Wereldmonitor draait dagen stabiel; voorspellingen worden automatisch getoetst; een HF-dataset gaat end-to-end naar elke bestemming.
