# CARD — `app/` (Tauri 2 + React + TS)

**Doel:** de GUI, 1-op-1 volgens `VAULT/design/reference-chat.png`. **Waves:** W00 (skelet), W03 (chat), elke wave eigen schermen.

## Ingangen
`src/App.tsx` · `src/gen/` (TS-contracten uit proto, `pnpm gen`, niet bewerken) · `src-tauri/` (Rust-shell, buiten core-workspace)

## Commando's
`pnpm install` · `pnpm dev` (browser) · `pnpm tauri dev` (app) · `pnpm build` · `pnpm lint` · `pnpm gen`

## Let op
`src-tauri` bouwt alleen met Tauri-vereisten (Windows: WebView2 + VS Build Tools). Iconen: `pnpm tauri icon app-icon.png`.
