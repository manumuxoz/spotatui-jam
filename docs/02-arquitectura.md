# 02 — Arquitectura

## 1. Principio rector

No construir una jam desde cero. **Listening Party ya resuelve el 70 % del problema**: autenticación, sala, presencia, relay, sincronía de playback y control compartido. La extensión añade una única cosa que falta: **una cola colaborativa con el host como autoridad**.

El diseño sigue tres invariantes:

1. **El host manda.** Ningún invitado escribe estado directamente; envía comandos y el host valida, aplica y difunde.
2. **La reproducción no se toca.** Se reutiliza el modelo actual: cada participante reproduce el track del host con su propia cuenta Premium. La cola no transporta audio.
3. **El relay es tonto.** No guarda la cola ni decide permisos; solo enruta mensajes, aplica límites de transporte y protege la sala. Esto permite self-hosting y mantiene el estado en el cliente.

---

## 2. Estado actual de spotatui (hechos verificados)

Línea base auditada: commit `4c5fc99` (2026-11-10), versión 0.48.0, 353 archivos `.rs` y ~207.900 líneas en `src/`; `cargo test` pasa 1.925 tests y el build por defecto tarda ~43 s en la máquina de auditoría. MSRV 1.90.

| Componente | Qué hace hoy | Archivo |
|---|---|---|
| `SyncMessage` | Enum del protocolo: `room_created`, `joined_room`, `guest_joined/left`, `sync_state`, `playback_command`, `set_control_mode`, `set_name`, `heartbeat`, `error`, `room_closed` | `src/infra/network/sync.rs` |
| `PartyConnection` | WebSocket dividido en sink/stream; el reader parsea y manda a un mpsc | `src/infra/network/sync.rs` |
| Relay | Cloudflare Worker + Durable Object por sala; código de 6 chars; máx. 8 participantes; máx. 4 KB por mensaje; rate limits por IP; sala expira a los 5 min sin actividad; el host cierra la sala al irse | `worker-relay/src/index.ts` |
| Sincronía | El host publica `track_uri`, `position_ms`, `is_playing`, `timestamp` cada 2 s; el invitado corrige con seek si el drift > ~3 s | `src/infra/network/mod.rs`, `docs/listening-party.md` |
| Control | `host_only` o `shared_control`; en shared los invitados pueden play/pause/next/prev/seek y el relay lo reenvía al host | `src/core/app/party.rs`, `worker-relay` |
| UI | Popup de Party: crear/unirse, código, nombre, miembros, salir, alternar control | `src/tui/ui/popups.rs`, `src/tui/handlers/party.rs` |
| Cola nativa | `App.native_queue: Vec<TrackInfo>`; el motor `infra/queue` la consume y suspende/reanuda el contexto de la fuente | `src/core/app/mod.rs`, `src/core/app/queue.rs`, `src/infra/queue/mod.rs` |
| Auth | Login nativo de Spotify, sin developer app ni client ID; Spotify requiere Premium | `docs/installation.md`, PR #752 |
| Plugins | Lua con HTTP GET/POST, timers, screens custom, acciones de playback y `add_to_queue`; **sin WebSocket** y sin quitar/reordenar la cola | `docs/scripting.md` |

### Limitaciones actuales que afectan al diseño

- La cola del host **no se comparte**: el invitado solo recibe el track actual, no lo que viene.
- El relay limita cada mensaje a **4 KB**; una cola de 100 items no cabe en un solo JSON.
- El protocolo no tiene identidad estable de participante: solo nombres; no se puede atribuir un add o un voto con fiabilidad.
- `SyncState` no incluye la cola ni un número de revisión.
- El host solo sincroniza mientras reproduce YouTube en la Party Queue; si cambia a otra fuente, la sala sigue abierta pero la cola no avanza.
- En modo `shared_control` los invitados ya envían transporte, aunque la documentación dice lo contrario; hay que corregir la doc.
- No hay reconexión del host: si se cae, la sala muere y el código se pierde.

---

## 3. Vista de componentes

