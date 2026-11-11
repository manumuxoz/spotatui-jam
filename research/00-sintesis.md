# 00 — Síntesis de investigación

**Fecha:** 11 de noviembre de 2026.
**Pregunta:** ¿Existe ya una extensión de spotatui para hacer jams con una cola compartida entre varios usuarios? Si no, ¿qué haría falta para construirla?

---

## Respuesta corta

**No existe.** Lo más cercano es la **Listening Party de spotatui 0.48**, que sincroniza la reproducción entre host e invitados, pero **no comparte la cola**. La Jam oficial de Spotify existe como producto de consumo, pero **no tiene API pública** para crear, unirse o gestionar sesiones. Los proyectos de cola compartida que sí existen son web/jukebox, no extensiones TUI de spotatui.

La funcionalidad es **técnicamente viable** y encaja de forma natural como extensión del protocolo de Listening Party. El gap real es la cola, no la sincronía.

---

## 1. Qué existe

### 1.1 Spotify Jam (oficial)

- Función de los clientes oficiales: host crea una sesión, invitados se unen por enlace/QR, todos añaden a una cola y controlan playback según permisos.
- **No hay endpoints de Jam** en la Web API ni en el Web Playback SDK a noviembre de 2026. Spotify declaró en abril de 2025 que Jam “pertenece más a los playback SDKs que a la Web API”; la petición formal de la comunidad (octubre de 2025) sigue sin implementación.
- Límites de la Web API para terceros: Development Mode permite **5 usuarios autenticados**; la cuota extendida exige organización con ≥250.000 MAU. Un grupo de amigos no cabe en el modelo oficial.
- Conclusión: **no se puede construir una Jam interoperable con la API documentada**.

