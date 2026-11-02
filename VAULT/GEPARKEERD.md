---
type: geparkeerd
updated: 2026-10-31
---
# Geparkeerd — pas bouwen op commando

## 1. Ethics & security layer
**Trigger:** gebruiker zegt letterlijk **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"**. Dan alles hieronder in één samenhangende laag (wave [[waves/W19-ethics-security]]).

Verzamelde items (hooks bestaan al, policies niet):
- Permissielagen per tool en per werkruimte; toegangspolicy op capability handles
- Bevestiging bij onomkeerbare/externe acties
- Prompt-injection trust boundaries (policy op de herkomstlabels die er al zijn)
- Webinhoud mag geheugen niet via instructies sturen (policy in conflict-team/Memory Writer)
- Netwerkbeleid per connector (allowlists, lokale adressen/redirects blokkeren)
- Sandbox-rechten aanscherpen
- Auditbeleid, bewaartermijnen backups/logs
- Ethiek-richtlijnen voor gedrag, onderzoek naar personen, autonome acties
- Veilige scheiding van wallets/sleutels bij autonome acties

## Bewust geschrapt (één gebruiker, één PC)
Horizontal scaling, failover/high availability, multi-user isolatie, conversation sharing/collaboration, kosten per gebruiker, worker leases met fencing tokens (pas bij meerdere workers), prefill/decode disaggregation, complexe GPU-topologieplaatsing (vaste rolverdeling volstaat). Terughalen als de situatie verandert.

## 2. Trading
**Trigger:** gebruiker zegt letterlijk **"TRADING NU MAKEN"**, en pas als de rest optimaal werkt (wave [[waves/W20-trading]]).

Al verzameld (alleen inspiratie, eigen connectors op endpoints):
- Inspiratie: OpenBB, Fincept Terminal, Godel Terminal, Nautilus Trader, Backtrader
- Bronnen: Yahoo Finance (onofficieel, kan breken → fallback), SEC EDGAR, CBOE, ECB, Federal Reserve, FINRA, Finviz, FMP, FRED, Polygon, Tiingo, Benzinga, Intrinio, TradingEconomics, IMF, OECD, DBnomics, World Bank, AkShare, Kraken, HyperLiquid, CoinGecko, exchanges (ccxt als referentie), GDELT
- Eigen DataProvider-interface: per bron connector met rate-limiting, caching, retries; normaliseren (OHLCV, fundamentals, nieuws → events in geheugen); gebruiksvoorwaarden per API
- Rolverdeling: LLM = research/nieuws/sentiment/strategie-code; signalen = statistiek + tijdreeksmodellen; backtests (eigen engine, Nautilus/Backtrader als inspiratie); harde risicoregels in code
- Paper trading eerst, verlieslimieten, kill switch, audit log
- Wereldmonitor + spelers-kennis (W14) hergebruiken
