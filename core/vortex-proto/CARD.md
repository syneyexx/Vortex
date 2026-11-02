# CARD — `vortex-proto`

**Doel:** gegenereerde Rust-gRPC-code uit `/proto` (bron van waarheid). Geen protoc nodig (protox).
**Kaart:** VAULT/15-CONTRACTEN.md · **Waves:** W00

## Publieke API
`vortex_proto::{common, worker, gui, admin, sandbox}::v1::*` — berichten, clients, servers (tonic).

## Regels
Nooit met de hand bewerken; wijzig `.proto` en draai `just gen` (Python + TS). Rust genereert bij build.
