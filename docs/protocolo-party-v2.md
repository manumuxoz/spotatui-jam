# Protocolo Party Queue v2

Especificación de los mensajes que extienden Listening Party. Es aditiva: ningún mensaje v1 cambia de forma o de significado. El relay v2 los reenvía; el host es la autoridad de la cola.

- **Versión:** 2
- **Transporte:** WebSocket, mensajes de texto JSON, uno por frame.
- **Límite duro:** 4.096 bytes por mensaje en el relay de referencia. El emisor debe medir el JSON serializado antes de enviar.
- **Codificación:** UTF-8. Los strings de metadata se truncan a 120 caracteres en el wire.
- **Compatibilidad:** clientes 0.48 ignoran los tipos desconocidos y siguen con el playback.

---

## 1. Handshake

En `create`/`join` se añaden parámetros a la URL:

```
wss://relay.example.com/ws?action=create&v=2&features=queue&name=manu
wss://relay.example.com/ws?action=join&code=ABC123&v=2&features=queue&name=ana
```

El relay responde con los campos nuevos (los clientes viejos los ignoran):

```json
{
  "type": "room_created",
  "code": "ABC123",
  "control_mode": "host_only",
  "protocol_version": 2,
  "features": ["queue"],
  "playback_mode": "follow_host",
  "you": { "id": "host", "name": "manu", "role": "host" },
  "queue_revision": 0
}
```

```json
{
  "type": "joined_room",
  "host_name": "manu",
  "control_mode": "host_only",
  "protocol_version": 2,
  "features": ["queue"],
  "playback_mode": "follow_host",
  "you": { "id": "p-7f3a", "name": "ana", "role": "guest" },
  "queue_revision": 42
}
```

Reglas:

- `protocol_version` ausente ⇒ relay v1 ⇒ cola no disponible.
- `features` ausente ⇒ no se envía `queue_*`.
- `queue_revision` es una pista para el late join; el invitado siempre pide snapshot.
- `playback_mode`: `follow_host` o `host_only`.

---

## 2. Item de cola

```json
{
  "id": 17,
  "uri": "spotify:track:7o2AeQZzfCERsRmOM86EcB",
  "title": "Archangel",
  "artists": ["Burial"],
  "duration_ms": 299000,
  "added_by": "p-7f3a",
  "added_by_name": "ana",
  "votes": 0,
  "added_at_ms": 1762891200000
}
```

| Campo | Tipo | Reglas |
|---|---|---|
| `id` | u64 | Único en la sala, asignado por el host, estable |
| `uri` | string | Solo `spotify:track:<22 base62>` en v1 |
| `title` | string | 1–120 chars en wire; se trunca con `…` |
| `artists` | string[] | 1–8; cada uno ≤ 120 chars |
| `duration_ms` | u32 | 1–3.600.000 |
| `added_by` | string | `participant_id` del relay, no del cliente |
| `added_by_name` | string | Snapshot del nombre, ≤ 32 chars |
| `votes` | i32 | 0 en MVP; P1 |
| `added_at_ms` | u64 | Unix ms del host |

No se envía `image_url`, `album`, `artist_refs` ni el `TrackInfo` completo: cada cliente resuelve la portada si la necesita.

---

## 3. Mensajes

### 3.1 `party_config`

Host → todos. Se envía al crear, al cambiar y al entrar un invitado.

```json
{
  "type": "party_config",
  "config": {
    "queue_mode": "guests_add",
    "playback_mode": "follow_host",
    "max_items": 100,
    "max_per_guest": 5,
    "add_cooldown_ms": 2000,
    "dedupe": true,
    "allow_explicit": true,
    "stop_when_empty": false,
    "votes_enabled": false
  }
}
```

`queue_mode`: `host_only`, `guests_add`, `guests_manage`.

### 3.2 `party_members`

Host → todos. Lista completa; se reemplaza, no se mergea.

```json
{
  "type": "party_members",
  "members": [
    { "id": "host", "name": "manu", "role": "host" },
    { "id": "p-7f3a", "name": "ana", "role": "guest" }
  ]
}
```