Fuentes: [Web API Player](https://developer.spotify.com/documentation/web-api/reference/), [Web Playback SDK](https://developer.spotify.com/documentation/web-playback-sdk/reference/), [Quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes), hilo [Web API info for Jams](https://community.spotify.com/forum/en/discussion/6235408/web-api-info-for-jams), petición [Add Spotify API support for Jams](https://community.spotify.com/t5/Spotify-for-Developers/Add-Spotify-API-support-for-Jams/td-p/7169789).

### 1.2 spotatui Listening Party (lo más cercano)

spotatui 0.48.0 (7 nov 2026) incluye `Ctrl+p` → Listening Party:

| Capacidad | ¿La tiene? |
|---|---|
| Sala con código de 6 caracteres | Sí |
| Relay WebSocket | Sí (`wss://spotatui-party.spotatui.workers.dev/ws`) |
| Sync de track/posición/play-pause cada 2 s | Sí |
| Seek correctivo si drift > ~3 s | Sí |
| Control compartido de play/pause/skip | Sí desde 0.48 (issue #442) |
| Cola compartida | **No** |
| Miembros con identidad estable | Solo nombres |
| Permisos de cola | **No** |
| Votación | **No** |
| Persistencia de sala | **No** (muere con el host) |

Evidencia: `src/infra/network/sync.rs` define `SyncMessage` sin ningún mensaje de cola; `worker-relay/src/index.ts` solo reenvía `sync_state` y `playback_command`; el PR #684 dice explícitamente que el invitado ve “The host picks the songs”.

**Matiz importante:** la documentación de `docs/listening-party.md` está desactualizada cuando dice que los invitados “no envían comandos”; el changelog 0.48 y el código muestran que el control compartido ya funciona.

**Contexto upstream:** en la discusión #331 (julio de 2026) se pidió una Jam; el maintainer respondió que no existe y que spotatui ya tiene party mode. No hay issue ni PR de cola compartida. Esto confirma que la feature no está en el roadmap y que un PR upstream necesitará justificar el valor añadido.

### 1.3 Proyectos de cola compartida

| Proyecto | Tipo | Licencia | Cola | Playback | Relevancia |
|---|---|---|---|---|---|
| **spotatui Listening Party** | TUI Rust | MIT | No | Sync multi-dispositivo | Base directa |
| **Sarthakischill/spot-along** | TUI Node | Sin verificar | No | Sync host | Competidor TUI, sin cola |
| **Kyzenkms/spicetify-jam** | Extensión Spicetify | README MIT, sin LICENSE | Sí | Sync host | Referencia de UX |
| **citrus401/spotify-listen-together-2.0** | Spicetify | MIT | Espejo | Sync ~10 ms | Técnica de drift |
| **SAKMZ/syncwave** | Web self-host | PolyForm NC | Sí + votos | YouTube/yt-dlp | UX de sala |
| **viscoci/OpenDJ** | Web self-host | MIT | Sí | Host Connect | Arquitectura reutilizable |
| **jaherhum/crowdroom** | Web | AGPL-3.0 | Sí + votos | Host | Flujos |
| **Dbochman/EchoNest** | Web | Sin licencia | Sí | Individual | Modelo conceptual |
| **uhteddy/spartify** | macOS app | Sin licencia | Sí | Host | QR/invitados |
| **junga3/jammix** | Web | Sin licencia | Generada | Host | Mezcla por gustos |
| **mattiasahlsen/spotify-queue** | Web | Sin licencia | Sí | Host | Muerto (2021) |
| **niclaszll/spotify-listening-party** | Web | Apache-2.0 | Sí | Sync | Referencia académica |
| **AugustoQueiroz/Shared-Sessions** | Go | MIT | Sesiones | Connect | Histórico |
| **t1mlange/spotify-group-session** | Parche desktop | GPL-3.0 | Group Session | Sync | Archivado |

Detalle y URLs en [`02-existing-projects.md`](02-existing-projects.md).

**Conclusión:** hay jukebox web y extensiones de Spicetify, pero **ninguna extensión de spotatui con cola compartida**. El nicho está vacío.

### 1.4 Vías no oficiales para Jam

- **`spotify_cli`** (incluido en Spotify Desktop): subcomando `jam` con `status`, `create`, `leave`, `end`, `members`, `kick`, `permissions`. Confirmado en macOS y Windows; Linux sin confirmar. **No tiene `join`**. No es API contractual; puede cambiar sin aviso.
- **GraphQL interno del Web Player** (`socialsession`): existe, pero usar tokens internos viola los Terms (reverse engineering).
- **librespot**: puede reproducir una Jam iniciada desde otro cliente como dispositivo Connect, pero no crearla ni gestionarla (issue [#1576](https://github.com/librespot-org/librespot/issues/1576), abierto desde sep 2025).

**Decisión:** no depender de estas vías en el núcleo. Un adaptador opcional macOS/Windows es posible en P2, nunca como base.

---

## 2. El gap real

La diferencia entre “fiesta de escucha” y “Jam” es una **cola propiedad de la sala**:

1. **Cola compartida:** varios usuarios añaden, quitan, reordenan y (opcionalmente) votan. Hoy la cola es privada del host.
2. **Resolución a playback:** el host convierte esa cola en reproducción. Ya existe el motor de cola nativa (`native_queue`) y el sync; falta conectarlos.
3. **Identidad y permisos:** saber quién añadió qué y quién puede moderar. Hoy solo hay nombres.
4. **Invitado sin Premium:** la Jam oficial permite unirse en persona a usuarios Free; una app de terceros puede dejarles añadir a la cola sin reproducir (modo jukebox). No está en spotatui.

Gaps secundarios: votos, historial, invitaciones por QR, persistencia sin host, reconexión.

### 2.1 Por qué `native_queue` no basta tal cual

La auditoría de código ([`01-spotatui-architecture.md`](01-spotatui-architecture.md)) confirma que `native_queue: Vec<TrackInfo>` es un FIFO de reproducción, no una cola colaborativa:

- No tiene IDs estables, autor, votos ni versión; se direcciona por URI + posición.
- El motor hace `remove(0)` al avanzar y mueve el track a `queue_now`; el item que suena sale de la cola.
- `play_queue_item` drena todos los anteriores; no hay “reproducir sin borrar”.
- No hay `insert_at`; solo push al final.
- Si el host usa un dispositivo Connect externo, `add_track_to_native_queue` desvía a la cola de Spotify (append-only) y `native_queue` queda vacía.
- La cola es multi-fuente, pero la jam v1 debe restringirse a Spotify.

Por eso el diseño necesita una `JamQueue` autoritativa con `current`, `items`, `history` e IDs, y usar `native_queue` solo como **proyección de reproducción** de los próximos items.

### 2.2 Por qué no plugin, MCP o CLI

| Vía | Qué permite | Por qué no basta |
|---|---|---|
| Plugin Lua | Pantalla custom, timers, HTTP, `play_uri`, `add_to_queue` | Sin WebSocket, sin acceso a `native_queue`, sin remove/reorder; `queue()` devuelve el espejo de Spotify, no la cola nativa |
| MCP | 18 tools; `get_queue`, `queue_tracks` (append), `play_now`, `skip_track` | No hay remove/reorder, no hay eventos push, no hay party; el host y el sidecar compiten por el estado |
| CLI | Playback, búsqueda, historial, plugins | Sin cola, sin JSON de playback, sin IPC con la TUI en ejecución |

La vía con techo funcional completo es el fork del core. El plugin queda como prototipo de UI o validación, no como producto.

### 2.3 El detalle de librespot

El fork de librespot que usa spotatui ya contiene `social_connect_v2.proto` y Spirc escucha `social-connect/v2/session_update` con campos de sesión social (`session_id`, `join_session_token`, `join_session_url`, `session_members`, `is_controlling`, `host_active_device_id`). Pero **no hay llamadas create/join/leave** ni resolución de códigos de Jam. Es decir: la base para reproducir una Jam iniciada desde la app oficial existe, pero iniciarla o gestionarla desde spotatui no está demostrado. Refuerza la decisión de no depender de la Jam nativa en el núcleo.

---

## 3. ¿Es viable?

**Sí, con matices.**

### Técnicamente

- Listening Party ya tiene relay, sala, presencia, sync y control. Añadir cola es extender el protocolo y la UI, no construir un reproductor.
- `native_queue: Vec<TrackInfo>` ya es una cola funcional que el motor consume; puede ser la base de la cola autoritativa.
- El relay limita mensajes a 4 KB: hay que trocear snapshots y usar deltas. Es un problema de ingeniería, no un bloqueo.
- Los invitados ya tienen sesión de Spotify y pueden buscar; enviar un URI al host es suficiente.

### Políticamente

- La Jam oficial no tiene API; no se puede prometer interoperabilidad.
- El sync multi-dispositivo se parece a Jam y activa el riesgo III.11 de la Developer Policy (“no replicar una experiencia core”); el webcasting III.4 es un riesgo menor pero real.
- El modelo **host-only (jukebox)** es el más limpio: solo un dispositivo reproduce, los invitados solo controlan la cola. Permite invitados sin Premium y reduce la exposición.
- El modelo **follow-host (sync)** es el que ya existe en spotatui; es más útil para remoto, pero más expuesto. Debe etiquetarse como sala privada y no operarse como servicio público.
- Medidas: sin ánimo de lucro, sin marcas de Spotify, sin retransmisión de audio, Premium obligatorio para quien reproduzca, self-hosting, no usar endpoints reverse-engineered.

**No es asesoría legal.** Antes de una release pública amplia, revisar los Terms y la Policy con alguien cualificado.

### Operativamente

- Relay público con Cloudflare o self-hosted en Rust.
- 8 participantes y 100 items son límites razonables para una fiesta.
- El host único es un punto de fallo; se documenta y se pospone la reconexión a P1.

---

## 4. Recomendación

**Construir `spotatui-jam` como extensión upstream-first de Listening Party:**

1. **Protocolo v2 aditivo:** `queue_snapshot` troceado, `queue_patch`, `queue_command`, `queue_ack`, `queue_resync`, identidad de participante.
2. **Host autoritativo:** `PartyQueue` con IDs, revisión, permisos y límites; `native_queue` derivada.
3. **Relay v2:** reenvía mensajes de cola, asigna IDs, valida dirección y rate limit; Worker + relay Rust.
4. **UI TUI:** pantalla Party con cola, búsqueda, miembros y moderación; keybindings configurables.
5. **Dos modos de playback:** follow-host (MVP, reutiliza sync) y host-only (P1, jukebox limpio).
6. **Sin Jam nativa en el núcleo:** adaptador opcional P2, flag, macOS/Windows, aviso.
7. **Upstream-first:** PR pequeño; fork como fallback.

El plan detallado está en [`../docs/02-arquitectura.md`](../docs/02-arquitectura.md), [`../docs/03-plan-entrega.md`](../docs/03-plan-entrega.md) y [`../docs/08-backlog.md`](../docs/08-backlog.md).

---

## 5. Qué no haría

- **No** intentar “crear una Jam de Spotify” con GraphQL interno o `spotify_cli` como base.
- **No** prometer sync de audio perfecto: es best-effort, seek-based.
- **No** construir un servicio público multi-tenant sin revisar ToS y costes.
- **No** empezar por votos, chat, web o QR; la cola es el núcleo.
- **No** mantener un fork sin intentar upstream primero.

---

## 6. Incertidumbres

| Incertidumbre | Impacto | Cómo resolverla |
|---|---|---|
| ¿Spotify publicará API de Jam? | Cambiaría el producto | Revisar trimestralmente; diseñar adaptador |
| ¿El maintainer aceptará el PR? | Fork permanente | Contacto en Fase 0; decisión en 3 semanas |
| ¿El hook de `native_queue` es viable? | Plan B | Spike PQ-004 |
| ¿El relay de 4 KB es suficiente con chunking? | Diseño | Test de peor caso PQ-014 |
| ¿La sync actual es suficiente para una fiesta? | UX | Prueba manual M7/M8 |
| ¿La política permite sync multi-dispositivo? | Release pública | Revisión legal; host-only por defecto |
| ¿`spotify_cli` funciona en Linux? | Adaptador P2 | No confirmado; no depender |

---

## 7. Fuentes principales

- spotatui: [repo](https://github.com/LargeModGames/spotatui), [v0.48.0](https://github.com/LargeModGames/spotatui/releases/tag/v0.48.0), [Listening Party docs](https://github.com/LargeModGames/spotatui/blob/main/docs/listening-party.md), [PR #684](https://github.com/LargeModGames/spotatui/pull/684), [issue #442](https://github.com/LargeModGames/spotatui/issues/442), [discusión #331](https://github.com/LargeModGames/spotatui/discussions/331).
- Spotify: [Developer Terms](https://developer.spotify.com/terms), [Developer Policy](https://developer.spotify.com/policy), [Web API](https://developer.spotify.com/documentation/web-api/), [Web Playback SDK](https://developer.spotify.com/documentation/web-playback-sdk/), [Quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes).
- Comunidad: [Web API info for Jams](https://community.spotify.com/forum/en/discussion/6235408/web-api-info-for-jams), [Add API support for Jams](https://community.spotify.com/t5/Spotify-for-Developers/Add-Spotify-API-support-for-Jams/td-p/7169789).
- No oficial: [spotify-cli-mcp](https://github.com/mrvnklm/spotify-cli-mcp), [librespot #1576](https://github.com/librespot-org/librespot/issues/1576).
- Proyectos: [OpenDJ](https://github.com/viscoci/OpenDJ), [syncwave](https://github.com/SAKMZ/syncwave), [spicetify-jam](https://github.com/Kyzenkms/spicetify-jam), [listen-together](https://github.com/citrus401/spotify-listen-together-2.0), [spot-along](https://github.com/Sarthakischill/spot-along).

Los informes completos, con metodología y marcas de verificación, están en:

- [`01-spotatui-architecture.md`](01-spotatui-architecture.md)
- [`02-existing-projects.md`](02-existing-projects.md)
- [`03-technical-feasibility.md`](03-technical-feasibility.md)