```
┌──────────────────────────────┐         ┌──────────────────────────────┐
│ spotatui host                │         │ spotatui invitado            │
│                              │         │                              │
│  TUI Party                   │         │  TUI Party                   │
│    │                         │         │    │                         │
│  PartyQueue (autoridad)      │         │  PartyMirror (réplica)       │
│    │                         │         │    │                         │
│  native_queue ──► playback   │         │  play_uri + seek ──► playback│
│    │                         │         │    │                         │
│  PartyConnection (WS)        │         │  PartyConnection (WS)        │
└──────────────┬───────────────┘         └──────────────┬───────────────┘
               │                                        │
               └──────────────┬─────────────────────────┘
                              │  WebSocket (JSON)
                   ┌──────────▼──────────┐
                   │  Relay (Worker o    │
                   │  Rust self-host)    │
                   │  - salas            │
                   │  - miembros         │
                   │  - rate limits      │
                   │  - pass-through     │
                   └─────────────────────┘
```

### Componentes nuevos

| Componente | Responsabilidad | Ubicación propuesta |
|---|---|---|
| `party::queue::PartyQueue` | Estado autoritativo del host: items, revisión, configuración, límites, idempotencia | `src/core/party/queue.rs` |
| `party::queue::PartyMirror` | Réplica del invitado: aplica snapshots/patches, detecta gaps, pide resync | mismo módulo |
| `party::protocol` | Tipos de mensajes v2, serialización, validación, chunking | `src/infra/network/party_protocol.rs` |
| `party::relay` | Cliente WebSocket v2; compatibilidad con relay v1 | `src/infra/network/sync.rs` (extender) |
| `PartyView` | Pantalla completa de cola/miembros/ajustes | `src/tui/ui/party.rs` (nuevo) |
| `party-sim` | Simulador host/N invitados sin audio para soak (P1) | `tests/party_sim` o crate aparte |
| `jam-relay` (P1) | Relay Rust auto-hospedable, mismo protocolo | `relay/` (nuevo crate del workspace) |

No se crea un daemon separado para la v1. Un daemon añade un proceso, un socket local, autenticación y empaquetado; no resuelve nada que el fork no resuelva mejor, y rompe la experiencia “todo en la TUI”.

---

## 4. Modelo de datos

### 4.1 Item de cola

```rust
pub struct PartyQueueItem {
    pub id: u64,                    // asignado por el host, único en la sala
    pub uri: String,                // solo youtube:<id> en la cola v1
    pub title: String,              // truncado a 200 chars
    pub artists: Vec<String>,       // máx. 8, cada uno 200 chars
    pub duration_ms: u32,           // 1..=3_600_000
    pub added_by: ParticipantId,    // id estable del relay
    pub added_by_name: String,      // snapshot del nombre, para historial
    pub added_at_ms: u64,
    pub votes: i32,                 // P1; 0 en MVP
}

pub struct PartyQueue {
    pub revision: u64,                  // +1 por cada mutación
    pub next_id: u64,
    pub current: Option<PartyQueueItem>, // el track que suena ahora
    pub items: Vec<PartyQueueItem>,     // próximos, en orden de reproducción
    pub history: Vec<PlayedItem>,       // últimos reproducidos/saltados (máx. 50)
    pub config: PartyQueueConfig,
}

pub struct PlayedItem {
    pub item: PartyQueueItem,
    pub played_at_ms: u64,
    pub skipped: bool,
}

pub struct PartyQueueConfig {
    pub mode: QueueMode,            // HostOnly | GuestsAdd | GuestsManage
    pub max_items: usize,           // default 100
    pub max_pending_per_guest: usize, // default 5
    pub add_cooldown_ms: u64,       // default 2000
    pub dedupe: bool,               // default true
    pub allow_explicit: bool,       // default true
    pub stop_when_empty: bool,      // default false (continuar contexto)
}
```

### 4.2 Identidad de participante

El relay asigna un `ParticipantId` opaco (`String`, UUID corto) al aceptar la conexión y lo comunica en `joined_room`/`guest_joined`. Todos los comandos reenviados llevan `from` con ese id. El host nunca confía en un `from` enviado por el cliente: usa el que añade el relay.

- Host: id reservado `"host"`.
- Invitado: id aleatorio del relay, estable durante la conexión.
- Nombre: solo para mostrar; puede repetirse (el relay ya resuelve colisiones).
- En v1 no hay autenticación criptográfica: un invitado podría abrir dos conexiones y tener dos identidades. Es aceptable para una fiesta; se documenta.

### 4.3 Relación con `native_queue`

`native_queue` es la cola real que consume el motor de playback. `PartyQueue` es la capa de intención colaborativa. Para evitar dos fuentes de verdad:

**Decisión:** durante una jam, `PartyQueue` es la autoridad y `native_queue` es su **ventana de materialización**. `current` vive en `queue_now`; `native_queue` contiene los próximos items:

```rust
fn sync_native_queue(&mut self) {
    self.native_queue = self.party_queue.items
        .iter()
        .map(|item| item.to_track_info())
        .collect();
}
```

Consecuencias:

- Al iniciar la jam, si suena un vídeo de YouTube se **adopta** como `current`; la `native_queue` existente se adopta como `items` (con ids nuevos) o se vacía, según configuración. Default: adoptar.
- Cuando el motor consume un item (`AdvanceNativeQueue`), se notifica a `PartyQueue`: el `current` anterior pasa a `history`, el head de `items` pasa a `current` y se incrementa `revision`. Esto requiere un hook pequeño en `infra/queue`.
- Si el motor introduce mutaciones externas que no pasan por la cola (reorder, clear desde la pantalla Queue), se reduce la ventana a 1–2 items y se materializa el siguiente explícitamente al avanzar. Es el plan B si la derivación completa resulta frágil.
- Si el host añade un track por la pantalla Queue normal, se redirige a `PartyQueue` mientras la jam esté activa; si no, se comporta como siempre.
- Al terminar la jam, `native_queue` queda como esté; no se destruye nada.
- Solo los vídeos de YouTube (`youtube:<id>`) entran en `PartyQueue` v1. Las demás fuentes (local, Qobuz, Subsonic, radio) y los `spotify:track:` se rechazan con un mensaje claro.

Alternativa considerada: mantener `PartyQueue` como metadata paralela y mutar `native_queue` a la vez. Se descarta por el riesgo de divergencia en reorder/consume. La variante de “solo materializar el siguiente item” se conserva como fallback si el motor no permite derivar la cola completa.

### 4.4 Invariantes

1. `revision` es monotónica y nunca decrece.
2. `items.len() + current.is_some() as usize <= config.max_items`.
3. `item.id` es único y estable durante la sesión, incluido `current` e `history`.
4. Solo hay URIs `youtube:<id>` válidas en la cola.
5. El orden de `PartyQueue.items` es el orden de la ventana de `native_queue`; en v1, la ventana contiene los próximos items.
6. `current` es el item que el motor tiene en `queue_now`; no está en `items`.
7. Un `QueuePatch` con `revision = N` solo se aplica si la réplica está en `N-1`; si no, se pide snapshot.
8. Un `op_id` se aplica como máximo una vez por participante.
9. El host es el único que produce `queue_patch` y `queue_snapshot`.
10. Los invitados solo producen `queue_command`.
11. `votes` no altera el orden en el MVP; se muestra como información.
12. `history` está acotado a 50 entradas; no crece sin límite.

---

## 5. Protocolo v2

### 5.1 Negociación

En `create`/`join` se añade `v=2` y `features=queue` a la query del WebSocket. El relay responde en `room_created`/`joined_room`:

```json
{
  "type": "room_created",
  "code": "ABC123",
  "control_mode": "host_only",
  "protocol_version": 2,
  "features": ["queue"],
  "you": {"id": "host", "name": "manu", "role": "host"}
}
```

- Cliente nuevo + relay viejo: no hay `features`; la UI muestra “este relay no soporta cola” y la jam funciona como Listening Party clásico.
- Cliente viejo + relay nuevo: ignora los campos desconocidos; sigue sincronizando playback.
- Host viejo + invitado nuevo: no hay `queue_snapshot`; el invitado muestra la cola como no disponible.
- Host nuevo + invitado viejo: el invitado ignora `queue_*` (su parser descarta el mensaje y sigue); el playback no se rompe.

### 5.2 Mensajes nuevos

Todos usan el mismo sobre `{ "type": "...", ... }` y `serde` con `rename_all = "snake_case"`.

| Tipo | Dirección | Campos | Propósito |
|---|---|---|---|
| `queue_snapshot` | host → todos / a uno | `revision`, `snapshot_id`, `chunk`, `chunks`, `to?`, `current?`, `items[]`, `history_tail[]?` | Estado completo, troceado |
| `queue_patch` | host → todos | `revision`, `ops[]` | Cambios incrementales |
| `queue_command` | invitado → host (relay) | `op_id`, `from`, `base_rev`, `action` | Petición de mutación |
| `queue_ack` | host → invitado | `op_id`, `ok`, `error?`, `revision?`, `to?` | Resultado |
| `queue_resync` | invitado → host | `have_revision` | Petición de snapshot tras un gap |
| `party_config` | host → todos | `config` | Permisos y límites |
| `party_members` | host → todos | `members[]` | Lista con ids y roles |

