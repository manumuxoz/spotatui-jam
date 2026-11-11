# spotatui: arquitectura y análisis de la Listening Party para una *jam* con cola compartida

**Repositorio analizado:** `https://github.com/LargeModGames/spotatui`  
**Commit:** `4c5fc994592c2f7ea70d1e270711632640bd5531` (2026-11-10, `chore(gates): raise the adoption floors [skip ci]`)  
**Versión del paquete:** `0.48.0`  
**Licencia:** MIT (`LICENSE`, copyright 2021 Alexander Keliris / 2025 LargeModGames)  
**MSRV declarada:** Rust `1.90` (`Cargo.toml`, `rust-version = "1.90"`); toolchain usada en la verificación: `rustc 1.94.1`.  
**Alcance:** auditoría de código y de la documentación del repo, más búsqueda de issues/PRs/discussions. No se modificó el repositorio. Copia de trabajo: `/tmp/opencode/research/spotatui-a` (clon `--depth 1`); fork de librespot en `/tmp/opencode/research/librespot-fork`.

> **Conclusión corta:** spotatui 0.48 ya tiene una *Listening Party* real, pero es **sincronización de transporte**, no una *jam* con cola compartida. El protocolo `SyncMessage` solo lleva el track actual, posición, estado play/pause y timestamp; no existe ningún mensaje de cola, alta, baja, reordenación, voto ni atribución. La `native_queue: Vec<TrackInfo>` del host puede ser la **proyección de reproducción** de una cola de jam, pero no es hoy una cola colaborativa autoritativa: es un FIFO que se consume por la cabeza, sin IDs estables, sin autor, sin versión y sin API de inserción.

---

## 0. Resumen ejecutivo

- La Listening Party se introdujo en `v0.37.1` (CHANGELOG) y en 0.48 tiene tres piezas: popup de TUI (`src/tui/handlers/party.rs`, `src/tui/ui/popups.rs`), página de GUI (`gui/src/Party.tsx`) y relay WebSocket en Cloudflare Workers + Durable Objects (`worker-relay/src/index.ts`).
- El host publica cada 2 s un `SyncState` con `track_uri`, `position_ms`, `is_playing`, `timestamp`; el invitado cambia de track con `start_playback_uris(vec![track_uri], None)`, ajusta play/pause y corrige deriva si supera 3 s (`src/infra/network/mod.rs:1852-1932`).
- No hay cola compartida. La página GUI del host muestra **su** cola local; el invitado ve el texto “The host picks the songs”. El popup de TUI no muestra cola alguna. `PartyPayload`/`PartyRoom` no tienen campo de cola (`src/gui/protocol.rs:756-786`).
- El relay impone límites duros: código de 6 caracteres, **4 KB por mensaje**, **8 participantes** (host incluido), 10 creaciones/uniones por IP/hora, 1 host y 2 invitados por IP, cierre por inactividad a los 5 minutos y cierre de la sala si el host se desconecta (`worker-relay/src/index.ts:9-16`, `:283-296`).
- `native_queue` es propiedad de `App` y se persiste en `last_session.yml`, pero el motor de reproducción **saca** la cabeza (`native_queue.remove(0)`) y la convierte en `queue_now` (`src/infra/queue/dispatch.rs:269-288`). No hay IDs de ítem, `added_by`, versión ni historial. Las mutaciones se direccionan por URI + posición (`src/core/app/queue.rs:83-139`).
- MCP tiene 18 herramientas, pero para cola solo ofrece `get_queue` (solo `native_queue`), `queue_tracks` (append), `play_now` y `skip_track`; no hay remove/reorder ni suscripciones/eventos (`src/infra/dj/tools.rs:65-382`, `:1265-1285`).
- Lua puede hacer HTTP(S), temporizadores, almacenamiento JSON y pantallas completas, pero **no** tiene WebSocket, no ve `native_queue` (`queue()` devuelve el espejo de Spotify) y `add_to_queue` va a la cola interna de Spotify, no a la cola nativa (`src/core/plugin_api.rs:809-824`, `src/infra/scripting/api.rs:478`).
- El CLI no tiene subcomando de cola, no habla con la TUI en ejecución y no emite JSON de playback; no sirve como *control plane* de una jam (`docs/cli.md`, `src/cli/cli_app.rs`).
- El fork de librespot contiene el proto `social_connect_v2.proto` y Spirc ya escucha `social-connect/v2/session_update`, incluyendo `session_id`, `join_session_token`, `join_session_url`, `session_members`, `is_controlling` y `host_active_device_id`; pero **no hay llamadas create/join/leave** ni resolución de códigos de Jam (`/tmp/opencode/research/librespot-fork/protocol/proto/social_connect_v2.proto`, `connect/src/spirc.rs:232-234`, `:1983-2034`).
- La vía recomendada es una **cola de jam autoritativa en el host** (nuevo `JamQueue` con IDs, versión, autor y permisos), usando `native_queue` como proyección de reproducción y extendiendo el protocolo/relay con snapshot, intents, acks y parches. La vía “Jam oficial de Spotify” es incierta y frágil; la vía plugin/sidecar no cubre remove/reorder en tiempo real.

---

## 1. Estructura general y modularización

### 1.1 Un solo paquete, dos frontends y un shell aparte

`Cargo.toml` declara **un solo paquete** `spotatui` con:

- `[lib] name = "spotatui"`, `path = "src/lib.rs"`.
- `[[bin]] spotatui` (`src/bin/spotatui.rs`), el binario de consola.
- `[[bin]] spotatui-gui` (`src/bin/spotatui-gui.rs`), solo con la feature `gui`.

No hay `crates/` ni workspace en la raíz. `desktop/` es un crate Tauri independiente, con su propio `Cargo.lock`, que depende del crate raíz por path y usa `run_gui_hosted`; no es miembro del workspace. `worker-relay/` es un subproyecto TypeScript/Cloudflare aparte.

Módulos de `src/`:

| Unidad | Rol | Archivos representativos |
|---|---|---|
| `core/` | Estado central `App`, tick neutro de frontend, vocabulario `Action`, persistencia, tipos de dominio sin rspotify | `src/core/app/`, `src/core/driver/`, `src/core/action/`, `src/core/plugin_api.rs`, `src/core/persisted_playback.rs` |
| `infra/` | Spotify (API interna + streaming librespot), fuentes alternativas, audio, Lua, DJ/MCP, integraciones OS | `src/infra/network/`, `src/infra/spotify/`, `src/infra/player/`, `src/infra/queue/`, `src/infra/scripting/`, `src/infra/mcp/` |
| `tui/` | Frontend de terminal: runner, input, handlers por pantalla, draw functions | `src/tui/runner.rs`, `src/tui/handlers/`, `src/tui/ui/` |
| `gui/` | Frontend web/desktop: servidor loopback, protocolo JSON con revisiones, bridge | `src/gui/server.rs`, `src/gui/protocol.rs`, `src/gui/bridge.rs` |
| `cli/` | Subcomandos clap, playback, historial, plugins, MCP | `src/cli/clap.rs`, `src/cli/cli_app.rs`, `src/cli/handle.rs` |
| `runtime/` | Entrada, bootstrap, pump de `IoEvent`, streaming startup, GUI runtime | `src/runtime/pump.rs`, `src/runtime/bootstrap.rs`, `src/runtime/streaming/` |

Tamaño observado: **353 archivos `.rs`**, **~207.893 líneas** bajo `src/` (`find src -name '*.rs' | wc -l`, `cat` acumulado). El repo está bien separado por capas, pero la Listening Party es un *cross-cutting feature*: toca `core/app`, `infra/network`, `tui`, `gui` y `worker-relay` a la vez.

### 1.2 Features relevantes

Del `Cargo.toml`:

- `default = ["telemetry", "streaming", "audio-viz-cpal", "macos-media", "discord-rpc", "mpris", "self-update", "windows-media", "scripting", "tui"]`.
- `streaming = ["spotify-session", "librespot-playback", "librespot-connect", "queue"]`.
- `spotify-session` habilita login y API interna; `streaming` añade reproducción nativa.
- `queue = []` es una meta-feature usada por las fuentes; `audio-decode-queue` añade rodio.
- `scripting = ["dep:mlua"]`; los plugins Lua están en el build por defecto.
- `mcp-server = ["dj-core", "dep:hmac", "dep:sha2", "dep:hex"]` **no** está en default; los binarios prebuilt sí lo llevan.
- `gui = ["dep:httparse"]`; `all-sources = ["local-files", "subsonic", "internet-radio", "youtube", "qobuz", "soloist"]` tampoco está en default.
- `ai-dj` no está en default y sí hace egress a proveedores externos.

Para una jam conviene un build con `streaming` (host nativo), `queue` y, si se quiere UI web, `gui`; `scripting` no resuelve la cola; `mcp-server` solo si se opta por sidecar.

### 1.3 Convenciones que afectan a la extensión

- `AGENTS.md` impone que `core/` sea neutral de frontend y que `tui/` no sea importado desde `core`/`infra`/`cli` (la pata CI `headless` lo comprueba).
- `src/core/app/AGENTS.md`: `App` es un struct plano de 171 campos; los métodos viven en 37 archivos `impl App`. Un campo sin escritor fuera de `core/app/` debe ser privado; los handlers mutan por métodos o `app.apply(Action::…)`.
- `src/infra/network/AGENTS.md`: todo `IoEvent` nuevo debe clasificarse en `event_bypasses_spotify_auth`, `runs_on_service_lane`, `runs_on_spotify_lane` y `event_is_transport`.
- `src/tui/AGENTS.md`: una pantalla nueva son **seis** sitios: `RouteId` + `ActiveBlock`, handler + `mod` + match, draw fn + `draw_route_content` o `terminal.draw`, `handle_escape`, `content_active_block_for_route`, y fila de ayuda/`default_binding`.
- `src/gui/protocol.rs` usa `DisplayRevisions` por dominio (`DisplayDomain::Party`, `Queue`, etc.); la GUI aplica un canal solo si su `rev` es mayor. Cualquier campo nuevo en un payload debe pasar por `ts-rs` y regenerar `gui/src/bindings/`.

Esto hace que una jam nativa sea un cambio de varias capas, no un módulo aislado.

---

## 2. Listening Party en 0.48 (análisis prioritario)

### 2.1 Máquina de estados y flujo

Estados (`src/infra/network/sync.rs:102-120`): `PartyStatus::{Disconnected, Connecting, Hosting, Joined}`. Rol (`:96-100`): `PartyRole::{Host, Guest}`. Sesión (`:122-131`): `PartySession { role, code, guests: Vec<String>, control_mode, host_name, host_state_at }`.

Flujo de host:

