# 10 — Estado de implementación

**Fecha:** 15 de noviembre de 2026.
**Base:** spotatui `v0.48.0` (`3174b77`), fork en `work/spotatui`, rama `feat/party-queue`, commit `e5be778`.
**Alcance de la cola:** YouTube-only (`youtube:<id>`).
**Parche exportado:** [`patches/0001-party-queue.patch`](../patches/0001-party-queue.patch).

---

## 1. Qué está implementado

### Núcleo

- `src/core/party/queue.rs`:
  - `PartyQueue` con `current`, `items`, `history`, `revision`, `next_id`, `config`, `members`.
  - `PartyQueueItem`, `PartyTrack`, `PlayedItem`, `PartyMember`.
  - Validación de URI `youtube:<id>`, metadata, duración, duplicados, límites y cooldown. Los URIs `spotify:track:` se rechazan en la cola.
  - Permisos `HostOnly` / `GuestsAdd` / `GuestsManage`.
  - `QueueAction` (add/remove/move/clear/play now/vote) y `QueueOp` (add/remove/move/clear/advance/set current/clear current/reset).
  - Replay de parches con `revision` contigua, snapshots y `SnapshotAssembler`.
  - `observe_playback()` para convertir un cambio de track del host en `Advance`/`ClearCurrent`.
  - 16 tests unitarios, incluido un test determinista de convergencia con 200 operaciones.

### Protocolo

- `src/infra/network/sync.rs`:
  - `SyncMessage` v2: `party_config`, `party_members`, `queue_command`, `queue_ack`, `queue_patch`, `queue_snapshot`, `queue_resync`.
  - `PartySession` con `members`, `participant_id`, `self_name`, `queue_revision`, `queue_enabled`.
  - Chunking por presupuesto de 3.500 bytes; límite duro de 4.096 bytes en `PartyConnection::send`.
  - Compatibilidad con mensajes v1 (campos nuevos con `serde(default)`).
  - 8 tests, incluidos golden de serialización y peor caso de chunking.

### Host e invitado

- `src/infra/network/mod.rs`:
  - Handshake `v=2&features=queue&name=...`.
  - Host: `start_party_queue()`, `observe_party_playback()`, `party_queue_command()`, `send_party_config()`, `send_party_snapshot()`, `handle_party_queue_command()`.
  - Guest: snapshot/patch/ack/config/members; resync al detectar un gap.
  - IDs de participante y `from`/`from_id`; idempotencia por `op_id`.
  - Errores de relay recuperables (rate limit, mensaje grande) sin cerrar la sala.
  - `Q` abre la pantalla Queue existente; `z` desde búsqueda añade a la cola; `x`, `J/K` y `Enter` se enrutan a la cola de Party.
- `src/core/app/queue.rs` intercepta las acciones de cola existentes; no hay una Action nueva.
- `src/core/app/party.rs` inicializa, proyecta y observa la cola.
- `src/tui/ui/popups.rs` muestra la cola en el popup Party y en la pantalla Queue.
- `src/tui/handlers/party.rs` añade `Q` para abrir la cola.
- `src/tui/handlers/queue_menu.rs` lee y modifica la cola de Party cuando está activa.

### Relays

- **Rust self-hosted:** `party-relay/`, crate independiente con axum/tokio.
  - `/ws`, `/healthz`, códigos de 6, 8 participantes, TTL, rate limit, `to`, IDs, `from`/`from_id`, `QUEUE_ENABLED`.
  - 6 tests unitarios + 12 de integración con `tokio-tungstenite`.
  - Dockerfile y README.
- **Cloudflare Worker:** `work/spotatui/worker-relay/`.
  - Handshake v2, routing puro en `src/routing.ts`, tests Node, `wrangler.toml` con staging/producción, `MAX_MESSAGE_BYTES` y `QUEUE_ENABLED`.
  - 17 tests de routing; `tsc --noEmit`, bundle y TOML validados.
  - Se validó con `wrangler deploy --dry-run` y `wrangler dev` (Node 22); no se desplegó a una cuenta real.

### Pruebas ejecutadas