`to` permite dirigir un snapshot o un ack a un participante concreto (late join); sin `to`, el relay lo difunde. `base_rev` es la revisión que el invitado conocía al enviar la acción; el host puede rechazar la operación como `stale` si la cola cambió de forma incompatible.

`queue_snapshot` se envía al entrar, al pedir resync y tras un cambio de configuración que afecte a la interpretación de la cola. `queue_patch` se envía en cada mutación, con coalescing de 100 ms si hay ráfaga.

### 5.3 Acciones

```rust
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueueAction {
    Add { uri: String, title: String, artists: Vec<String>, duration_ms: u32 },
    Remove { id: u64 },
    Move { id: u64, after: Option<u64> }, // None = al principio
    Clear,
    PlayNow { id: u64 },
    Vote { id: u64, value: i8 }, // P1
}
```

`Move` usa `after` (id del item tras el cual insertar) en vez de un índice: dos movimientos concurrentes no se pisan por el desplazamiento de posiciones. La UI sigue mostrando índices; el host traduce.

`QueuePatch` no reutiliza `QueueAction`; usa operaciones ya aplicadas:

```rust
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueueOp {
    Add { item: PartyQueueItem },
    Remove { id: u64 },
    Move { id: u64, after: Option<u64> },
    Clear,
    /// El head (o el item elegido) pasa a `current`; el anterior va a `history`.
    Advance { current: PartyQueueItem, played_id: u64, skipped: bool },
    /// Cambio de `current` sin consumir el head (play now).
    SetCurrent { current: PartyQueueItem, played_id: Option<u64>, skipped: bool },
    Reset { current: Option<PartyQueueItem>, items: Vec<PartyQueueItem> }, // correcciones internas
}
```

`Advance` es atómico: el receptor mueve el `current` anterior a `history`, elimina el nuevo `current` de `items` y actualiza `revision`. `SetCurrent` se usa para “play now” y para el arranque de la jam.

### 5.4 Chunking

El relay actual rechaza mensajes > 4 KB. Una cola de 100 items no cabe. Solución:

- Cada item se serializa con strings truncados a 120 chars para el wire (la UI puede mostrar el título completo localmente, pero el wire usa el truncado).
- `queue_snapshot` se parte en chunks **por presupuesto de bytes**, no por número fijo: el emisor acumula items hasta que el JSON supera 3.500 bytes y cierra el chunk. Un chunk puede llevar 25 items cortos o 8 largos.
- `chunks` se calcula tras el troceado; todos comparten `snapshot_id` y `revision`.
- El receptor acumula chunks; al completar, reemplaza su espejo. Si falta un chunk en 5 s, pide resync.
- Test obligatorio: el peor caso de chunk serializado debe medir < 3.800 bytes, con títulos/artistas de 120 chars y UTF-8 multibyte.

Alternativa descartada: subir el límite del relay a 64 KB. Es más simple, pero rompe compatibilidad con relays viejos y aumenta la superficie de abuso. Se puede permitir como feature opcional del relay Rust, pero el protocolo base usa chunking.

### 5.5 Revisiones y convergencia

```
host: add item -> revision 42 -> queue_patch(42)
guest A: last=41 -> apply -> last=42
guest B: last=40 -> gap -> queue_resync(40)
host: queue_snapshot(revision=42, chunks=2)
guest B: replace mirror -> last=42
```

- No se usa CRDT: el host es secuencial y el orden de WebSocket es FIFO por conexión.
- Los comandos concurrentes se serializan en el host; el orden de llegada decide. No hay relojes distribuidos.
- `op_id` (UUID v4 generado por el invitado) permite reintentar sin duplicar. El host guarda los últimos 64 op_ids por participante.
- `base_rev` permite rechazar una operación calculada sobre una vista vieja (por ejemplo, mover un item que ya no existe) sin bloquear operaciones válidas.
- Si el host consume un item mientras un invitado lo mueve, gana el host: el patch incluye `Remove` del consumido y el `Move` se rechaza con ack `stale_item`.

