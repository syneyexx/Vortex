# CARD — `app/` (Tauri 2 + React + TS)

**Doel:** de GUI, 1-op-1 volgens `VAULT/design/reference-chat.png`. **Waves:** W03 (chat), elke wave eigen schermen.

## Structuur
| Pad | Wat |
| --- | --- |
| `src/styles/tokens.css` | design tokens (kleur, radii, spacing, typografie, glow) — één bron |
| `src/styles/app.css` | layout + componenten (onderaan: verfijningen na visuele vergelijking) |
| `src/api/client.ts` · `events.ts` · `types.ts` | REST, WebSocket met hervatten (from_seq, backoff, rAF-batching), typen |
| `src/state/reduce.ts` | pure reducer events → toestand (getest: `reduce.test.ts`) |
| `src/state/store.ts` | store (useSyncExternalStore), snapshot + stream, sessies |
| `src/components/` | TopNav, LeftColumn (hero/agents/memory), ChatPane, Composer, Markdown, TaskBar, StatusBar, VortexShader, Icons |
| `src/pages/Pages.tsx` | Dashboard, Lopend werk, Models, Instellingen (prompts + versies), lege staten |
| `src/gen/` | TS-contracten uit proto (`pnpm gen`, niet bewerken) |
| `src-tauri/` | Rust-shell (buiten core-workspace) |

## Regels
- Nooit nep-data: wat nog niet bestaat toont een lege staat met de wave.
- Tijdens streamen geen highlighting/LaTeX per frame (pas bij afronden) → 60 fps.
- WebGL: één geanimeerde context (hero); logo/avatars = gecachet frame.

## Commando's
`pnpm dev` (vite, API op :7420) · `pnpm build` (dan `vortex serve` serveert `dist/` op http://127.0.0.1:7420) · `pnpm tauri dev` · `pnpm test` · `pnpm lint`