1. `Action::StartParty` → `App::start_party` (`src/core/app/party.rs:37`) comprueba `spotify_connected`; si no, muestra `party_needs_spotify`.
2. `IoEvent::StartParty(ControlMode::HostOnly)` → `Network::start_party` (`src/infra/network/mod.rs:1525`).
3. `sync::connect_to_relay(relay_url, "create", &[("control_mode", mode)])` (`sync.rs:159-177`) abre `wss://…/ws?action=create&control_mode=…`.
4. El relay crea una sala Durable Object por código y responde `RoomCreated { code, control_mode }`.
5. `App` guarda `PartySession` con `code` vacío hasta que llega `RoomCreated`; el popup muestra “generating…”.
6. En cada tick, si `party_status == Hosting` y pasaron ≥2 s, `App` despacha `IoEvent::SyncPlayback` (`src/core/app/tick.rs:126-133`).

Flujo de invitado:

1. `Action::JoinParty { code, name }` → `Network::join_party` (`mod.rs:1591`) exige sesión Spotify.
2. `connect_to_relay(relay_url, "join", &[("code", code), ("name", name)])`.
3. El relay responde `JoinedRoom { host_name, control_mode? }`; el cliente crea `PartySession` Guest con `guests: Vec::new()` y `host_name` vacío hasta el mensaje.
4. `process_party_messages` se drena cada 25 ms desde el pump (`src/runtime/pump.rs:102`).
5. Al recibir `SyncState`, `handle_incoming_sync_state` compara URI, cambia track si hace falta, aplica play/pause y corrige deriva.

El host publica **solo** lo que un invitado puede seguir: exige que `party_yields_to_local_playback()` sea falso y que el snapshot tenga un URI `spotify:track:`/`spotify:episode:` canónico. Si el host reproduce Local/Subsonic/Qobuz/YouTube/radio, o tiene el backend nativo aparcado, manda `Heartbeat` en vez de estado (`mod.rs:1662-1697`). Esto mantiene viva la sala 5 minutos, pero el invitado no recibe nada nuevo.

### 2.2 Protocolo `SyncMessage`

Definido en `src/infra/network/sync.rs:54-94`, serializado como JSON con `#[serde(tag = "type", rename_all = "snake_case")]`:

| Tipo | Campos | Dirección / uso |
|---|---|---|
| `room_created` | `code`, `control_mode` | Relay → host |
| `joined_room` | `host_name`, `control_mode?` | Relay → invitado; `control_mode` opcional por compatibilidad con relays viejos |
| `guest_joined` | `name` | Relay → resto |
| `guest_left` | `name` | Relay → resto |
| `sync_state` | `track_uri`, `position_ms`, `is_playing`, `timestamp` | Host → invitados, cada 2 s |
| `playback_command` | `action: PlaybackAction`, `from: Option<String>` | Invitado → host (solo `shared_control`); el relay rellena `from` |
| `set_control_mode` | `control_mode` | Host → relay; el relay reemite a todos |
| `set_name` | `name` | Definido, pero **el cliente no lo envía**; el relay lo soporta |
| `heartbeat` | — | Host → relay cuando no hay estado seguible |
| `error` | `message` | Relay → cliente |
| `room_closed` | — | Relay → todos |

`PlaybackAction` (`sync.rs:37-52`): `Play`, `Pause`, `NextTrack`, `PrevTrack`, `ForcePrevTrack`, `Seek { position_ms }`, `PlayTrack { uri }`.

**No existe** ningún mensaje de cola: ni `queue_snapshot`, ni `queue_add`, ni `queue_remove`, ni `queue_move`, ni `vote`, ni `participant_list`, ni `ack`, ni `seq`. La única referencia a “queue” en la fiesta es el texto de UI y la cola local que cada cliente pinta por su cuenta.

Detalles del protocolo:

- `timestamp` es `SystemTime::now()` del host en ms (`sync.rs:152-157`). El invitado calcula `transit_ms = now - timestamp`, con tope de 5 s para no saltar por desfase de reloj (`mod.rs:1874-1885`). No hay sincronización NTP ni estimación RTT; se compensa la posición si `is_playing`.
- Deriva: si `abs_diff(current_progress, compensated_position) > 3000` y no hubo cambio de track, se hace `IoEvent::Seek` (`mod.rs:1924-1931`).
- Cambio de track: `app.start_playback_uris(vec![track_uri], None)` (`:1907-1911`). Es un `StartPlayback` de una lista de un solo URI; no usa contexto ni cola.
- Play/pause tras cambio: si el host está pausado, se fuerza `PausePlayback` después del start, porque `start_playback_uris` puede arrancar reproduciendo (`:1913-1922`).
- El invitado ignora `SyncState` si `party_yields_to_local_playback()` o si no es Guest (`:1864-1871`).

### 2.3 Relay: Cloudflare Worker + Durable Object

`worker-relay/src/index.ts`:

- `PartyRoom extends DurableObject` (`:71`), una instancia por `room_code` (`idFromName(roomCode)` en el entrypoint, `:377-380`).
- Estado en memoria: `sessions: Map<WebSocket, Session>`, `hostWs`, `roomCode`, `hostName = "Host"`, `roomControlMode = "host_only"`, `ipConnections` (`:72-77`).
- `Session { id, isHost, name, ip }` (`:64-69`); el `id` es un UUID, pero **no se usa en el protocolo** ni se envía al cliente.
- Entrada: `GET /ws?action=create|join`, `Upgrade: websocket` obligatorio; `create` genera código y deriva a `PartyRoom` con `is_host=1`; `join` valida código de 6 y nombre (`:332-397`).
- Rate limiter por IP en otro Durable Object (`RateLimiter`, `:45-62`): `MAX_CREATES = 10`, `MAX_JOIN_ATTEMPTS = 10` por ventana de 1 hora.
- Límites del `PartyRoom`: `MAX_PARTICIPANTS = 8` (`:15`), `MAX_MESSAGE_SIZE = 4*1024` (`:14`), `INACTIVITY_MS = 5*60*1000` (`:16`), `CODE_LENGTH = 6` (`:13`), `CODE_CHARS = "A-Z0-9"` (`:9`).
- Reglas de admisión: un host por sala; sin host, un guest recibe `room not found`; host duplicado rechazado; 1 host/IP; 2 guests/IP; sala llena a 8 (`:110-138`).
- `sendJson`, `broadcastExcept`, `broadcastAll` **descartan silenciosamente** cualquier mensaje serializado cuyo `raw.length > 4096` (`:191-199`, `:258-266`, `:304-312`). No hay error al emisor.
- `handleMessage` (`:201-256`) solo entiende `set_name`, `set_control_mode`, `sync_state`, `playback_command`, `heartbeat`; cualquier otro tipo cae en `default: break`.
- `set_control_mode`: solo el host; el relay actualiza `roomControlMode` y hace `broadcastAll`.
- `sync_state`: solo el host; `broadcastExcept(ws, msg)`.
- `playback_command`: solo un guest; si `roomControlMode === "shared_control"` y hay host, reenvía al host con `from: session.name`; si no, se descarta.
- `handleClose` (`:268-302`): si se va el host, `room_closed` a todos, cierra cada socket y limpia `sessions`; si se va un guest, `guest_left`.
- `alarm` (`:314-329`): a los 5 minutos sin mensajes, `room_closed` y cierre. El host rearma el alarm con cada mensaje/heartbeat.
- No hay almacenamiento de la sala ni de la cola en `ctx.storage`; el único uso de storage es el rate limiter (`:47-60`). `wrangler.toml` declara las migraciones SQLite de las clases, pero el `PartyRoom` no persiste sesiones.

**Implicación:** el relay es un *fan-out* de transporte, no una autoridad de sala. No tiene lista de participantes completa para late join, no tiene cola, no tiene historial y no sobrevive a la caída del host.

### 2.4 Límites duros y su efecto en una jam

| Límite | Valor | Fuente | Efecto |
|---|---:|---|---|
| Tamaño de mensaje | 4 KB | `worker-relay/src/index.ts:14`, `:194`, `:207`, `:260`, `:306` | Un snapshot de cola con ~8-12 `TrackInfo` puede superarlo; se descarta sin aviso. No cabe una cola de 50 temas. |
| Participantes | 8 (host incluido) | `:15`, `:122` | Jam pequeña; no hay sala de 20. |
| Código | 6 chars A-Z0-9 | `:9`, `:13`, `:18-26` | ~2.18e9 combinaciones; es un bearer token, no una contraseña. |
| Rate limit | 10 create + 10 join/IP/hora | `:11-12`, `:45-57` | Suficiente para uso personal; problemático en NAT compartido. |
| IP por sala | 1 host, 2 guests | `:128-138` | Tres personas tras el mismo NAT no pueden entrar. |
| Inactividad | 5 min | `:16`, `:314-316` | El heartbeat del host lo evita; si el host se cuelga, la sala muere. |
| Host disconnect | cierra sala | `:283-296` | No hay migración de host ni reconexión. |
| Mensajes desconocidos | ignorados | `:216-255` | Cualquier extensión de cola exige tocar el relay; no se puede “pasar” un tipo nuevo. |
| Persistencia | ninguna | `:72-77` | Un late join no puede recibir estado anterior; el DO puede perder las sesiones si se desaloja. |
| Nombre visible | TUI/GUI 32; relay 64 | `src/tui/handlers/party.rs:8`; `worker-relay/src/index.ts:28-31` | El relay acepta 64 chars y el cliente recorta a 32; no hay identidad estable, solo display name. |

### 2.5 Modos de control y permisos

`ControlMode` (`sync.rs:10-16`): `HostOnly` (default) y `SharedControl`.

- El host arranca siempre en `HostOnly`; `Action::TogglePartyControlMode` alterna y despacha `IoEvent::SetPartyControlMode` (`src/core/app/party.rs:60-71`).
- El relay solo acepta `set_control_mode` del host (`worker-relay/src/index.ts:232-237`).
- El host revalida en `handle_incoming_playback_command`: rol Host, `SharedControl`, y que no haya owner local (`src/infra/network/mod.rs:1934-1944`). Es defensa en profundidad correcta.
- Un guest en `SharedControl` relaya su transporte si `host_state_at` es reciente (<5 s) y no hay owner local (`src/core/app/party.rs:92-116`). Esto cubre teclas, GUI y teclas multimedia del OS (`relay_party_play_key`, `relay_party_toggle`).
- `PlaybackCommand.from` lo rellena el relay, pero el host lo **ignora**: en `process_party_messages` el brazo es `SyncMessage::PlaybackCommand { action, .. }` (`mod.rs:1811`). No hay auditoría por invitado.
- `SharedControl` no es “control de cola”: permite `PlayTrack { uri }` con cualquier URI `spotify:track:`/`episode:` y `Seek` a cualquier posición. Es control total de transporte, no un permiso de “añadir a la cola”.
- No hay roles intermedios (co-host, DJ, solo-cola), ni `kick`, ni `ban`, ni transferencia de host, ni aprobación de temas.
- No hay identidad Spotify en la sala: el relay solo tiene un `name` de texto; el cliente no envía `SetName` (`sync.rs:86-88`; no hay llamada a `SetName` en `src/`), y el relay fija el nombre del host a `"Host"` (`worker-relay/src/index.ts:106`). `host_name` que ve el invitado es siempre `"Host"` salvo que alguien implemente el envío.
- El código de sala es la única credencial. Cualquiera con el código y una sesión Spotify puede entrar; no se verifica que el guest sea el dueño de una cuenta Spotify concreta.

