# Changelog

## v0.1.0-youtube — 2026-11-15

Primer MVP de cola colaborativa para Listening Party, con YouTube como fuente.

### Añadido

- `PartyQueue` autoritativa en el host con `current`, `items`, `history`, revisión, IDs, permisos, límites y deduplicación.
- Protocolo v2: `queue_snapshot`, `queue_patch`, `queue_command`, `queue_ack`, `queue_resync`, `party_config` y `party_members`.
- Sincronización host/guest de vídeos de YouTube, con debounce de 30 s para no reiniciar la descarga de `yt-dlp` en cada tick.
- UI TUI: cola en el popup Party y en la pantalla Queue; `Q` abre la cola, `z` añade desde búsqueda, `x` quita, `J/K` mueve, `Enter` pone ahora.
- Relay Rust auto-hospedable (`party-relay/`) con `/ws`, `/healthz`, límites, rate limit y Dockerfile.
- Worker de Cloudflare v2 con routing puro testeado, `healthz`, `QUEUE_ENABLED` y `MAX_MESSAGE_BYTES`.
- Parche reproducible `patches/0001-party-queue.patch` contra spotatui `v0.48.0`.
- Scripts `smoke-party.mjs`, `smoke-youtube.sh` y `bootstrap.sh`.
- Documentación de arquitectura, protocolo, operación, riesgos, backlog y reproducibilidad.

### Cambiado

- La cola es YouTube-only: se aceptan `youtube:<id>` y se rechazan `spotify:track:` y otras fuentes.
- `start_party`/`join_party` funcionan sin sesión de Spotify cuando se compila con `youtube`.
- El transporte Spotify legado de Listening Party se mantiene para compatibilidad.

### Pruebas

- 2474 tests en el build default de spotatui.
- 2305 tests con `telemetry,tui,streaming,youtube`.
- 18 tests del relay Rust.
- 17 tests del Worker.
- Integración real host+guest contra relay Rust y Worker.
- Smoke real de `yt-dlp` (metadatos y descarga de 5 s).