### 5.6 Sincronía de playback

Se mantiene el `sync_state` actual, con dos mejoras:

1. Se envía **inmediatamente** en `track_change`, no solo cada 2 s.
2. Se añade `queue_revision` para que el invitado sepa si su cola está al día; no es imprescindible para reproducir.

El invitado sigue aplicando la misma lógica: si el `track_uri` difiere, `play_uri`; si la posición difiere > umbral, `seek`. El umbral se baja de 3 s a 2 s (configurable) y se compensa el RTT con `timestamp` del host. No se intenta sincronía de muestra (imposible sin control del reloj de audio); el objetivo es “suficientemente sincronizado”.

---

## 6. Flujos

### 6.1 Unirse y recibir estado

```
Guest                          Relay                         Host
  │ join v=2, name               │                             │
  ├─────────────────────────────►│                             │
  │                              ├─ joined_room(you, members) ─►│
  │                              │                             │
  │◄─ joined_room(you, members) ─┤                             │
  │                              │                             │
  │                              │◄──── queue_snapshot ────────┤
  │◄──── queue_snapshot ─────────┤                             │
  │                              │◄──── sync_state ────────────┤
  │◄──── sync_state ─────────────┤                             │
  │ apply snapshot + seek        │                             │
```

El host envía el snapshot al recibir `guest_joined`, sin esperar el tick de 2 s. Si el snapshot es grande, los chunks van seguidos; el relay los reenvía en orden.

### 6.2 Añadir un track

```
Guest                       Relay                        Host
  │ queue_command(add)        │                            │
  ├──────────────────────────►│                            │
  │                           ├─ queue_command(from) ─────►│
  │                           │                            │ valida:
  │                           │                            │  uri, límites,
  │                           │                            │  permisos, duplicado
  │                           │                            │ aplica:
  │                           │                            │  revision++
  │                           │◄──── queue_ack(ok) ────────┤
  │◄──── queue_ack(ok) ───────┤                            │
  │                           │◄──── queue_patch ──────────┤
  │◄──── queue_patch ─────────┤                            │
  │ muestra item              │                            │
```

Si falla, el host envía `queue_ack { ok: false, error: "queue_full" }` y **no** difunde patch. El invitado muestra el error y no hace cambios optimistas.

### 6.3 El host reproduce un item

```
Host: Enter sobre item #3
  → PartyQueue.set_current(id)   // el current anterior va a history
  → native_queue reconstruida sin el item elegido
  → dispatch(PlayQueueItem)
  → revision++
  → queue_patch(set_current)
  → sync_state inmediato
  → invitados: play_uri(track) + seek
```

### 6.4 Consumo natural

Cuando el motor nativo termina un track y avanza:

```
AdvanceNativeQueue → consume head de native_queue
  → hook: PartyQueue.advance()
      current anterior → history
      head de items → current
  → revision++
  → queue_patch(advance)
  → sync_state(track nuevo)
```

Si la cola queda vacía:

- `stop_when_empty = true` → pausa y `sync_state(is_playing=false)`.
- `stop_when_empty = false` → se reanuda el contexto suspendido (comportamiento actual del motor). Los invitados siguen el nuevo track del contexto aunque no esté en la cola; la UI muestra “fuera de cola” y `current = None`.

---

### 6.5 Modos de playback

La cola es independiente de quién reproduce. El protocolo separa `queue_mode` (quién modifica la cola) de `playback_mode`:

| Modo | Quién reproduce | Requisitos | Uso |
|---|---|---|---|
| `follow_host` (MVP) | Cada participante, en su dispositivo, siguiendo al host | spotatui + `yt-dlp` | Sala remota/privada; reutiliza Listening Party |
| `host_only` (P1) | Solo el host, en su altavoz | Host Premium; invitados pueden no tener cuenta si usan web | Sala física/jukebox; no retransmite audio |

En `host_only`, los invitados **no reciben audio**: si están en la misma habitación, oyen el altavoz del host; si están remotos, solo ven y controlan la cola. Esto evita el webcasting de III.4 y reduce la exposición. El invitado web sin cuenta es P1; en la TUI, un invitado con cuenta Free puede unirse si el login y la búsqueda funcionan sin reproducción (a validar en Fase 0).