**Inconsistencia documental:** `docs/listening-party.md` dice en la tabla de modos que “spotatui guests do not send any yet”, pero el código y los tests de `src/core/app/party.rs` sí relayan transporte desde #954. La documentación está desactualizada respecto a 0.48.

### 2.6 UI: TUI y GUI

**TUI**

- Ruta `RouteId::Party` / `ActiveBlock::Party` (`src/core/app/route.rs:78`, `:158`).
- `handlers/party.rs`: menú disconnected (1/h/Enter host, 2/j/J join), entrada de código de 6 y nombre máx. 32, hosting (`l` leave, `c` toggle), joined (`l` leave). `Esc` cierra el popup sin terminar la fiesta.
- `draw_party` (`src/tui/ui/popups.rs:1718`): popup centrado de **50x16** (`popup_width = 50`, `popup_height = 16`), sin `ListState`, sin scroll y sin cola. Muestra código, modo, lista de guests y ayudas.
- El playbar muestra `Party: N listeners` o `Party: following <host>`; el estado se actualiza por `DisplayDomain::Party`.

**GUI**

- `gui/src/Party.tsx`: página de inicio (host/join) y sala.
- La sala muestra código, listeners, cover/track actual y una sección “Up next”.
- Si `room.host`, pinta `queue.native` y `queue.spotify` **de su propia instancia**, con filas marcadas `only you` / `guests follow`; si no es host, muestra `host_picks_songs` (“The host picks the songs”) y no pinta cola (`Party.tsx`, rama `!room.host`).
- `PartyPayload` (`src/gui/protocol.rs:756-774`): `phase`, `room`, `available`. `PartyRoom` (`:776-786`): `host`, `code`, `host_name`, `guests`, `shared_control`. **No hay cola ni participantes con ID.**
- El canal `Party` se empuja por revisión (`ServerMessage::Party { rev, payload }`) y se difunde cuando `App::set_party_status`/`set_party_session`/`party_session_mut` hacen `bump(DisplayDomain::Party)` (`src/core/app/party.rs:17-32`).
- El `QueuePayload` existente (`src/gui/protocol.rs:449-453`) lleva `spotify`, `native`, `now`; es la cola local, no la del host remoto.

### 2.7 Late join, roster y reconexión

- `JoinedRoom` solo lleva `host_name` y `control_mode`; **no** lleva la lista de miembros ni la cola (`sync.rs:61-66`).
- El relay, al admitir un guest, le manda `joined_room` y hace `guest_joined` solo a los demás (`worker-relay/src/index.ts:165-172`). El nuevo guest no recibe los guests que ya estaban; su `PartySession.guests` empieza vacío y solo aprende los que lleguen después.
- El host sí tiene la lista completa (`guests` se llena con cada `guest_joined` y se vacía con `guest_left`).
- No hay `snapshot` inicial: el primer `SyncState` llega en el siguiente tick de 2 s. Si el host está pausado o reproduce una fuente no seguible, el guest no recibe track hasta que cambie.
- No hay reconexión: si el socket se cae, `start_party_reader` emite `Error`/`RoomClosed`, `leave_party` limpia la sesión. Para volver a entrar hay que teclear código y nombre otra vez.
- Si el host se cae, el relay cierra la sala y no hay host migration.

### 2.8 Tests de la fiesta

En la suite slim (`telemetry,tui`) hay **31 tests** con “party” en el nombre, entre ellos:

- `src/core/app/party.rs`: guest shared-control relaya transporte; host-only no; host nunca relaya; sin host state no relaya.
- `src/infra/network/mod.rs`: `start_party_without_a_session_opens_no_relay`, `join_party_without_a_session_opens_no_relay`, `process_party_messages_closes_a_party_that_outlived_its_session`, `a_host_only_party_ignores_a_guest_command`, `an_oversized_party_seek_is_dropped`.
- `src/tui/handlers/party.rs`: entrada de código/nombre/backspace/esc.
- `src/gui/protocol.rs`: `no_party_pushes_a_disconnected_party_that_says_spotify_is_missing`, `a_hosted_party_pushes_its_code_guests_and_control_mode`.
- `src/infra/network/sync.rs`: parseo de `joined_room` con y sin `control_mode`.

No hay tests del Worker TypeScript (`worker-relay/package.json` solo tiene `dev` y `deploy`), ni un test de integración real WebSocket contra el relay. La cobertura de la fiesta es unitaria y de protocolo, no end-to-end.

---

## 3. Modelo de datos de la cola

### 3.1 Dos colas distintas

| Cola | Tipo / campo | Origen | Autoridad | Mutaciones |
|---|---|---|---|---|
| Espejo de Spotify | `App.queue: Option<QueueState>` (`src/core/app/mod.rs:245`; `src/core/app/queue.rs:6-9`) | `GET /me/player/queue` respondido por `infra/spotify/player.rs::queue` desde el `Cluster` del `Observer` | Spotify (servidor); la app solo lee | Solo `IoEvent::GetQueue`; no hay remove/move |
| Cola nativa | `App.native_queue: Vec<TrackInfo>` (`src/core/app/mod.rs:249`) | App; `add_track_to_native_queue` | La app (host local) | push, remove por URI+posición, move, clear, play (drena anteriores) |

`QueueState` es un espejo de solo lectura: `currently_playing` + `queue`. Se rellena en `Network::get_queue` (`src/infra/network/playback.rs:3021-3055`) y se expone a Lua como `QueueSnapshot` (`src/core/plugin_api.rs:446-451`, `:809-824`). El comentario de `App` lo dice explícitamente: “a read-only mirror of Spotify's Web-API queue”.

`native_queue` es la cola propia, multi-fuente. El comentario de `App` la describe como FIFO propiedad de la app, con tracks de cualquier fuente.

### 3.2 Operaciones de la cola nativa

`src/core/app/queue.rs`:

- **Añadir**: `App::add_track_to_native_queue(track: TrackInfo)` (`:32-79`).
  - Rechaza sin URI, `radio:`, `applemusic:`; rechaza Apple Music si Music owns; rechaza Soloist como target.
  - Si el track es Spotify y `spotify_external_device_active()`, **no** lo mete en `native_queue`: despacha `IoEvent::AddItemToQueue` (cola interna de Spotify, append).
  - En caso normal hace `native_queue.push(track)`; no hay inserción en posición.
  - Si hay streaming nativo activo y la cola no owns playback, dispara `GetQueue` para refrescar el espejo.
- **Saltar a un ítem**: `play_queue_item(uri, position)` (`:100-120`). Busca por posición/URI, **drena todos los ítems anteriores** (`native_queue.drain(..skip)`), suspende el contexto activo si hace falta y despacha `AdvanceNativeQueue`. Es destructivo: no es “reproducir sin borrar”.
- **Quitar**: `remove_queue_item(uri, position)` (`:123-127`). `queue_position_for` comprueba primero la posición y luego cae al primer URI igual (`:83-96`); con duplicados, la posición es el desempate.
- **Mover**: `move_queue_item(uri, from, to)` (`:130-139`). Índice absoluto; fuera de rango no-op.
- **Vaciar**: `clear_native_queue()` (`:142-144`).
- No hay `insert_at`, `replace`, `dedupe`, `set_metadata`, `lock`, `vote` ni `added_by`.

El motor `infra/queue/dispatch.rs`:

- `route_queue_event(app, event)` (`:39`) se ejecuta **antes** de los routers por fuente en el pump.
- `advance_native_queue` (`:269-288`): si la cola está vacía, `resume_or_finish(QueueEnd::Drained)`; si no, **`native_queue.remove(0)`** y `try_play_queued`; si el ítem no se puede reproducir, sigue con el siguiente.
- `try_play_queued` (`:293+`) enruta por esquema: `file:`, `subsonic:`, `qobuz:`, `youtube:`, `spotify:`.
- `QueueNowPlaying` (`src/infra/queue/mod.rs:566-575`): `Decoded(DecodedQueuePlayback)` o `Spotify { track }`; es lo que suena desde la cola, separado de `native_queue`.
- `queue_now` se publica al reproducir y se limpia al terminar; `handle_native_spotify_track_end` (`src/core/app/queue.rs:281-304`) decide si la cola toma el relevo de un contexto Spotify, suspende el contexto y despacha `AdvanceNativeQueue`.

### 3.3 Persistencia

`App::current_persisted_session` (`src/core/app/persistence.rs:195-211`) guarda `PersistedSession { playback, queue }` en `last_session.yml` (`src/core/persisted_playback.rs:48-53`). Si hay un track sonando desde la cola, lo **prepende** a la copia de `native_queue` (`queue.insert(0, track)`) porque `advance_native_queue` ya lo había sacado. Esto demuestra que el modelo real es “`queue_now` + `native_queue`”, no un vector con cursor.

### 3.4 Cola nativa vs “Spotify queue” real

- La API pública de Spotify (`POST /me/player/queue?uri=…`) es **append-only** y `GET /me/player/queue` es de lectura; no hay endpoint público para borrar, insertar o reordenar. spotatui, de hecho, **no** llama a `api.spotify.com`: `infra/spotify/router.rs` responde a las rutas con la API interna de librespot, y `infra/network/requests.rs:339` es el embudo.
- `infra/spotify/player.rs::queue` (`:426-440`) lee `cluster.player_state.next_tracks`, filtra `spotify:track:`/`spotify:episode:` y **corta a `QUEUE_LENGTH = 20`** (`:33`, `:434`). Es decir, el espejo remoto visible tiene como máximo 20 próximos temas.
- `infra/spotify/player.rs::add_to_queue` (`:645-660`) envía un comando `add_to_queue` al device target; no implementa `set_queue`, `remove_from_queue`, `reorder` ni `insert`. Si la API interna soporta algo de eso, este código no lo usa; **no verificado**.
- `StartPlayback` con `uris` (`play_command`, `:526-555`) construye un contexto `pages: [{ tracks: [...] }]`; eso **reemplaza** el contexto de reproducción, no añade a la cola.
- Consecuencia: si el host está en un device Connect externo (teléfono, altavoz, Spotify app), la cola colaborativa no puede ser la `native_queue`; solo se puede hacer append a la cola de Spotify y leer 20. Para una jam con cola propia, el host debe reproducir en el device nativo de spotatui (o en una fuente decodificada), no en un device externo.

---

## 4. Motor de reproducción

### 4.1 Modos