### 3.3 `queue_snapshot`

Host → todos o a uno. Estado completo troceado.

```json
{
  "type": "queue_snapshot",
  "revision": 42,
  "snapshot_id": "s-9c1e",
  "chunk": 0,
  "chunks": 2,
  "to": "p-7f3a",
  "current": { "id": 16, "uri": "spotify:track:...", "title": "Archangel", "artists": ["Burial"], "duration_ms": 299000, "added_by": "host", "added_by_name": "manu", "votes": 0, "added_at_ms": 1762891000000 },
  "items": [],
  "history_tail": []
}
```

- `snapshot_id` identifica el conjunto; todos los chunks lo comparten.
- `chunk` es 0-based; `chunks` es el total.
- `current` es el track sonando (o `null`); no está en `items`.
- `items` son los próximos, en orden.
- `history_tail` es opcional (últimos 20) y se envía solo al hacer late join; no es necesario para reproducir.
- `to` opcional: si existe, el relay lo entrega solo a ese participante; si no, broadcast.
- El receptor descarta un snapshot de `revision` menor o igual al actual.
- Si falta un chunk a los 5 s, envía `queue_resync`.

### 3.4 `queue_patch`

Host → todos. Delta desde `revision - 1`.

```json
{
  "type": "queue_patch",
  "revision": 43,
  "ops": [
    { "op": "add", "item": { "...": "..." } },
    { "op": "remove", "id": 15 },
    { "op": "move", "id": 17, "after": 14 },
    { "op": "advance", "current": { "...": "..." }, "played_id": 16, "skipped": false },
    { "op": "set_current", "current": { "...": "..." }, "played_id": 16, "skipped": true },
    { "op": "clear" }
  ]
}
```

Reglas:

- `revision` debe ser exactamente `last_revision + 1`; si no, el invitado pide resync.
- `add` inserta al final (el host decide la posición; el patch no necesita índice).
- `move` usa `after`; `null` = al principio.
- `advance` es atómico: el `current` anterior se mueve a `history` (con `skipped`), el nuevo `current` sale de `items`.
- `set_current` se usa para “play now”: el `current` anterior va a `history`, el elegido sale de `items` y pasa a `current`.
- `clear` deja `items` vacío; no afecta a `current`.
- `reset` es interno y no se usa en operación normal.

### 3.5 `queue_command`

Invitado → host. El relay añade `from` y descarta el que envíe el cliente.

```json
{
  "type": "queue_command",
  "op_id": "3f1c9a0e-...",
  "from": "p-7f3a",
  "base_rev": 42,
  "action": {
    "op": "add",
    "uri": "spotify:track:...",
    "title": "Archangel",
    "artists": ["Burial"],
    "duration_ms": 299000
  }
}
```

Acciones:

| `op` | Campos | Permiso |
|---|---|---|
| `add` | `uri`, `title`, `artists`, `duration_ms` | `guests_add` o `guests_manage` |
| `remove` | `id` | Host, o autor si `guests_manage` |
| `move` | `id`, `after` | Host, o `guests_manage` |
| `clear` | — | Host |
| `play_now` | `id` | Host, o `shared_control` |
| `vote` | `id`, `value` (-1/0/1) | P1 |

`remove`, `move` y `vote` solo aplican a items de `items`; para quitar el track sonando se usa `next`/`skip`, no `remove`.

`op_id` es UUID v4; el host lo usa para idempotencia. `base_rev` es la revisión que el cliente conocía.

### 3.6 `queue_ack`

Host → autor. Resultado de una acción.

```json
{
  "type": "queue_ack",
  "op_id": "3f1c9a0e-...",
  "ok": true,
  "revision": 43,
  "to": "p-7f3a"
}
```

```json
{
  "type": "queue_ack",
  "op_id": "3f1c9a0e-...",
  "ok": false,
  "error": "queue_full",
  "message": "La cola está llena (100)",
  "to": "p-7f3a"
}
```

Códigos de error:

| Código | Significado |
|---|---|
| `forbidden` | El rol no permite la acción |
| `queue_full` | `max_items` alcanzado |
| `guest_limit` | `max_per_guest` alcanzado |
| `rate_limited` | Cooldown o token bucket |
| `duplicate` | URI ya presente y `dedupe=true` |
| `invalid_uri` | No es `spotify:track:` válido |
| `invalid_metadata` | Título/artistas/duración fuera de rango |
| `not_playable` | El host no puede reproducirlo (opcional) |
| `unknown_item` | `id` no existe |
| `stale_item` | El item cambió o se consumió |
| `stale_rev` | `base_rev` demasiado antiguo para una operación segura |
| `internal` | Error del host; se loguea |

### 3.7 `queue_resync`

Invitado → host. Petición de snapshot.

```json
{ "type": "queue_resync", "have_revision": 40 }
```

El host responde con `queue_snapshot` dirigido (`to`) y la revisión actual.

### 3.8 `sync_state` (v1, sin cambios)

```json
{
  "type": "sync_state",
  "track_uri": "spotify:track:...",
  "position_ms": 42000,
  "is_playing": true,
  "timestamp": 1762891200000
}
```

Se sigue enviando cada 2 s y de inmediato al cambiar de track. No incluye la cola.

---

## 4. Chunking

Objetivo: ningún frame > 3.800 bytes, incluso con UTF-8 multibyte.

Algoritmo del emisor:

```text
budget = 3500
items = cola ordenada
chunks = []
actual = []
size = overhead_base
for item in items:
    s = json(item)
    if actual no está vacío and size + len(s) + 1 > budget:
        chunks.push(actual); actual = []; size = overhead_base
    actual.push(item); size += len(s) + 1
chunks.push(actual)
```

- `overhead_base` incluye `type`, `revision`, `snapshot_id`, `chunk`, `chunks`, `to` y el array.
- Si un solo item excede el budget, se rechaza (no debería ocurrir con los límites de metadata).
- El receptor valida `chunks <= 50` y `items_por_chunk <= 100` para evitar allocaciones abusivas.
- Test de peor caso: 100 items con títulos/artistas de 120 chars, emojis y CJK; cada chunk < 3.800 B.

---

## 5. Orden y convergencia

```
Host: revision 42
  │
  ├─ queue_command(add) de ana (base_rev 42)
  │     ├─ valida
  │     ├─ aplica: revision 43
  │     ├─ queue_ack(ok, rev 43) a ana
  │     └─ queue_patch(rev 43) a todos
  │
  ├─ queue_command(move) de luis (base_rev 42)
  │     ├─ valida contra rev 43
  │     ├─ si `after` ya no existe → ack(stale_item)
  │     └─ si es válido → revision 44, patch
```

- El host serializa las operaciones en el orden de llegada.
- Un `queue_patch` siempre se aplica sobre `revision - 1`.
- Si un invitado detecta un gap, no aplica el patch; pide `queue_resync`.
- El host conserva los últimos 64 `op_id` por participante; un duplicado devuelve el ack original sin reaplicar.
- No hay relojes distribuidos ni CRDT.

---

## 6. Validación en el host

Orden de comprobaciones para `add`:

1. `queue_mode` permite add.
2. `op_id` no procesado (idempotencia).
3. `uri` cumple `spotify:track:<22 base62>`; no `episode`, `local`, `album`, URL.
4. `title` 1–200 chars; `artists` 1–8 × 200; `duration_ms` 1–3.600.000.
5. Cola < `max_items`.
6. Items del participante < `max_per_guest`.
7. Cooldown desde el último add.
8. Si `dedupe`, el URI no está en la cola.
9. Si `allow_explicit=false`, el track no es explícito (si se conoce).
10. Aplicar, asignar `id`, `added_by`, `added_at_ms`, incrementar `revision`.

Si falla, `queue_ack` con el código y sin patch.

Validación de strings:

- Quitar `\x00-\x1f` y `\x7f`.
- Normalizar espacios.
- Truncar a 120 chars en wire, 200 en UI.
- No interpretar Markdown, ANSI ni HTML.

---

## 7. Seguridad del relay