El modo se negocia en `room_created`/`joined_room` y se muestra en la cabecera. El host puede cambiarlo solo al crear la sala en v1.

## 7. Integración en spotatui

### 7.1 Núcleo

| Archivo | Cambio |
|---|---|
| `src/infra/network/sync.rs` | Añadir variantes v2 a `SyncMessage`; `PartySession` con `members: Vec<PartyMember>`, `queue_revision`, `features`; helpers de chunking/validación |
| `src/infra/network/mod.rs` | Manejar `queue_command`, `queue_ack`, `queue_patch`, `queue_snapshot`, `queue_resync`; enviar snapshot al `guest_joined`; hook de `track_change` |
| `src/core/app/party.rs` | `PartyQueue`/`PartyMirror`; métodos `apply_queue_command`, `apply_patch`, `request_resync`, `party_queue_add_local`; permisos |
| `src/core/app/queue.rs` | Hook de consumo (`AdvanceNativeQueue` → `party_queue_pop`); materializar la ventana de `native_queue`; bloquear adds que no sean YouTube durante la jam; reconciliación |
| `src/core/action/mod.rs` | Nuevas acciones: `PartyAddTrack`, `PartyRemoveTrack`, `PartyMoveTrack`, `PartyPlayNow`, `PartyVote`, `PartySetQueueMode` |
| `src/tui/handlers/party.rs` | Navegación de la pantalla, atajos, búsqueda, confirmaciones |
| `src/tui/ui/party.rs` (nuevo) | Layout de cola/miembros/ajustes |
| `src/tui/keymap.rs`, `src/core/user_config.rs` | Teclas `party_add`, `party_remove`, `party_move_up/down`, `party_play_now`, `party_vote`, `party_members`; defaults sin colisión |
| `src/core/i18n/*` | Strings nuevas en en/zh_hans/zh_hant (o fallback en) |
| `docs/listening-party.md` | Documentar cola, permisos, límites y relay v2 |
| `worker-relay/src/index.ts` | Pasar `queue_*`; ids de participante; rate limit por conexión; validar dirección host/guest; límite de tamaño |
| `src/gui/protocol.rs`, `gui/src/Party.tsx` | Paridad de la página Party del desktop (P1; el protocolo es el mismo) |

### 7.2 Búsqueda para añadir

El invitado puede buscar en YouTube con la fuente activa. Opciones:

1. **Reutilizar la pantalla Search existente** con un modo “party add”. Al pulsar Enter sobre un track, en vez de reproducir, envía `QueueAdd`.
2. **Overlay dedicado** dentro de Party. Es más limpio y evita perder el contexto de la cola.

Recomendación: overlay dedicado que reutiliza el motor de búsqueda (`get_search_results` / IoEvent de búsqueda) y muestra resultados con un flag `party_add_mode`. No se abre la pantalla Search global.

### 7.3 UI

- `RouteId::Party` ya existe. La v1 puede mantener el popup para crear/unirse y abrir una **pantalla completa** al estar dentro de una sala.
- La lista de cola es un `Table`/`List` con selección; el now playing arriba; miembros en un panel lateral si hay ancho, o tab.
- Los items se renderizan con `Span`/`Line`; no se interpreta Markdown ni secuencias de escape.
- Snapshot tests con `ratatui::backend::TestBackend` + `insta` para 80×24, 120×40 y 200×60.
- La pantalla debe ser usable sin ratón.

### 7.4 Keybindings y configuración

```yaml
keys:
  party_add: a
  party_remove: x
  party_move_up: K
  party_move_down: J
  party_play_now: enter
  party_vote: v
  party_members: m

behavior:
  party_queue_mode: guests_add      # host_only | guests_add | guests_manage
  party_queue_max: 100
  party_queue_per_guest: 5
  party_queue_dedupe: true
  party_queue_allow_explicit: true
  party_queue_stop_when_empty: false
  party_sync_threshold_ms: 2000
```

Todas las teclas deben pasar por el sistema de keymap existente y aparecer en `docs/keybindings.md`.

---

## 8. Alternativas evaluadas

### Variante A — Fork core (recomendada)

**Pros:** reutiliza todo, experiencia integrada, upstream posible, tests existentes, un solo binario.
**Contras:** hay que tocar código core de un proyecto grande y activo; riesgo de conflicto en rebase.