1. **Streaming nativo librespot** (`feature = "streaming"`): `StreamingPlayer` (`src/infra/player/streaming.rs:647`) envuelve `Spirc` y `Player` de librespot; registra spotatui como device Connect. Métodos: `play_uri` (`:953`), `load` (`:987`), `play_track` (`:992`), `pause` (`:1008`), `play` (`:1018`), `next` (`:1057`), `prev` (`:1064`), `seek` (`:1071`), `set_shuffle` (`:1078`), `set_repeat` (`:1084`), `set_volume` (`:1125`), `get_state` (`:1137`), `transfer` (`:1159`), `shutdown` (`:1194`).
2. **Connect remoto**: `PlaybackBackend::Connect` (`src/infra/player/backend.rs:24-34`); la selección la hace `select_native(is_native_active, player_present)` (`:44`). Los comandos van por `Network` → `infra/spotify/player.rs` → `SpClient::send_player_command`.
3. **Fuentes decodificadas**: Local, Subsonic, Qobuz, YouTube, radio. Todas usan un único `LocalPlayer` rodio (`src/infra/audio/player.rs`); Subsonic/YouTube descargan a tempfile, Qobuz descifra a FLAC temporal, radio streamea. El estado por fuente vive en `App.local_playback`, `subsonic_playback`, `qobuz_playback`, `youtube_playback`, `radio_playback`.
4. **Apple Music** (macOS, opt-in): remote de Music.app; no comparte cola.

`App::playback_owner()` pliega los predicados en un `PlaybackOwner` y los transports terminan en `dispatch_spotify_fallback` (ver `src/core/app/AGENTS.md`). El orden de propiedad es: Apple Music → cola nativa → fuente decodificada → streaming nativo.

### 4.2 Control y estado

- **Nativo**: `spawn_player_event_handler` (`src/infra/player/events.rs:374`) consume `librespot_playback::player::PlayerEventChannel` y actualiza `App` directamente, sin Web API. Maneja `Playing`, `Paused`, `EndOfTrack`, `Seeked`, `TrackChanged`, `PositionChanged`, `VolumeChanged`, `SessionDisconnected`, `Unavailable`, `Loading`, `Preloading`, `Stopped`. `EndOfTrack` es donde se decide la continuidad y el takeover de la cola (`events.rs:992-1055`).
- **Remoto**: `Observer` (`src/infra/spotify/player.rs:35-123`) se suscribe al dealer `hm://connect-state/v1/cluster` y guarda un `Cluster`; `GetCurrentPlayback`/`playback_state` lo convierten a `CurrentPlaybackContext`. `current_playback_context` es el estado remoto/espejo.
- **Decodificado**: el sink se consulta en el tick; `plan::decoded_advance` decide `Dispatch`, `Replay`, `SuspendToQueue` o `Teardown` (`src/core/driver/mod.rs:650-714`).
- Campos de estado relevantes: `current_playback_context` (`mod.rs:236`), `song_progress_ms` (`:340`), `native_track_info` (`:430`), `is_streaming_active` (`:432`), `native_is_playing` (`:441`), `native_queue` (`:249`), `queue_now` (`:260`), `queue_slot_desired_playing` (`:275`).
- Integraciones OS: MPRIS, macOS Now Playing, Windows SMTC y Discord RPC leen un `PlaybackSnapshot` único de `infra/media_metadata.rs`; no leen `current_playback_context` directamente (regla de `direct_playback_context_reads`).

Para una jam, el punto importante es que el host puede ser autoridad de reproducción **solo si el device que suena es el de spotatui**. Si suena un device Connect externo, la autoridad es Spotify y la app solo ve un espejo de 20 temas.

---

## 5. Autenticación con Spotify

- No hay app de desarrollador propia, ni PKCE, ni redirect URI local, ni client secret. El login es **Device Authorization Grant** de librespot:
  - `SPOTIFY_PLAYER_CLIENT_ID = "65b708073fc0480ea92a077233ca87bd"` (el client id de escritorio de Spotify / `KEYMASTER_CLIENT_ID` de librespot) (`src/infra/spotify/credentials.rs:22`).
  - `STREAMING_SCOPES` (`:11-18`): `streaming`, `user-read-playback-state`, `user-modify-playback-state`, `user-read-currently-playing`, `user-library-read`, `user-read-private`.
  - `DeviceAuthClientBuilder::new(client_id, scopes)` y `request_device_code_async` (`:52-61`); el usuario aprueba en la página de pairing; `poll_for_token_async` devuelve un access token; se construye `Credentials::with_access_token` (`:43-49`).
  - El token se cachea en el `Cache` de librespot (`streaming_cache/credentials.json`), bajo el cache dir de spotatui; `streaming_credentials_are_cached` (`:63-73`) comprueba `Cache::credentials()`.
  - `SpotifySession` (`src/infra/spotify/session.rs:110`) conecta desde las credenciales cacheadas; exige Premium (`Unavailable::NotPremium`).
- La app **no** usa el token para `api.spotify.com`. `infra/spotify/router.rs` implementa las rutas con la API interna (`SpClient`, Pathfinder, colección, playlists, player commands) y `infra/network/requests.rs::spotify_api_request_json_for` es el embudo. `AGENTS.md` lo dice: “There is no client id and no token… Never send the internal login's token to `api.spotify.com`.”
- Para una jam esto importa: no hay un token OAuth público con scopes de Jam que se pueda reutilizar; cualquier integración con la Jam oficial tendría que hablar la API interna/social-connect, no la Web API pública.
- El login se puede borrar con `--reconfigure-auth` / `forget_spotify_login`; el cache se limpia (`clear_cached_streaming_credentials`).

---

## 6. Plugins Lua

### 6.1 API y capacidades

Feature `scripting` (default-on), `mlua 0.12` con `lua54`, `vendored`, `serde`. `ScriptEngine::new` crea `Lua::new()` (`src/infra/scripting/engine.rs:90`), es decir, biblioteca estándar completa; los plugins **no están sandboxeados**.

Eventos (`docs/scripting.md`): `start`, `quit`, `track_change`, `playback_state_change`, `seek`, `volume_change`, `queue_change`, `shuffle_change`, `repeat_change`, `device_change`, `search_results`, `route_change`.

Lecturas: `playback()`, `current_track()`, `devices()`, `playlists()`, `queue()`, `search_results()`, `current_route()`, `config()`. `queue()` devuelve `QueueSnapshot` construido desde `app.queue` (el espejo de Spotify), **no** desde `native_queue` (`src/core/plugin_api.rs:809-824`).

Acciones: `play`, `pause`, `next`, `previous`, `seek`, `set_volume`, `shuffle`, `search`, `notify`, `set_repeat`, `cycle_repeat`, `transfer_playback`, `play_uri`, `play_context`, `add_to_queue`, `create_playlist`, `playlist_add_track`, `playlist_remove_track`, `follow_playlist`, `unfollow_playlist`, `toggle_save_track`, `save_album`, `unsave_album`, `save_show`, `unsave_show`, `follow_artist`, `unfollow_artist`.

`add_to_queue` se instala como `Action::AddToQueue` (`src/infra/scripting/api.rs:478`), que en `apply.rs:76` llama a `App::add_to_spotify_queue`, no a `add_track_to_native_queue`. `play_uri` solo acepta `spotify:track:`/`episode:` o URIs de contexto Spotify (`api.rs:437-456`).

UI: `popup`, `set_playbar`, `set_theme`, `register_screen`/`set_screen`/`show_screen` con widgets `paragraph`, `list`, `gauge`, `cover_art`, `row`, `column`; profundidad máxima 8, 256 widgets; `on_key` por pantalla. Persistencia: `storage_get/set/remove/keys` en `plugin-data/<plugin>.json`; flush cada ~3 s y al salir. HTTP: `http_get`, `http_post` (solo `http://`/`https://`, timeout 30 s, callbacks en el tick). Temporizadores: `set_timeout`, `set_interval`, `cancel_timer`, resolución limitada por el tick (≤999 ms). Comandos: `register_command`, enlazables por `plugin_commands` en `config.yml`.

Límite de ejecución: `PLUGIN_TIME_LIMIT = 1 s` por entrada (`src/infra/scripting/shared.rs:75`), con hook cada 100k instrucciones. No hay hilo propio ni llamadas bloqueantes seguras.

### 6.2 ¿Puede un plugin implementar una jam?

**No, no con la API actual, para una cola colaborativa real.**

- No hay WebSocket ni socket API; solo HTTP request/response. Se podría hacer *long polling* contra un servidor propio, pero no hay push y el tick limita la latencia.
- No puede mutar `native_queue`: no hay `remove`, `move`, `insert`, `clear` expuestos; `add_to_queue` va a la cola interna de Spotify; `queue()` no ve la cola nativa.
- No puede leer el estado de la fiesta ni los participantes; `navigate("party")` solo abre la pantalla, no da API de sala.
- Puede persistir votos y pintar una pantalla custom; podría ser un **prototipo de “cola de peticiones”** donde el host aplica manualmente los cambios, pero no una jam automática.
- `os.execute`/`io` permiten lanzar un proceso externo, pero sería un hack bloqueante, fuera del modelo async y con el límite de 1 s; no es una base de producto.

**Veredicto:** plugin = esfuerzo bajo, riesgo bajo, pero techo funcional muy por debajo de una jam. Útil para UI de votos o integración con un servidor externo que solo añade, no para remove/reorder.

---

## 7. Servidor MCP

### 7.1 Transporte y arquitectura

- `spotatui mcp` es un servidor **stdio** JSON-RPC por líneas (`src/infra/mcp/relay.rs:44`), pensado para que lo arranque un agente (Claude Code, Codex, etc.).
- La TUI en ejecución escucha en `127.0.0.1:<puerto aleatorio>` y publica `mcp.json` con `{ port, token, pid }` en el config dir, permisos `0600`, directorio `0700` (`src/infra/mcp/control.rs:57-66`, `:146-160`).
- El relay hace handshake: nonce + HMAC con el token, luego token; el listener solo acepta loopback (`control.rs:1-30`). Es opt-in con `behavior.mcp_enabled: true`.
- Protocolo MCP revisión `2026-07-28`, con compatibilidad para `initialize` de clientes legacy (`src/infra/mcp/protocol.rs`).
- Un proceso externo puede hablar MCP contra `spotatui mcp` (stdio) o, leyendo `mcp.json`, contra el socket loopback directamente; lo segundo es privado/no documentado y sigue requiriendo el token.

### 7.2 Herramientas (18)

`src/infra/dj/tools.rs:65-382`:

`get_listening_history`, `get_now_playing`, `get_queue`, `search_tracks`, `queue_tracks`, `play_now`, `skip_track`, `set_dj_vibe`, `list_playlists`, `get_album`, `get_artist`, `list_devices`, `transfer_playback`, `create_playlist`, `add_to_playlist`, `remove_from_playlist`, `like_tracks`, `follow_artist`.

Semántica de cola:

- `get_queue` (`tools.rs:1265-1285`) devuelve **solo `native_queue`**; si está vacía, `{"tracks":[]}`. No devuelve `queue_now` ni el espejo de Spotify.
- `queue_tracks` (`:129-164`) acepta URIs o `title`+`artist`, resuelve y aplica `Action::QueueTracks` → `extend_native_queue_from_dj` → `add_track_to_native_queue` (`src/core/app/dj.rs:40-67`). Acepta `spotify:track:` y también `file:`, `subsonic:`, `youtube:`, `qobuz:track:`; para Spotify en device externo cae al append de la cola interna.
- `play_now` y `skip_track` son transporte; no tocan la cola.
- No hay `remove_from_queue`, `move_queue_item`, `clear_queue`, `vote`, `queue_snapshot` con versión, ni suscripción a eventos. `remove_from_playlist` es de playlists, no de la cola.

### 7.3 ¿Suficiente como *control plane* de una jam?

**No.** Sirve para un DJ/agente único que añade temas y salta, no para una jam multi-usuario:

- No puede quitar ni reordenar; una cola colaborativa sin remove/reorder no es viable.
- No hay push: un sidecar tendría que hacer polling de `get_now_playing`/`get_queue`; pierde eventos y compite con el pump.
- No hay identidad de invitado ni autoría; `queue_tracks` no lleva `added_by`.
- No hay API de sala/party: el MCP no sabe si hay una Listening Party.
- El server controla **una** instancia local; no hay multiplexación de participantes.
- Requiere `mcp-server` (no default) y `mcp_enabled`; no es un canal disponible en todos los builds.

Como *transporte* para un sidecar que mantenga su propia cola, seguiría faltando remove/reorder y un mecanismo atómico de aplicación. Se podría hacer “clear + replay” solo si existiera clear/append atómico, y no existe.

---

## 8. CLI

`docs/cli.md` y `src/cli/clap.rs`:

- `spotatui` sin subcomando abre la TUI.
- `playback` (alias `pb`): `--toggle`, `--status`, `--next`, `--previous`, `--seek`, `--volume`, `--shuffle`, `--repeat`, `--transfer`, `--like/--dislike`, `--share-track/--share-album`, `--device`, `--format`.
- `play` (alias `p`): `--uri` o `--name` + tipo (`--track/--album/--artist/--playlist/--show`), `--queue`, `--random`.
- `list` (alias `l`): `--devices`, `--playlists`, `--liked`, `--playlist-tracks`, `--limit`.
- `search` (alias `s`): `--tracks/--albums/--artists/--playlists/--shows`, `--limit`.
- `sync`: sincronización de playlists entre fuentes (no playback).
- `history recap`: HTML.
- `mcp`: servidor stdio y `mcp status [--json]`.
- `plugin`: add/list/remove/update/new.
- `update`: self-update.

Salida:

- `--format` con placeholders `%a %b %p %t %h %u %f %s %v %d %r`; es texto, no JSON.
- `mcp status --json` es el único JSON de estado; `history recap` es HTML.
- No hay `--json` para `playback`, `list`, `search`.

Daemon/headless:

- No hay modo daemon ni servidor de control. `spotatui` con subcomando construye su propio `Network`/`App` (`CliApp` en `src/cli/cli_app.rs`) y ejecuta la operación; no se conecta a la TUI en ejecución.
- El build headless (`--no-default-features --features telemetry`) compila CLI sin `tui`, pero no ofrece playback nativo ni control de la cola de una TUI.
- `playback` controla el device Connect activo mediante la API interna; no la `native_queue` de otra instancia.

Para una jam, el CLI no es un *control plane*: no hay subcomando de cola, no hay IPC con el reproductor y no hay salida estructurada de playback.

---

## 9. Bucle de eventos y UI

### 9.1 TUI

Flujo (`AGENTS.md`, `src/tui/runner.rs`):

```text
crossterm thread → runner::start_ui (draw frame, then read one event)
  → runner::dispatch_key
  → handlers::handle_app (plugin popup → help filter → global keybindings)
  → handle_block_events (per-screen handler)
  → app.dispatch(IoEvent)
      → runtime/pump.rs::start_tokio
      → source routers → infra/network (Spotify)
      → muta App; la UI redibuja en el siguiente frame
```

- `App` vive en `Arc<Mutex<App>>`; los handlers de red lo bloquean y mutan; la UI dibuja con `&App`.
- `Driver::tick` (`src/core/driver/mod.rs`) corre en el tick: playback tick, auto-advance de cola/fuentes, presencia OS, lyrics/cover art, persistencia `last_session.yml`.
- La fiesta se integra en el tick: `App::update_on_tick` dispara `SyncPlayback` cada 2 s si Hosting (`src/core/app/tick.rs:126-133`).
- El pump drena mensajes de fiesta cada 25 ms (`src/runtime/pump.rs:102`), en el mismo `tokio::select!` que los `IoEvent`.

### 9.2 GUI

- `src/gui/server.rs` sirve la página loopback con comprobaciones Host/Origin/launch-code; `src/gui/bridge.rs` conecta el socket con el tick; `src/gui/protocol.rs` define los mensajes con `rev`.
- Cada dominio (`DisplayDomain::Party`, `Queue`, `Playback`, …) tiene un contador; cuando `App` muta, `display_revisions.bump(domain)` y el servidor empuja el payload si cambió.
- La GUI aplica un mensaje solo si su `rev` es mayor para ese canal (`src/gui/protocol.rs:1-4`). `PartyPayload` ya está en ese esquema.
- Para una vista de cola de jam: añadir el campo al payload de `Party` (o un canal `JamQueue` propio) y regenerar `gui/src/bindings/` con `ts-rs`; `gui/src/Party.tsx` ya tiene la estructura visual de “Up next”.

### 9.3 Cómo integrar una vista/panel nuevo

Regla de `src/tui/AGENTS.md` (seis sitios):

1. `RouteId` + `ActiveBlock` en `src/core/app/route.rs`.
2. `src/tui/handlers/<screen>.rs` con `pub fn handler(key: Key, app: &mut App)`, `mod <screen>;` y match en `handlers/mod.rs`.
3. Draw fn en `src/tui/ui/<screen>.rs`, en `ui::draw_route_content` o en el match de `terminal.draw` del runner; no-op explícito en el otro.
4. `handlers::handle_escape`.
5. `common_key_events::content_active_block_for_route` si se llega con flecha derecha.
6. Fila en `help_entries` (`src/tui/keymap.rs`) + `default_binding` + `sample_actions()` si hay `Action` nuevo.

Para una jam, lo más barato es **extender el popup existente** (`draw_party`) con una lista de cola y teclas, no crear una pantalla nueva; si se quiere una vista grande, usar una ruta nueva y el checklist anterior.

Actualizaciones en tiempo real desde red: el handler de red debe mutar `App` bajo lock y hacer `bump(DisplayDomain::Party)` (o el dominio nuevo); la TUI redibuja en el siguiente frame y la GUI empuja el diff. No hace falta un canal UI aparte.

---

## 10. Configuración y estado persistido

| Archivo | Owner | Contenido |
|---|---|---|
| `config.yml` (config dir) | `core/user_config.rs` | settings, keys, theme; `behavior.relay_server_url`, `behavior.mcp_enabled`, `behavior.enable_streaming`, `behavior.streaming_device_name`, `behavior.playback_engine`, `behavior.sync_token` |
| `state.yml` (state dir) | `core/state.rs` | runtime values (volumen, active source, layout, etc.) |
| `last_session.yml` (state dir) | `core/persisted_playback.rs` | `PersistedSession { playback, queue }`; la cola nativa y el playback no-Spotify |
| `mcp.json` (config dir) | `infra/mcp/control.rs` | `{ port, token, pid }`, `0600` |
| `plugin-data/<plugin>.json` | `infra/scripting` | storage privado de cada plugin |
| `plugins.lock` | `cli/plugin.rs` | plugins git instalados |
| `streaming_cache/credentials.json` | librespot `Cache` | credenciales Spotify |
| `qobuz_credentials.yml`, `soloist_credentials.yml` | fuentes | tokens propios |

- `behavior.relay_server_url` default: `wss://spotatui-party.spotatui.workers.dev/ws` (`src/core/user_config.rs:1453`).
- No hay `party.yml` ni estado de fiesta persistido: `PartySession` es memoria; al reiniciar se pierde.
- `config.yml` se escribe con permisos `0600` y directorio `0700`; el `relay_server_url` y `sync_token` son secretos y no se exponen a plugins (`BehaviorSnapshot` los excluye).
- `last_session.yml` se guarda de forma atómica (temp + rename) (`src/core/persisted_playback.rs:170-178`).

---

## 11. Jam en el repo y en GitHub

### 11.1 Búsqueda en el código

- `grep -rni "jam|group.session|shared queue|multiuser|collaborative queue"` no encuentra ninguna implementación de Jam; solo:
  - `connect/src/spirc.rs` del fork de librespot: comentario “might still be useful for later 'jam' support” y listener `social-connect/v2/session_update`.
  - `protocol/proto/social_connect_v2.proto`: proto de sesión social con `join_session_token`, `join_session_url`, `session_members`, `is_controlling`, `host_active_device_id`, `SessionType::{IN_PERSON, REMOTE, REMOTE_V2}`.
  - `src/core/i18n/*` y docs: textos de “Listening Party”.
- No hay `Queue` en `SyncMessage`, ni `party` en `src/infra/queue/*`.

### 11.2 GitHub: issues, PRs y discussions

Datos obtenidos con la API de búsqueda de GitHub el 2026-11-11 (no autenticada; los conteos son una foto y la búsqueda de GitHub es difusa):

| Referencia | Tipo | Estado | Título / contenido |
|---|---|---|---|
| Discussion **#331** | Discussion (General) | 1 comentario, 2026-07-04 | “Is it possible to create a jam?” — el usuario pregunta por la Jam de Spotify; el maintainer responde: “It doesnt have the official spotify jam yet but it does have a party mode where two or more people can join a party inside the terminal.” |
| Issue **#442** | Issue | Abierta 2026-08-12; cerrada 2026-10-28 | “Party shared-control mode is inert: PartyPlaybackCommand is never dispatched”. El invitado no podía enviar play/pause/skip; el maintainer pidió cablearlo en los transports de `App` para que también lo usaran GUI/MCP/Lua. |
| PR **#954** | PR | Mergeada, 2026-10-28 | “fix: repair ten open bug reports”; incluye el arreglo de #442: shared control desde teclas, GUI y teclas multimedia. |
| PR **#684** | PR | Mergeada, 2026-09-30 | “feat(gui): add the listening party screen”. Resumen del autor: “Left out: an activity log, votes, invite links and guest transport.” La cola de la GUI es la del host; el guest ve “The host picks the songs”. |
| PR **#685** | PR | Cerrada | “feat(gui): add the Listening Room with lyrics” (pantalla GUI adyacente; no añade cola compartida). |
| CHANGELOG `v0.37.1` | — | — | Introducción de “Listening Party Mode (Host/Join)”, código de 6, nombre de guest, relay `worker-relay/`, `behavior.relay_server_url`. |
| CHANGELOG `v0.48.0` | — | — | “Safer local channels”: el host aplica host-only, un guest no puede tomar el nombre del host, rate limit por IP, etc. |