| Comando | Resultado |
|---|---|
| `cargo test --locked` (default spotatui) | **2474 passed**, 0 failed, 10 ignored |
| `cargo test --locked --no-default-features --features telemetry,tui,streaming --lib` | **2250 passed**, 0 failed, 10 ignored |
| `cargo test ... telemetry,tui,streaming,youtube --lib` | **2305 passed**, 0 failed, 20 ignored |
| `cargo test` en `party-relay` | **18 passed** (6 unit + 12 integration) |
| `npm test` en `worker-relay` | **17 passed** |
| `npx tsc --noEmit` (worker) | OK |
| `cargo fmt --check` (spotatui y relay) | OK |
| `cargo clippy ... -D warnings` (slim+streaming) | OK |
| `cargo clippy ... -D warnings` (slim+streaming+youtube, all-targets) | OK |
| `scripts/smoke-party.mjs` contra relay Rust y Worker | `add -> ack -> patch` OK en ambos |
| `scripts/smoke-youtube.sh` (yt-dlp real) | OK: `youtube:dQw4w9WgXcQ`, título y 213 s |
| Descarga de 5 s con `yt-dlp` + `ffprobe` | OK: m4a de 81 KiB, 5,015 s |
| `wrangler deploy --dry-run --env staging` (Node 22) | OK; bundle 17,5 KiB |
| `./scripts/bootstrap.sh /tmp/...` | OK: clona v0.48.0, aplica el parche y compila |
| `relay_roundtrip_carries_a_queue_command_from_a_guest` (`--ignored`) | OK contra relay real |
| `two_apps_exchange_a_queue_add_through_the_real_relay` (`--ignored`) | OK contra Rust y contra Worker |

Los tests ignorados requieren `PARTY_RELAY_URL=ws://127.0.0.1:8080/ws` y el relay arrancado:

```bash
cd party-relay && cargo run --release
cd ../work/spotatui
PARTY_RELAY_URL=ws://127.0.0.1:8080/ws \
  cargo test --locked --no-default-features --features telemetry,tui,streaming \
  -- --ignored
```

---

## 2. Qué no está implementado (P1/P2)

- Votación y orden por score.
- Aprobación previa de tracks.
- Exportar la cola a playlist.
- Invitado web sin cuenta.
- Reconexión del host con gracia.
- Paridad de la página Party del GUI/desktop.
- i18n: la UI de cola usa `PopupStrings`; los mensajes de error del núcleo siguen en español.
- Textos de error de `PartyQueue` sin traducir.
- `allow_explicit`, `stop_when_empty` y `votes_enabled` están modelados pero no aplicados por completo.
- Prueba con audio real de YouTube en dos máquinas (requiere oído humano y red).
- Cola para Spotify: el transporte sigue, la cola no.
- Despliegue real del Worker en Cloudflare.
- Paquete de release y firma.

---

## 3. Desviaciones del plan

1. **`party_queue` pasó de `pub` a privado** para respetar el ratchet `pub_fields_on_app` (111). La UI usa `party_queue()`.
2. **El guest no proyecta `native_queue`**; dibuja desde `party_queue()` para no convertir el espejo en una cola reproducible local.
3. **`party_yields_to_local_playback()`** ahora permite el slot nativo cuando el host reproduce un track Spotify de la cola; los items decodificados siguen bloqueados.
4. **El chunking es por bytes** (3.500 B), no por 8 items fijos.
5. **La cola se materializa en `native_queue`** en el host; el guest solo consume `SyncState` y su espejo.
6. **El relay Rust** se añadió ya en esta iteración, no en P1, porque permite probar el flujo completo sin Cloudflare.
7. **La cola pasó a YouTube-only** por decisión del operador: no requiere Premium y se puede validar con `yt-dlp`. El transporte Spotify de Listening Party se mantiene, pero la cola rechaza sus URIs.
7. **El Worker usa `from` = nombre y `from_id` = id**, no `from` = id. El host prioriza `from_id`.

---

## 4. Cómo probarlo a mano

```bash
# 1. Relay
cd party-relay
cargo run --release

# 2. spotatui (otra terminal; ver docs/11-reproducibilidad.md)
cd vendor/spotatui
cargo build --locked --no-default-features --features telemetry,tui,streaming,youtube
./target/debug/spotatui
```

En `config.yml`:

```yaml
behavior:
  relay_server_url: "ws://127.0.0.1:8080/ws"
```

Host: `Ctrl+P` → crear sala. Invitado: `Ctrl+P` → unirse con el código. `Q` abre la cola; `/` busca; `z` añade; `x` quita; `J/K` mueve; `Enter` pone ahora.

La cola y la sala se pueden validar sin cuentas. Para el audio real, pon un vídeo de YouTube y comprueba que ambas instancias lo descargan con `yt-dlp`.

---

## 5. Siguiente paso

1. Revisar el diff del fork y el parche.
2. Probar con dos instancias y un vídeo de YouTube en la misma LAN.
3. Añadir i18n y los tests de GUI si se quiere paridad desktop.
4. Decidir si se abre el PR upstream o se mantiene el fork.
5. Desplegar el Worker en staging con Node 22 y ejecutar el smoke.
