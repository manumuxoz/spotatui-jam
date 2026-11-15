# spotatui-jam

Extensión de [spotatui](https://github.com/LargeModGames/spotatui) para convertir su **Listening Party** en una jam real usando **YouTube**: reproducción sincronizada y una **cola compartida** que todos los participantes pueden ver y modificar.

> **Estado:** MVP implementado y probado sobre spotatui 0.48.0. El fork de desarrollo vive en `work/spotatui` (no versionado; rama `feat/party-queue`, commit `e5be778`) y el repositorio publica el parche reproducible [`patches/0001-party-queue.patch`](patches/0001-party-queue.patch). La cola es **YouTube-only**; el transporte Spotify de Listening Party se mantiene por compatibilidad, pero no se usa para la cola. Ver [`docs/10-implementation-status.md`](docs/10-implementation-status.md).

---

## Veredicto rápido

| Pregunta | Respuesta |
|---|---|
| ¿Existe ya Spotify Jam? | Sí, como función de los clientes oficiales de Spotify. **No tiene API pública documentada** para crear/unirse/gestionar una Jam. |
| ¿Existe ya una extensión así para spotatui? | **No.** spotatui 0.48 tiene Listening Party (sync de playback), pero no cola compartida. |
| ¿Por qué YouTube? | No exige Spotify Premium ni developer app; `yt-dlp` ya es una fuente de spotatui y permite probar el flujo completo con una URL/ID. |
| ¿Se puede integrar la Jam nativa de Spotify? | Solo por vías no documentadas (`spotify_cli`, endpoints internos). No es la base del producto. |
| ¿Cuál es la vía recomendada? | **Extender Listening Party** con una cola autoritativa del host y reproducir vídeos de YouTube en cada participante. |

La investigación original, con fuentes y fechas, está en [`research/00-sintesis.md`](research/00-sintesis.md). El cambio de alcance a YouTube está documentado en [`docs/10-implementation-status.md`](docs/10-implementation-status.md).

---

## Qué construimos

**spotatui-jam** convierte el Listening Party en una jam de terminal con YouTube:

- El host crea una sala y comparte un código de 6 caracteres.
- Todos los participantes reproducen el **mismo vídeo de YouTube**, en la misma posición (best-effort).
- El host mantiene la **cola compartida autoritativa**; los invitados la ven en tiempo real.
- Los invitados buscan en YouTube y **añaden vídeos a la cola**; según permisos, también pueden quitarlos o reordenarlos.
- El host modera: límites por invitado, tamaño máximo, filtro de duplicados y cooldown.
- La sala y el relay son **self-hostables**; no se depende de una nube propietaria.

### Requisitos

- spotatui compilado con `youtube` (y `streaming` si también quieres Spotify).
- `yt-dlp` en `PATH`; `ffmpeg` recomendado.
- Cada participante necesita su propia instancia de spotatui, pero **no** una cuenta de Spotify para la jam de YouTube.

### Lo que no es

- No es un cliente de la Jam oficial de Spotify ni promete interoperar con ella.
- No retransmite audio: cada participante descarga/reproduce el mismo vídeo de YouTube en su dispositivo.
- No es un servicio de webcasting ni un producto comercial.
- YouTube/yt-dlp es una integración no oficial y puede romperse cuando YouTube cambie; la mitigación es actualizar `yt-dlp`.

---

## Decisiones de arquitectura (resumen)

1. **Base:** fork de spotatui. Listening Party ya resuelve auth, relay, presencia y sincronía; el gap era la cola.
2. **Autoridad:** el host es la única fuente de verdad de la cola. Los invitados envían comandos; el host valida, aplica y difunde un `revision` monotónico.
3. **Playback:** cada participante reproduce el vídeo de YouTube del host con su propia instancia. La cola se implementa sobre `native_queue: Vec<TrackInfo>` y se restringe a `youtube:` en la v1.
4. **Protocolo:** `SyncMessage` v2 con snapshots troceados y deltas; compatible con clientes v1. Mensajes pequeños para respetar el límite de 4 KB del relay.
5. **Relay:** se actualizó el Worker de Cloudflare incluido en spotatui y se añadió un relay Rust auto-hospedable. El relay sigue siendo tonto: no guarda la cola, solo enruta.
6. **Spotify:** el transporte Spotify de Listening Party se conserva para clientes v1, pero la Party Queue no acepta `spotify:track:`.
7. **Upstream-first:** PR pequeño y revisable contra spotatui; si no se acepta, fork mantenido con rebase. La licencia MIT lo permite.

Detalle y alternativas en [`docs/02-arquitectura.md`](docs/02-arquitectura.md) y [`docs/07-adr/`](docs/07-adr/). Cambios por versión en [`CHANGELOG.md`](CHANGELOG.md); flujo de contribución en [`CONTRIBUTING.md`](CONTRIBUTING.md).

---

## Estructura del repositorio

```
spotatui-jam/
├── README.md                     ← este documento
├── LICENSE                       ← MIT
├── .github/workflows/ci.yml      ← patch + relay + scripts
├── docs/
│   ├── 01-vision-producto.md     ← problema, usuarios, alcance, UX
│   ├── 02-arquitectura.md        ← componentes, protocolo, modelo de datos
│   ├── protocolo-party-v2.md     ← especificación de mensajes y test vectors
│   ├── 03-plan-entrega.md        ← fases, hitos, estimaciones, gates
│   ├── 04-pipeline.md            ← flujo de desarrollo y CI/CD
│   ├── 05-calidad-y-pruebas.md   ← estrategia de pruebas
│   ├── 06-riesgos.md             ← registro de riesgos y mitigaciones
│   ├── 07-adr/                   ← decisiones arquitectónicas
│   ├── 08-backlog.md             ← épicas, historias y criterios de aceptación
│   ├── 09-operacion.md           ← relay, despliegue, observabilidad
│   ├── 10-implementation-status.md ← qué está hecho, pruebas y deuda
│   └── 11-reproducibilidad.md    ← reconstruir y probar desde cero
├── research/
│   ├── 00-sintesis.md            ← conclusión de la investigación original
│   ├── 01-spotatui-architecture.md
│   ├── 02-existing-projects.md
│   └── 03-technical-feasibility.md
├── party-relay/                  ← relay Rust auto-hospedable (crate independiente)
├── patches/
│   └── 0001-party-queue.patch    ← parche aplicable a spotatui v0.48.0
├── pipeline/
│   ├── ci.yml                    ← GitHub Actions (PR)
│   ├── nightly.yml               ← upstream + auditoría
│   └── release.yml               ← artefactos + relay
├── scripts/
│   ├── smoke-party.mjs           ← create/join/add/ack/patch contra un relay
│   ├── smoke-youtube.sh          ← metadatos reales de yt-dlp
│   └── bootstrap.sh              ← clona v0.48.0 + aplica el parche + compila
└── plan/
    ├── checklist-fase-0.md       ← qué validar antes de escribir código
    ├── decision-fase-0.md        ← plantilla de decisión del gate G0
    └── definicion-de-hecho.md
```

---

## Cómo probarlo ahora

```bash
# Clona spotatui v0.48.0, aplica el parche y compila
./scripts/bootstrap.sh

# Relay Rust, en otra terminal
cd party-relay && cargo run --release

# spotatui con YouTube (el bootstrap ya lo compila)
cd vendor/spotatui
./target/debug/spotatui
```

Configura `behavior.relay_server_url: "ws://127.0.0.1:8080/ws"`. Host: `Ctrl+P` → crear. Invitado: `Ctrl+P` → unirse. `Q` abre la cola; `/` busca; `z` añade; `x` quita; `J/K` mueve; `Enter` pone ahora.

Para validar la fuente YouTube sin abrir la TUI:

```bash
./scripts/smoke-youtube.sh
# youtube OK: youtube:dQw4w9WgXcQ | ... | 213s
```

## Siguiente paso

1. Leer [`docs/10-implementation-status.md`](docs/10-implementation-status.md): qué está hecho, qué falta y qué pruebas pasan.
2. Probar con dos instancias y un vídeo de YouTube en la misma LAN.
3. Revisar el parche y decidir upstream vs fork.
4. Completar P1: votos, i18n de errores del núcleo, paridad GUI, reconexión y despliegue del Worker.

## Supuestos de trabajo

- La cola v1 es **YouTube-only**: se aceptan URIs `youtube:<id>`; `spotify:track:` se rechaza en la cola.
- Cada participante tiene `yt-dlp` y reproduce el vídeo localmente; no se retransmite audio.
- El alcance inicial es una sala de hasta 8 personas y 100 vídeos en cola.
- El host reproduce en el dispositivo de spotatui; si usa una fuente distinta, la cola de YouTube no se sincroniza.
- El proyecto se publica con licencia MIT, compatible con spotatui, y se intenta upstream antes de mantener un fork.

Si quieres volver a Spotify, el transporte sigue existiendo, pero la cola YouTube-only no acepta sus URIs; haría falta un adaptador de fuente adicional.