Conteos de búsqueda (issues+PRs, `repo:LargeModGames/spotatui`):

- `jam`: **0** resultados.
- `"group session"` (frase exacta): **0**.
- `listening party`: **28**.
- `party`: **52** (incluye coincidencias difusas).
- `multiuser`: **0**.
- `collaborative queue`: **3**, todos PRs no relacionados (`#752` drop Web API, `#428` dj-core, `#432` MCP plugin).
- No apareció ninguna issue/PR titulada “shared queue” o “jam” con implementación en las primeras coincidencias revisadas; la búsqueda exacta de “shared queue” quedó interrumpida por el rate limit de la API (ver Incertidumbres).

**Lectura:** la comunidad pidió Jam una vez (#331), el maintainer reconoció que no existe la Jam oficial y ofreció la party; la party se construyó como sync de transporte y la propia PR de GUI dejó fuera votos, log, invite links y guest transport (este último se cerró después en #954). No hay un diseño previo de cola colaborativa en el repo.

---

## 12. Gap analysis: qué falta exactamente para una cola colaborativa

| Capacidad | Estado en 0.48 | Gap / dónde va |
|---|---|---|
| **Modelo de cola compartida** | No existe; `SyncState` solo lleva el track actual | Nuevo `JamQueue`/`JamState` en `core` (o `core/app/jam.rs`), con `items`, `current`, `history`, `version` |
| **Añadir tema** | El host puede añadir a `native_queue` localmente; no hay mensaje de red | `QueueIntent::Add { track, client_op_id }`; el host valida y aplica; `native_queue` como proyección |
| **Quitar tema** | Solo el host local (`remove_queue_item`); no hay red | `QueueIntent::Remove { item_id, base_version }`; el host aplica y broadcast |
| **Reordenar** | Solo el host local (`move_queue_item`, J/K en la pantalla Queue); no hay red | `QueueIntent::Move { item_id, to_index, base_version }`; usar IDs, no URI+posición |
| **Votar / consenso** | No existe; `SharedControl` es transporte global | Estado `votes` por ítem, reglas de mayoría, o “host decide” configurable |
| **Late join** | `JoinedRoom` solo host_name/control_mode; no hay roster ni cola | `RoomSnapshot` al admitir: roster, cola, current, versión, permisos; chunking si >4 KB |
| **Atribución por invitado** | `PlaybackCommand.from` existe pero el host lo ignora; no hay `added_by`; host name siempre “Host” | `participant_id` estable + `display_name` + `added_by` en cada ítem; `SetName` real; quizá `username` de Spotify |
| **Permisos** | Solo `HostOnly`/`SharedControl`; shared = todo el transporte | Roles: host, co-host, guest; permisos `queue_add`, `queue_remove_own`, `queue_remove_any`, `queue_move`, `transport`, `vote`; el host siempre manda |
| **Moderación** | No hay kick/ban/host transfer; si el host se va, la sala muere | `Kick`, `Ban`, `TransferHost`, `RoomPolicy`; o aceptar que la sala muere y documentarlo |
| **Resolución de conflictos** | Sin `seq`/`version`/`ack`; comandos fire-and-forget | `op_id`, `base_version`, `QueueAck { applied, version, error }`; el host es el único escritor |
| **Persistencia / reconexión** | Relay en memoria; host disconnect cierra sala; no hay resume | Estado de sala en Durable Object storage o snapshot del host al reconectar; `Resume` con token de sesión |
| **Tamaño de payload** | 4 KB y descarte silencioso | Snapshot por chunks, deltas/parches, o subir el límite y validar; el relay debe devolver error, no descartar |
| **Cobertura de fuentes** | Solo Spotify; Local/Subsonic/Qobuz/YouTube/radio solo heartbeat | Decidir: jam Spotify-only (más simple) o cola multi-fuente con `TrackInfo` y playback por fuente (más complejo) |
| **UI** | TUI popup 50x16 sin cola; GUI host ve su cola, guest no | Extender `draw_party` o nueva pantalla; `PartyPayload` con cola/roster; GUI `Party.tsx` con acciones y atribución |
| **Seguridad** | Código bearer, sin identidad Spotify, shared control total, sin rate limit por mensaje | Autenticar participantes (si se quiere), limitar comandos, separar “añadir” de “controlar transporte”, validar en host y relay |
| **Autoridad de reproducción** | `native_queue` solo si el host suena en spotatui; device externo = append-only y espejo de 20 | Forzar host en device nativo para jam, o aceptar jam “solo pedidos” en device externo |

### 12.1 Mínimo viable de una jam (si se recorta)

Si no se quiere construir todo:

1. **Host-authoritative queue** con add/remove/move y snapshot; sin votos.
2. **Roster + atribución** (`participant_id`, `display_name`, `added_by`).
3. **Permisos simples**: host edita todo; guests añaden y quitan solo lo suyo; transporte sigue en `HostOnly`/`SharedControl`.
4. **Late join snapshot** y reconexión básica.
5. **UI**: lista de cola en el popup TUI y en la página GUI, con autor.
6. **Límite**: 8 participantes y cola ≤50; snapshot por chunks si hace falta.

Eso ya es una jam útil y no requiere la Jam oficial de Spotify.

---

## 13. ¿Puede `native_queue: Vec<TrackInfo>` ser la cola autoritativa?

**Respuesta corta: no tal como está; sí como proyección de reproducción de una cola autoritativa nueva.**

### 13.1 Lo que ya resuelve

- Es estado de `App`, neutral de frontend, compartido por TUI/GUI/CLI (en la misma instancia).
- El motor de cola ya la consume y sabe suspender/reanudar contextos por fuente (`infra/queue/dispatch.rs`, `queue_suspend.rs`).
- `handle_native_spotify_track_end` permite que la cola preempte un contexto Spotify nativo (`src/core/app/queue.rs:281-304`), así que una jam puede interrumpir un álbum y reproducir lo siguiente de la cola.
- Se persiste en `last_session.yml` y se reconstruye al arrancar (`current_persisted_session` prepende `queue_now`).
- Las operaciones básicas (push, remove, move, clear, jump) ya existen y están testeadas.

### 13.2 Lo que le falta para ser autoritativa

1. **IDs estables**: se direcciona por `uri` + `position`; los duplicados y los reorders concurrentes son ambiguos (`queue_position_for`, `:83-96`).
2. **Autoría**: `TrackInfo` no tiene `added_by`, `added_at`, `client_op_id` ni estado de voto (`src/core/plugin_api.rs:37-77`).
3. **Versión**: no hay `version`/`seq` para resolver intents concurrentes ni para que un guest sepa si su operación se aplicó.
4. **Consumo destructivo**: `advance_native_queue` hace `remove(0)`; el ítem que suena sale de la cola y pasa a `queue_now`. Un guest que pida “la cola” vería solo los próximos, no el current; para mostrar la cola completa hay que combinar `queue_now` + `native_queue` o mantener un `JamQueue` con `current`/`history`.
5. **Inserción**: no hay `insert_at`; solo `push` al final. Para “añadir a continuación” o “mover a la posición N” hace falta una API nueva.
6. **`play_queue_item` es destructivo**: drena todos los anteriores; no sirve para “reproducir este ítem sin borrar la cola”.
7. **Device externo**: si el host está en un Connect externo, `add_track_to_native_queue` desvía a la cola interna de Spotify (append) y `native_queue` queda vacía (`src/core/app/queue.rs:50-59`). La autoridad sería Spotify, no la app.
8. **Fuentes decodificadas**: si el host reproduce Local/Subsonic/Qobuz/YouTube, `party_yields_to_local_playback()` hace que la fiesta no publique estado; la cola nativa puede existir, pero la jam no la sigue.
9. **Sin IPC**: `native_queue` no es accesible desde otro proceso; MCP solo la lee y añade, no la muta por completo.

### 13.3 Veredicto de diseño

- **No** exponer `native_queue` directamente a los guests ni dejar que la muten.
- **Sí** mantener una `JamQueue` autoritativa en el host (`items` con `id`, `track`, `added_by`, `votes`, `state`, `version`, `current`, `history`).
- El host aplica las intents de los guests en el pump serializado (una sola writer), valida permisos y versión, y **proyecta** el resultado en `native_queue` (los ítems después del current) para que el motor existente los reproduzca. `queue_now` sigue siendo el current.
- Al avanzar la cola, el host mueve el ítem de `JamQueue.current` a `history`, incrementa `version` y re-proyecta. Se puede enganchar en `try_play_queued`/`advance_native_queue` o en el tick que observa `queue_now`.
- Si se quiere evitar tocar el motor, una primera versión puede usar `native_queue` como proyección y un `HashMap<uri, JamItem>` para metadata; pero los IDs estables y la versión son necesarios igualmente.

---

## 14. Puntos de extensión para una jam

### (a) Fork nativo dentro de spotatui

**Qué tocar:**

- `core`: nuevo `JamQueue`/`JamState`, `JamItem`, `Participant`, `Permission`, `JamOp`; métodos `App::apply_jam_intent`, `App::project_jam_queue_to_native`.
- `infra/network/sync.rs`: ampliar `SyncMessage` con `JamSnapshot`, `JamPatch`, `QueueIntent`, `QueueAck`, `Roster`, `Vote`, `Kick`, `TransferHost`; versionar el protocolo (`protocol_version`).
- `infra/network/mod.rs`: `process_party_messages` y `sync_playback`; nuevos `IoEvent`; clasificarlos en las cuatro listas del pump.
- `worker-relay/src/index.ts`: aceptar y validar los tipos nuevos; reenviar intents al host; broadcast de patches; snapshot al join; tamaño/chunking; tests.
- `tui`: extender `draw_party` o nueva pantalla; teclas add/remove/move/vote; `help_entries`.
- `gui`: `PartyPayload`/`PartyRoom` con cola/roster; `Party.tsx` con acciones; `ts-rs`.
- Tests: reducer de cola, concurrencia, snapshot, relay.

**Esfuerzo:** alto (varias capas, Rust + TypeScript + UI + tests). **Riesgo:** medio. El riesgo principal no es técnico sino de compatibilidad: relays viejos, clientes viejos, feature flags, y la regla de que el host es el único writer. Si se hace con una feature `jam` y un `protocol_version`, el riesgo se controla.

### (b) Plugin Lua

**Qué permite:** pantalla custom, timers, HTTP a un servidor propio, storage de votos, comandos/keys, popup, playbar. **Qué no permite:** WebSocket, mutar `native_queue`, remove/reorder, ver la cola nativa, identidad de sala.

**Esfuerzo:** bajo (un plugin, sin tocar core). **Riesgo:** bajo. **Techo:** solo “cola de peticiones/votos” con aplicación manual por el host; no es una jam automática. Si el servidor externo hace append vía MCP/Spotify y el host reproduce, podría ser un prototipo, pero no cubre quitar/reordenar.

### (c) Sidecar/servidor con MCP o CLI

**MCP:** 18 tools, append + skip + play; no remove/reorder, no push, no party. **CLI:** sin cola, sin JSON de playback, sin IPC con la TUI.

**Esfuerzo:** medio (servidor + cliente MCP + polling + estado propio). **Riesgo:** alto: el sidecar y la TUI compiten por el estado; no hay atomicidad; el host puede cambiar la cola por su cuenta; no hay forma de borrar/reordenar; los guests no tienen identidad. Solo vale para un “bot DJ” o para un prototipo de solo añadir.

### (d) Parche upstream

Dos variantes:

1. **Jam oficial de Spotify** vía `social-connect/v2` en el fork de librespot. El proto y el listener de `session_update` ya existen; faltan create/join/leave, resolución de `join_session_token`/`join_session_url`, roles y el modelo de cola de Spotify. **Esfuerzo:** muy alto; **riesgo:** muy alto (API interna no documentada, cambios de Spotify, ToS, tests imposibles sin cuentas reales). No lo recomendaría como primera vía.
2. **Jam propia upstream**: la misma propuesta de (a), pero con revisión del maintainer y feature flag. **Esfuerzo:** alto; **riesgo:** medio (aceptación del diseño, tamaño del PR, compatibilidad). Es la vía correcta si el objetivo es que la función viva en spotatui, no en un fork privado.

### Tabla resumen

| Vía | Esfuerzo | Riesgo | Cobertura | Comentario |
|---|---|---|---|---|
| (a) Fork nativo | Alto | Medio | Completa (add/remove/reorder/vote/snapshot) | Recomendada; `JamQueue` + `native_queue` como proyección |
| (b) Plugin Lua | Bajo | Bajo | Solo peticiones/votos, sin mutar cola | Prototipo/UI, no jam real |
| (c) Sidecar MCP/CLI | Medio | Alto | Append + skip, sin remove/reorder | No recomendado para jam |
| (d) Upstream | Muy alto | Muy alto | Oficial o completa | Solo con buy-in del maintainer; Jam oficial es frágil |

---

## 15. Diseño recomendado (si se implementa)

### 15.1 Modelo

```text
JamState {
  protocol_version: u16,
  room_id: String,
  host_participant_id: ParticipantId,
  participants: Vec<Participant>,       // id, display_name, role, permissions
  items: Vec<JamItem>,                  // orden de reproducción
  current: Option<JamItem>,             // el que suena
  history: Vec<PlayedItem>,             // para late join / auditoría
  version: u64,                         // se incrementa con cada mutación aplicada
  policy: JamPolicy,                    // add/remove/move/vote/transport
}

JamItem {
  id: JamItemId,                        // UUID/ULID estable
  track: TrackInfo,                     // metadatos y URI
  added_by: ParticipantId,
  added_at: u64,
  score: i32,                           // votos
  state: Queued | Playing | Played | Skipped,
  client_op_id: Option<String>,         // idempotencia
}
```

### 15.2 Protocolo

- `JamSnapshot { version, room, participants, items, current, history_tail }` — al admitir o al reconectar; si >4 KB, `JamSnapshotChunk { snapshot_id, index, total, chunk }`.
- `QueueIntent { op_id, base_version, op: Add|Remove|Move|Vote|Clear|PlayNow, ... }` — guest → host.
- `QueueAck { op_id, applied, version, error? }` — host → guest.
- `JamPatch { version, ops: Vec<AppliedOp> }` — host → todos; incluye `added_by` y el estado resultante.
- `Roster { participants }`, `PermissionChanged`, `Kick`, `TransferHost`.
- `SyncState` se mantiene para el transporte actual; la cola es un canal aparte.

Reglas:

- El host es el **único** writer de `JamState`; los guests solo envían intents.
- `base_version` + `op_id` permiten rechazar/rebasar; el host aplica en orden y devuelve `version`.
- El relay no decide la cola; solo valida tipos, reenvía al host y difunde. Si se quiere late join sin host, el relay puede guardar el último snapshot en `ctx.storage` (opcional).
- Compatibilidad: clientes viejos ignoran los tipos nuevos; el host puede rechazar `protocol_version` no soportada y seguir en modo transporte.

### 15.3 Proyección a `native_queue`

- En cada cambio de `JamState`, el host reconstruye `native_queue` con los ítems `Queued` (después de `current`) y deja `queue_now` intacto.
- El motor existente sigue haciendo `advance_native_queue`; al arrancar un ítem, el host lo marca `Playing`/lo mueve a `history` y actualiza `JamState.current`.
- Para no pelear con `add_track_to_native_queue` (que tiene side effects y desvíos), añadir un método interno `App::set_native_queue_projection(Vec<TrackInfo>)` que solo reemplace los próximos, sin mensajes de estado ni `GetQueue`.
- Si el host está en device externo, o se fuerza el device nativo para la jam, o se rechaza el modo “cola compartida” y se queda en “solo peticiones” (append a Spotify).

### 15.4 Permisos

- `Host`: todo.
- `CoHost`: add/remove/move, kick guests, no transfer.
- `Guest`: add; remove solo lo propio; vote; transport solo si `shared_control`.
- `policy`: `host_only` (solo host edita), `collaborative` (guests añaden/quitan lo suyo), `open` (todos editan, con rate limit).
- Validar siempre en el host, aunque el relay también lo haga (precedente de `handle_incoming_playback_command`).

### 15.5 UI

- TUI: lista de cola dentro de `draw_party` con `ListState`, scroll y teclas `a` (add), `x` (remove), `J/K` (move), `v` (vote); o nueva ruta `RouteId::Jam` con el checklist de seis sitios.
- GUI: `PartyRoom` con `items`, `participants`, `version`; `Party.tsx` ya tiene “Up next”, añadir botones y atribución; `ts-rs` regenera bindings.
- Mostrar `added_by` y `votes`; para el guest, marcar “you” y el estado de su intent.

### 15.6 Fases

1. **Fase 0 (prototipo):** `JamState` en memoria, snapshot al join, add/remove/move, sin votos; relay acepta los tipos nuevos; UI TUI básica.
2. **Fase 1:** atribución, permisos, `QueueAck`, late join completo, tests de concurrencia.
3. **Fase 2:** votos, moderación, persistencia/reconexión, chunking, límites configurables.
4. **Fase 3 (opcional):** explorar `social-connect/v2` para la Jam oficial, como feature experimental y separada.

---

## 16. Build, test, licencia, MSRV y modularización

### 16.1 Comandos del repo

`AGENTS.md` / `CONTRIBUTING.md`:

```bash
cargo run
cargo run --no-default-features --features telemetry,tui
cargo run --features all-sources

cargo fmt --all
cargo clippy --no-default-features --features telemetry,tui -- -D warnings
cargo test --no-default-features --features telemetry,tui
```

La matriz CI tiene nueve patas: `default`, `all-sources`, `mcp-only`, `ai-dj-only`, `slim`, `headless`, `headless-streaming`, `gui`, `soloist`, más `macos`. Cada pata usa `--locked`.

### 16.2 Verificación realizada

En `/tmp/opencode/research/spotatui-a`:

```bash
cargo check --locked --no-default-features --features telemetry,tui
# Finished dev profile ... in 42.76s
# EXIT=0

cargo test --locked --no-default-features --features telemetry,tui
# test result: ok. 1925 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.64s
# EXIT=0

cargo check --locked --no-default-features --features telemetry,tui,mcp-server
# Finished dev profile ... in 9.07s
# EXIT=0
```

- La pata slim (`--no-default-features --features telemetry,tui`) compila la TUI y el núcleo, pero **no** activa `streaming`, `scripting`, `mcp-server`, `gui` ni la meta-feature `queue`; por eso el `cargo test` slim no cubre librespot, Lua, MCP ni la GUI. El check separado con `mcp-server` sí compila esa feature. `core/app/queue.rs` compila igual porque sus tipos no están gateados, pero `QueueNowPlaying` y el motor de cola sí dependen de `queue`/`audio-decode-queue`.
- No se ejecutaron `default`, `all-sources`, `gui`, `soloist` ni la matriz completa; no se ejecutó `cargo clippy`. Por tanto, “compila y tests pasan” se limita a la pata slim + check MCP.
- El repo tiene 58 features y 87 dependencias directas; el build slim es rápido porque la mayor parte de deps ya estaba cacheada.

### 16.3 Licencia y MSRV

- MIT, compatible con un fork y con una extensión propietaria separada (si se distribuye, conservar el aviso).
- MSRV 1.90; el entorno tiene 1.94.1. `rust-toolchain.toml` no está en la raíz (solo el fork de librespot tiene el suyo), así que el proyecto no fija toolchain; CI usa stable.
- `Cargo.toml` tiene `edition = 2021`, `default-run = "spotatui"`, y `[profile.release]` con LTO, `opt-level = "s"`, `strip = true`.

### 16.4 Modularización

**Bien:**

- `core` es neutral de frontend; `tui` no se importa desde `core`/`infra`/`cli` (la pata `headless` lo verifica).
- El pump tiene routers por esquema antes del handler Spotify, lo que mantiene `infra/network` Spotify-only.
- `Action` centraliza las mutaciones y permite que Lua/DJ/MCP/TUI compartan caminos.
- `DisplayRevisions` desacopla la GUI del render.

**Mal / riesgos para la jam:**

- Un solo crate: no hay separación física entre “modelo de cola” y “transporte”; un cambio de protocolo toca `core`, `infra`, `tui`, `gui` y `worker-relay`.
- `App` es un struct plano de 171 campos; añadir `JamState` es fácil, pero la disciplina de privacidad y los getters importan.
- La fiesta está repartida en varios archivos y no tiene un módulo de dominio `jam` coherente; para una jam conviene crear `src/core/app/jam.rs` y `src/infra/jam/` (o `src/infra/network/jam.rs`) en vez de seguir metiendo brazos en `mod.rs`.
- El relay TypeScript no tiene tests ni tipos compartidos con Rust; el contrato es JSON a mano. Un cambio de protocolo puede romper clientes viejos silenciosamente.
- `worker-relay` no está en el workspace ni en CI de Rust; su build/deploy es independiente (`wrangler`).

---

## 17. Hechos verificados

- **Commit y versión:** `4c5fc994592c2f7ea70d1e270711632640bd5531`, `0.48.0`, fecha 2026-11-10; `Cargo.toml` `version = "0.48.0"`, `license = "MIT"`, `rust-version = "1.90"`.
- **Tamaño/estructura:** 353 `.rs`, ~207.893 líneas en `src/`; un solo paquete; `desktop/` y `worker-relay/` aparte; no hay `crates/`.
- **Listening Party existe** en TUI, GUI y relay: `src/core/app/party.rs`, `src/infra/network/sync.rs`, `src/infra/network/mod.rs`, `src/tui/handlers/party.rs`, `src/tui/ui/popups.rs:1718`, `src/gui/protocol.rs:756`, `gui/src/Party.tsx`, `worker-relay/src/index.ts`.
- **Protocolo:** `SyncMessage` en `src/infra/network/sync.rs:54-94` tiene exactamente `RoomCreated`, `JoinedRoom`, `GuestJoined`, `GuestLeft`, `SyncState`, `PlaybackCommand`, `SetControlMode`, `SetName`, `Heartbeat`, `Error`, `RoomClosed`. No hay mensaje de cola.
- **SyncState:** solo `track_uri`, `position_ms`, `is_playing`, `timestamp` (`sync.rs:73-78`); se publica cada 2 s (`src/core/app/tick.rs:126-133`) y se consume en `src/infra/network/mod.rs:1852-1932`.
- **Relay:** Cloudflare Worker + Durable Object `PartyRoom` (`worker-relay/src/index.ts:71`), `MAX_MESSAGE_SIZE = 4*1024` (`:14`), `MAX_PARTICIPANTS = 8` (`:15`), `CODE_LENGTH = 6` (`:13`), rate limit 10/10 (`:11-12`), inactividad 5 min (`:16`), host disconnect cierra sala (`:283-296`), sin persistencia de sala (`:72-77`).
- **Relay en producción:** `GET https://spotatui-party.spotatui.workers.dev/ws` responde `426 Upgrade Required` (endpoint vivo); `GET /` responde `404`.
- **Modos:** `ControlMode::{HostOnly, SharedControl}` (`sync.rs:10-16`); el relay solo acepta `set_control_mode` del host (`index.ts:232-237`); el host revalida (`mod.rs:1934-1944`); `from` se ignora (`mod.rs:1811`).
- **UI TUI:** popup 50x16, sin lista de cola (`src/tui/ui/popups.rs:1718-1740`); handler de código/nombre (`src/tui/handlers/party.rs`).
- **UI GUI:** `PartyPayload`/`PartyRoom` sin cola (`src/gui/protocol.rs:756-786`); `Party.tsx` pinta cola solo si `room.host`, y `host_picks_songs` para guest.
- **Docs desactualizadas:** `docs/listening-party.md` dice que los guests no envían comandos aún; el código de `relay_party_transport` (`src/core/app/party.rs:92-116`) y #954 los relayan.
- **Cola nativa:** `App.native_queue: Vec<TrackInfo>` (`src/core/app/mod.rs:249`); `add_track_to_native_queue` (`src/core/app/queue.rs:32`), `remove_queue_item` (`:123`), `move_queue_item` (`:130`), `play_queue_item` (`:100`, drena anteriores), `clear_native_queue` (`:142`), `handle_native_spotify_track_end` (`:281`).
- **Motor de cola:** `route_queue_event` (`src/infra/queue/dispatch.rs:39`), `advance_native_queue` (`:269`, `native_queue.remove(0)`), `QueueNowPlaying` (`src/infra/queue/mod.rs:566`), `advance_decision` (`:194`).
- **Persistencia:** `PersistedSession { playback, queue }` (`src/core/persisted_playback.rs:48`); `current_persisted_session` prepende `queue_now` (`src/core/app/persistence.rs:195-211`); no hay estado de fiesta persistido.
- **Cola Spotify:** `queue` lee `cluster.player_state.next_tracks` y corta a `QUEUE_LENGTH = 20` (`src/infra/spotify/player.rs:33`, `:426-440`); `add_to_queue` envía comando `add_to_queue` (`:645-660`); no hay remove/reorder en el router (`src/infra/spotify/router.rs:215-225`).
- **Autenticación:** device auth con `SPOTIFY_PLAYER_CLIENT_ID = "65b708073fc0480ea92a077233ca87bd"` y seis scopes (`src/infra/spotify/credentials.rs:11-22`), cache de librespot; no hay developer app ni PKCE; la API interna sustituye a la Web API (`src/infra/spotify/router.rs:1-3`; `src/infra/network/requests.rs:339`).
- **Plugins:** `mlua` + `Lua::new()` (`src/infra/scripting/engine.rs:90`), API v6 (`src/core/plugin_api.rs:15`), `PLUGIN_TIME_LIMIT = 1 s` (`src/infra/scripting/shared.rs:75`), `add_to_queue` → `Action::AddToQueue` (`src/infra/scripting/api.rs:478`), `queue()` → `app.queue` (`src/core/plugin_api.rs:809-824`); sin WebSocket ni mutación de `native_queue`.
- **MCP:** 18 herramientas (`src/infra/dj/tools.rs`, conteo 18), stdio + loopback token (`src/infra/mcp/control.rs:57-66`, `:146-160`), `get_queue` solo `native_queue` (`:1265-1285`), `queue_tracks` append (`:129-164`), sin remove/reorder.
- **CLI:** subcomandos `playback`, `play`, `list`, `search`, `sync`, `history recap`, `mcp`, `plugin`, `update` (`docs/cli.md`, `src/cli/clap.rs`); sin cola, sin JSON de playback, sin daemon; `CliApp` usa su propio `Network` (`src/cli/cli_app.rs`).
- **GitHub:** Discussion #331 (2026-07-04) pide Jam; el maintainer dice que no existe la Jam oficial y que hay party mode. Issue #442 cerrada; PR #954 la arregla (2026-10-28). PR #684 (2026-09-30) añade GUI party y deja fuera “activity log, votes, invite links and guest transport”. Búsqueda `jam`: 0; `"group session"`: 0; `listening party`: 28; `party`: 52.
- **Fork librespot:** `SPOTATUI_FORK.md`; proto `protocol/proto/social_connect_v2.proto` con `join_session_token`, `join_session_url`, `session_members`, `is_controlling`, `host_active_device_id`; Spirc escucha `social-connect/v2/session_update` (`connect/src/spirc.rs:232-234`) y actualiza `session_id` (`:1983-2034`); no hay create/join/leave.
- **Build/test:** `cargo check` slim `EXIT=0` 42.76 s; `cargo test` slim `1925 passed, 0 failed` 3.64 s; `cargo check` slim+mcp `EXIT=0` 9.07 s. No se ejecutó la matriz completa ni clippy.

---

## 18. Incertidumbres

- **Jam oficial de Spotify:** el fork contiene el proto `social_connect_v2` y el listener de `session_update`, pero no hay llamadas de create/join/leave ni resolución de `join_session_token`. No sé si los endpoints internos de Spotify siguen disponibles, si requieren headers/versión de cliente concretos, ni si funcionarían con una cuenta normal. Cualquier implementación sería reverse engineering, no una API pública.
- **Reordenar/borrar en la cola remota de Spotify:** `infra/spotify/player.rs` solo implementa `add_to_queue`; no sé si el `send_player_command` interno acepta `set_queue`, `remove_from_queue` o `reorder`. La API pública no lo permite; la interna no está documentada. No lo afirmo.
- **Persistencia del Durable Object:** el código guarda las sesiones en memoria y no usa `ctx.storage` para la sala. No sé si Cloudflare puede desalojar el DO entre mensajes y perder `sessions`/`hostWs`; si ocurre, un guest vería “Room not found” aunque el host siga conectado. No hay tests de ello.
- **Tamaño de 4 KB:** `raw.length` en JS cuenta unidades UTF-16, no bytes UTF-8; el límite real en bytes puede ser mayor para texto no ASCII. No sé cuántos `TrackInfo` caben exactamente; depende del JSON y de los campos opcionales. Un snapshot de 20 temas probablemente lo supera, pero no lo medí con el serializador real.
- **Reconexión del host:** no hay lógica de resume; no sé si el relay reasigna la sala al mismo host tras un corte breve o si el `close` del socket dispara siempre `room_closed`. El código sugiere que cualquier cierre del host termina la sala.
- **Identidad de participantes:** el relay no verifica la cuenta Spotify; no sé si dos instancias con la misma cuenta pueden unirse a la misma sala (el cliente solo comprueba `spotify_connected` local). Tampoco sé si el `username` de librespot podría usarse como identidad estable sin exponerlo.
- **Nombre del host:** el relay fija `"Host"` y el cliente no envía `SetName`; no sé si hay algún camino no visible que lo cambie. En el código auditado, no.
- **Documentación de la fiesta:** `docs/listening-party.md` está desactualizada en shared control; no sé si hay otras afirmaciones obsoletas (p. ej. el número de listeners o el comportamiento de `Esc`).
- **Búsqueda GitHub:** los conteos se tomaron con la API no autenticada y se agotó el rate limit a mitad; las cifras `28`/`52` son una foto, no un censo. La búsqueda de GitHub es difusa y puede omitir issues por sinónimos.
- **Cobertura de tests:** la suite slim no compila `scripting`, `mcp-server`, `gui`, `streaming` ni `all-sources`; los tests de fiesta cubren lógica y protocolo, no un WebSocket real ni el Worker. No sé si el relay desplegado acepta exactamente los mismos tipos que el código local (el endpoint responde 426, pero no hice un handshake autenticado).
- **Multi-instancia:** no verifiqué el lock de instancia única (`runtime/instance.rs`) para el caso CLI + TUI simultáneos; el CLI construye su propio `Network`, pero no sé si comparte el cache de credenciales de forma segura con la TUI.
- **`native_queue` como autoridad:** la proyección propuesta es mi diseño, no algo que el repo ya soporte. No hay un método `set_native_queue_projection`; habría que añadirlo y testear la interacción con `advance_native_queue`, `queue_now`, `queue_suspended` y la recuperación de backend.
- **Fuentes decodificadas en jam:** no sé si tiene sentido sincronizar posición de un FLAC local entre máquinas con relojes distintos; el protocolo actual solo compara URIs de Spotify. Una jam multi-fuente requeriría decidir si cada guest reproduce su propia copia o si solo se comparte el “now playing”.

---

## 19. Referencias rápidas

- `docs/listening-party.md` — guía de usuario; contiene la frase obsoleta sobre guest transport.
- `docs/scripting.md` — API Lua completa, incluida la nota de “not sandboxed”.
- `docs/mcp-setup.md` — 18 tools y el flujo `spotatui mcp status`.
- `docs/cli.md` — subcomandos y `--format`.
- `AGENTS.md` — arquitectura, pump, playback ownership, reglas de features.
- `src/core/app/AGENTS.md` — estructura de `App`, ownership, errores.
- `src/infra/network/AGENTS.md` — pump, routers, listening party, listas de clasificación de `IoEvent`.
- `src/infra/queue/AGENTS.md` — repeat/shuffle de fuentes decodificadas.
- `src/infra/scripting/AGENTS.md` — reglas de snapshots y `Action`.
- `src/infra/mcp/AGENTS.md` — transporte y separación de errores de protocolo/ejecución.
- `CHANGELOG.md` — `v0.37.1` (party inicial), `v0.48.0` (hardening), #442/#684/#954.
- `/tmp/opencode/research/librespot-fork/SPOTATUI_FORK.md` — parches del fork.
- `/tmp/opencode/research/librespot-fork/protocol/proto/social_connect_v2.proto` — proto de sesión social.