### Variante B — Jukebox con invitados sin cuenta

El host reproduce; los invitados usan una web mínima para ver/añadir. Requiere:

- Servir una web desde el host (axum) o desde el relay.
- Autenticación de invitados (código + cookie), no Spotify.
- Búsqueda usando la cuenta del host (una sola sesión, rate limits compartidos).
- No hay sync de audio para invitados remotos; solo control. El audio suena en el altavoz del host; no se retransmite.

**Pros:** abre el producto a quien no tiene spotatui/Premium; no hay varios streams; es el modo más limpio para una sala física.
**Contras:** segundo frontend, segundo modelo de auth, más superficie de seguridad; no es “una extensión TUI” para el invitado.
**Decisión:** P1/P2; la cola se diseña para soportarlo, pero no entra en el MVP.

### Variante C — Plugin Lua + sidecar

Un plugin registra una pantalla `plugin:jam`, usa `set_interval` y `http_*` para hablar con un sidecar/relay HTTP, y controla playback con `play_uri`/`seek`/`add_to_queue`.

**Pros:** no toca el core; se instala con `spotatui plugin add`; ideal para prototipar.
**Contras:** sin WebSocket (solo polling HTTP), sin quitar/reordenar la cola nativa, timers limitados al tick de UI (≤ 999 ms), sin acceso al `SyncMessage` existente, y la sincronía sería más frágil.
**Decisión:** prototipo de validación si el fork se atasca; no es la arquitectura final.

### Variante D — Puente con la Jam nativa

Usar `spotify_cli` del cliente de escritorio (macOS) o endpoints internos para `create_jam`/`join`/`get_jam_status`.

**Pros:** audio oficialmente sincronizado, sin relay propio, sin que cada invitado tenga spotatui.
**Contras:** no documentado, confirmado solo en macOS/Windows (Linux sin confirmar), sin SDK Rust, sin subcomando `join`, frágil ante updates, riesgo de ToS.
**Decisión:** experimental P2 detrás de un flag, nunca dependencia del núcleo.

### Matriz de decisión

| Criterio | Fork core | Jukebox web | Plugin + sidecar | Jam nativa |
|---|---|---|---|---|
| Es una extensión TUI | ✅ | ❌ | ✅ | ❌ |
| Cola colaborativa real | ✅ | ✅ | ⚠️ limitada | ✅ |
| Sincronía de playback | ✅ reutiliza | ❌ | ⚠️ polling | ✅ oficial |
| Invitados sin cuenta | ❌ | ✅ | ❌ | ✅ |
| Esfuerzo | Medio | Alto | Medio | Alto y frágil |
| Riesgo legal | Bajo | Medio | Bajo | Alto |
| Upstream posible | ✅ | ❌ | ✅ | ❌ |
| **Veredicto** | **Sí** | P2 | Prototipo | No |

---

## 9. Seguridad y privacidad

### Modelo de amenaza (v1)

| Amenaza | Mitigación |
|---|---|
| Código de sala brute-force | 36^6 ≈ 2.2e9; rate limit 10 joins/h/IP; sala efímera; sin datos sensibles |
| Invitado malicioso envía comandos no permitidos | Host valida permisos y límites; relay solo reenvía; ack de error |
| Spoofing de `from` | El relay añade `from` y descarta el que envíe el cliente; el host usa el del relay |
| Inyección de metadata (títulos con ANSI/control chars) | Sanitizar: quitar `\x00-\x1f`, limitar longitud, renderizar como texto |
| URIs maliciosas / SSRF | Solo `youtube:<id>`; no se hacen requests a URLs de invitados |
| Flood de comandos | Token bucket en relay (10 burst, 2/s) + rate limit en host por participante |
| Mensajes gigantes | Límite 4 KB del relay; chunking; rechazar > 4 KB |
| Suplantación del host | El relay solo acepta un host por sala y por IP; el host no acepta comandos de otro host |
| Relay ve la actividad | No hay tokens ni audio; solo URIs, nombres y cola; self-host disponible; no logs por defecto |
| Invitado abre múltiples conexiones | Límite 2 por IP; el host puede limitar por nombre; no hay auth fuerte en v1 |
| Sala secuestrada al caer el host | La sala se cierra; no hay takeover. Documentado |

### Datos que viajan

