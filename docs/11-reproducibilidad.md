# 11 — Reproducibilidad

Este documento describe cómo reconstruir y probar el proyecto desde el repositorio, sin depender del worktree local.

## Requisitos

- Linux, macOS o WSL.
- Rust 1.90+ (spotatui MSRV) y Cargo.
- `yt-dlp` en `PATH`; `ffmpeg` recomendado.
- Node.js 22+ para el relay Worker (el relay Rust no lo necesita).
- `git`.

## 1. Construir spotatui con la cola

```bash
./scripts/bootstrap.sh
```

El script:

1. Clona `LargeModGames/spotatui` en `vendor/spotatui` en el tag `v0.48.0`.
2. Aplica `patches/0001-party-queue.patch`.
3. Compila con `telemetry,tui,streaming,youtube`.
4. Imprime la ruta del binario y el comando del relay.

Si el parche ya está aplicado, el script lo detecta y no lo duplica. Si el árbol no es v0.48.0, falla en vez de aplicar a ciegas.

## 2. Relay Rust

```bash
cd party-relay
cargo test
cargo run --release
```

Escucha en `ws://127.0.0.1:8080/ws`; healthcheck en `http://127.0.0.1:8080/healthz`.

## 3. Relay Worker (opcional)

```bash
cd work/spotatui/worker-relay   # o vendor/spotatui/worker-relay tras bootstrap
npm ci
npm test
npx tsc --noEmit
PATH=/ruta/a/node22/bin:$PATH npx wrangler dev --local --port 8787
```

El Worker usa Durable Objects; `wrangler.toml` trae entornos staging y producción. No se despliega desde este repositorio sin credenciales.

## 4. Pruebas

```bash
# Núcleo + red + TUI con YouTube
cd vendor/spotatui
cargo test --locked --no-default-features \
  --features telemetry,tui,streaming,youtube --lib

# Integración real host + guest contra un relay vivo
PARTY_RELAY_URL=ws://127.0.0.1:8080/ws \
  cargo test --locked --no-default-features \
  --features telemetry,tui,streaming,youtube \
  two_apps_exchange -- --ignored

# Protocolo contra el relay
PARTY_RELAY_URL=ws://127.0.0.1:8080/ws \
  cargo test --locked --no-default-features \
  --features telemetry,tui,streaming,youtube \
  relay_roundtrip -- --ignored

# Relay Rust
cd ../../party-relay && cargo test

# Smoke del protocolo (create/join/add/ack/patch)
cd .. && node scripts/smoke-party.mjs --url ws://127.0.0.1:8080/ws

# YouTube real (metadatos)
./scripts/smoke-youtube.sh
```

## 5. Prueba manual

1. Arranca el relay.
2. Arranca dos instancias de spotatui.
3. En ambas, configura `behavior.relay_server_url: "ws://127.0.0.1:8080/ws"`.
4. Host: `Ctrl+P` → crear sala.
5. Invitado: `Ctrl+P` → código + nombre.
6. En el invitado, busca un vídeo de YouTube con `/`, selecciónalo y pulsa `z`.
7. Abre `Q` en ambos: la cola debe coincidir y el vídeo debe empezar en los dos.
8. Prueba `x`, `J/K`, `Enter` y el modo de control compartido.

La primera reproducción tarda lo que tarde `yt-dlp` en descargar el audio. El guest mantiene la petición pendiente 30 s para no reiniciar la descarga en cada tick de 2 s.

## 6. Limitaciones conocidas

- YouTube/yt-dlp es una integración no oficial; actualiza `yt-dlp` si falla.
- La sincronización es best-effort (seek), no sample-accurate.
- El host es un punto único; si se cae, la sala muere.
- La cola es YouTube-only; Spotify sigue disponible como transporte legado, no como cola.