- El relay asigna `participant_id` al aceptar la conexión y lo incluye en `you`.
- En `queue_command`, sobrescribe `from` con el id de la sesión; descarta el del cliente.
- Solo el host puede enviar `queue_snapshot`, `queue_patch`, `party_config`, `party_members`.
- Solo los invitados pueden enviar `queue_command` y `queue_resync`.
- Si `queue_snapshot.to` existe, se entrega solo a esa sesión; si no existe la sesión, se descarta.
- Rate limit por conexión: 10 comandos de burst, 2/s de recarga.
- Límite de tamaño: 4.096 bytes; si se supera, se descarta y se envía `error` (sin cerrar la conexión).
- El relay no valida permisos de cola; el host es la autoridad. El relay solo impone dirección y forma.

---

## 8. Compatibilidad

| Emisor | Receptor | Resultado |
|---|---|---|
| Host v1 | Guest v2 | Sin `features`; guest muestra “cola no disponible” |
| Host v2 | Guest v1 | Guest ignora `queue_*`; playback sigue |
| Relay v1 | Host v2 | `queue_*` se descartan; la UI se desactiva |
| Relay v2 | Host v1 | Los mensajes v1 pasan sin cambios |
| Host v2 | Relay v2 | Cola completa |

Reglas:

- Los campos nuevos llevan `#[serde(default)]`.
- No se renombran campos existentes.
- No se reutilizan tipos existentes para semánticas nuevas.
- El relay v2 debe poder desplegarse antes que el cliente.
- `protocol_version` y `features` son la fuente de verdad para activar la UI.

---

## 9. Test vectors

### 9.1 Add aceptado

```json
{"type":"queue_command","op_id":"a1","from":"p-1","base_rev":0,"action":{"op":"add","uri":"spotify:track:7o2AeQZzfCERsRmOM86EcB","title":"Archangel","artists":["Burial"],"duration_ms":299000}}
```

Respuesta:

```json
{"type":"queue_ack","op_id":"a1","ok":true,"revision":1,"to":"p-1"}
{"type":"queue_patch","revision":1,"ops":[{"op":"add","item":{"id":1,"uri":"spotify:track:7o2AeQZzfCERsRmOM86EcB","title":"Archangel","artists":["Burial"],"duration_ms":299000,"added_by":"p-1","added_by_name":"ana","votes":0,"added_at_ms":1762891200000}}]}
```

### 9.2 Add duplicado

```json
{"type":"queue_ack","op_id":"a2","ok":false,"error":"duplicate","message":"Ya está en la cola","to":"p-1"}
```

### 9.3 Gap y resync

Invitado en `rev 40`, recibe patch `rev 42`:

```json
{"type":"queue_resync","have_revision":40}
```

Host:

```json
{"type":"queue_snapshot","revision":42,"snapshot_id":"s-1","chunk":0,"chunks":1,"to":"p-1","current":null,"items":[],"history_tail":[]}
```

### 9.4 Move con `after`

```json
{"type":"queue_patch","revision":44,"ops":[{"op":"move","id":17,"after":14}]}
```

`after: null` mueve al principio.

### 9.5 Avance natural

```json
{"type":"queue_patch","revision":45,"ops":[{"op":"advance","current":{"id":18,"uri":"spotify:track:...","title":"Halcyon","artists":["Jon Hopkins"],"duration_ms":300000,"added_by":"host","added_by_name":"manu","votes":0,"added_at_ms":1762891300000},"played_id":17,"skipped":false}]}
```

### 9.6 Error de permisos

```json
{"type":"queue_ack","op_id":"a3","ok":false,"error":"forbidden","message":"Solo el host puede vaciar la cola","to":"p-2"}
```

---

## 10. Cambios futuros

- Votos: `vote` y `queue_reorder` por ranking (P1).
- Aprobación: `queue_pending`, `queue_approve`, `queue_reject` (P1).
- Historial: `queue_history` (P1).
- Multi-host: requeriría un protocolo de elección; fuera de alcance.
- Relay Rust: debe pasar los mismos test vectors y la matriz de compatibilidad.