- Código de sala, nombres de participantes, URIs de tracks, títulos/artistas, posiciones, estado de playback.
- **Nunca** tokens de Spotify, credenciales, email, historial completo ni audio.

### Privacidad

- El relay público no debe persistir la cola ni los nombres más allá de la sesión.
- El Worker ya usa Durable Objects en memoria; no hay storage de mensajes. Mantener esa propiedad.
- Documentar en la guía de auto-hosting que el operador del relay ve los metadatos de la sala.

---

## 10. Compatibilidad y migración

| Combinación | Resultado esperado |
|---|---|
| Cliente 0.48 + relay v2 | Listening Party clásico; los mensajes `queue_*` se ignoran |
| Cliente nuevo + relay 0.48 | La sala funciona; la UI indica “cola no soportada por el relay” |
| Host 0.48 + invitado nuevo | Sin cola; playback normal |
| Host nuevo + invitado 0.48 | El invitado ignora `queue_*`; playback normal; el host ve al invitado sin cola |
| Host nuevo + relay v2 + invitado nuevo | Jam completa |

Estrategia:

1. Desplegar el relay v2 en una URL nueva (`wss://party-v2...`) y probarlo con clientes nuevos.
2. Mantener el relay v1 para clientes antiguos.
3. Cambiar el default en una release menor, con fallback automático si el relay no responde.
4. Documentar el flag `behavior.relay_server_url` para self-hosting.

No se cambia el formato de `sync_state` ni se elimina ningún mensaje existente. Las variantes nuevas son aditivas.

---

## 11. Límites y rendimiento

| Recurso | Límite v1 | Razón |
|---|---|---|
| Participantes | 8 | Relay actual; fans-out manejable |
| Items de cola | 100 | UI y tamaño de snapshot |
| Tamaño de mensaje | 4 KB | Relay actual |
| Snapshot | chunks por presupuesto de 3.500 B | 100 items ≈ 3–6 chunks |
| Frecuencia de sync | 2 s + inmediato en track change | Ya existente |
| Frecuencia de patches | Coalescing 100 ms | Evitar floods |
| Comandos por invitado | 10 burst, 2/s | Anti-abuso |
| Nombres | 32 chars | Ya existente |
| Títulos/artistas | 200 chars (120 en wire) | UI y tamaño |
| Duración de track | 1 ms–60 min | Evitar basura |

Carga estimada: 8 invitados × 1 patch cada 2 s = 4 msg/s; snapshot de 3–6 chunks al entrar; irrelevante para un Worker o un axum local.

---

## 12. Preguntas abiertas

1. ¿El host puede usar un dispositivo Connect externo? La v1 lo desaconseja porque no puede reordenar la cola remota; se puede permitir en modo “solo añadir al final” con limitaciones.
2. ¿Qué pasa si dos invitados añaden el mismo URI a la vez? `dedupe` rechaza el segundo; el ack explica.
3. ¿Se permite añadir episodios? No en v1; solo tracks.
4. ¿La cola sobrevive a un reconnect del invitado? Sí, pide snapshot; el host no cambia.
5. ¿La cola sobrevive a un reconnect del host? No en v1; P1 con gracia de 60 s.
6. ¿Se puede tener más de un host? No; una sala, un host.
7. ¿Se puede cambiar de host? No en v1; habría que transferir la sala y el estado.
8. ¿La cola se persiste en disco? No; es estado de sesión. Al cerrar la sala se pierde, salvo export P1.
9. ¿Se muestran los tracks del contexto de Spotify que no están en la cola? No; la UI puede mostrar “a continuación: contexto del host” sin listarlos.
10. ¿Se sincroniza la cola si el host está en pausa? Sí; la cola es independiente del estado de playback.

---

## 13. Diagrama de estados de la sala

```
                 create
   Disconnected ─────────► Hosting ◄────┐
        │                    │          │
        │ join               │ leave    │ reconnect (P1)
        ▼                    ▼          │
     Joined ─────────────► Disconnected
        │
        │ host_closed / error
        ▼
     Disconnected
```

Estados de la cola en el host:

```
Empty ──add──► NonEmpty ──consume──► NonEmpty
  ▲               │                     │
  └──clear────────┴──────consume last───┘
```

Estados de la réplica:

```
Synced ──patch contiguous──► Synced
  │                             │
  │ gap                         │ resync
  ▼                             ▼
Resyncing ──snapshot complete──► Synced
```
