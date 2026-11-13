# 08 — Backlog

Convenciones:

- **ID:** `PQ-###` (Party Queue).
- **Prioridad:** Must (MVP), Should (MVP si hay tiempo), Could (P1), Won't (P2/no-goal).
- **Estimación:** S (≤ 0,5 d), M (1–2 d), L (3–5 d), XL (> 5 d, dividir).
- **Estado:** `todo`, `doing`, `blocked`, `done`.
- **Labels:** `area:protocol`, `area:host`, `area:guest`, `area:relay`, `area:tui`, `area:docs`, `area:ci`, `kind:feature`, `kind:bug`, `kind:test`, `kind:spike`.

El MVP se congela en el gate G3. Todo lo que no esté en Must va a P1/P2.

---

## Épica E0 — Spikes y validación

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-001 | Reproducir Listening Party con 2 clientes | Host + guest con relay local; probar sync, control y límites | Notas con capturas; se observa sync y shared control | S | Must |
| PQ-002 | Medir límites del relay | 4 KB, 8 participantes, rate limits, inactividad | Tabla de límites verificada | S | Must |
| PQ-003 | Spike `queue_ping` | Añadir un mensaje nuevo al enum y al Worker; verificar old/new | Cliente viejo no se rompe; cliente nuevo lo recibe | M | Must |
| PQ-004 | Spike `native_queue` | Leer/añadir/quitar/avanzar; hook de consumo | Se puede mutar sin divergencia en una prueba manual | L | Must |
| PQ-005 | Metadata de búsqueda en guest | Obtener URI, título, artistas y duración | Datos suficientes para `QueueAdd` | S | Must |
| PQ-006 | Revisión de ToS | Caso de uso vs Developer Policy | Nota con riesgos y mitigaciones | S | Must |
| PQ-007 | Contacto con maintainer | Issue/PR draft con design doc | Respuesta o fecha límite registrada | S | Must |
| PQ-008 | ADR de decisión | Fork vs plugin vs upstream | ADR-0005 actualizado | S | Must |

**Gate G0:** PQ-003 y PQ-004 verdes; PQ-006 sin bloqueo; PQ-007 contactado.

---

## Épica E1 — Protocolo y modelo de cola

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-010 | Tipos `PartyQueue` | Item, `current`, `history`, config, revisión, IDs | Compila; serde estable | M | Must |
| PQ-011 | `QueueAction`/`QueueOp` | Add/Remove/Move/Clear/PlayNow | Roundtrip JSON | M | Must |
| PQ-012 | Validación de acciones | URI, límites, permisos, duplicados, cooldown | Tests de cada rechazo | M | Must |
| PQ-013 | `PartyMirror` | Apply patch/snapshot; gap detection | Convergencia en tests | M | Must |
| PQ-014 | Chunking | Snapshot troceado y reensamblado | Peor chunk < 3.800 B | M | Must |
| PQ-015 | Idempotencia | `op_id` por participante | Reintento no duplica | S | Must |
| PQ-016 | Property tests | Secuencias aleatorias host/mirror | 10k ops, 0 divergencias | M | Must |
| PQ-017 | Golden fixtures | JSON de todos los mensajes | Parseo + roundtrip | S | Must |
| PQ-018 | Fuzz parser | `parse_sync_message` | 1 h sin crash | M | Must |
| PQ-019 | Spec de protocolo | `docs/protocolo-party-v2.md` | Revisada contra código | M | Must |

**Gate G1:** tests de cola verdes; property y fuzz sin fallos; compatibilidad de parseo.

---

## Épica E2 — Cola del host

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-020 | Integrar `PartyQueue` en `App` | Estado, config, revisión | Inicializa/limpia correctamente | M | Must |
| PQ-021 | Derivar `native_queue` | Rebuild en cada mutación | Invariante 5 | M | Must |
| PQ-022 | Hook de consumo | `AdvanceNativeQueue` → `advance` (current→history) | No quedan fantasmas ni current duplicado | M | Must |
| PQ-023 | Add/remove/move/clear local | Acciones del host | Funcionan con/sin party | L | Must |
| PQ-024 | Play now | Mover al head y reproducir | Suena y se difunde | M | Must |
| PQ-025 | Adoptar cola existente | Al iniciar la jam | Se adopta o vacía según config | S | Must |
| PQ-026 | Rechazar fuentes no Spotify | Local/YouTube/Qobuz/Subsonic | Error claro; no entra | S | Must |
| PQ-027 | Broadcast snapshot/patch/ack | Con coalescing 100 ms | Cliente de test recibe todo | L | Must |
| PQ-028 | Sync inmediato en track change | No esperar 2 s | Guest cambia en < 2 s | M | Must |
| PQ-029 | Tests de host | Unitarios + integración | Cobertura > 80 % | M | Must |

**Gate G2:** host + relay + sim de 8 invitados sin divergencia.

---

## Épica E3 — Relay v2

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-030 | Participantes con ID | Asignar UUID y propagar | `joined_room`/`guest_joined` con id | M | Must |
| PQ-031 | Rutar `queue_*` | Pasar snapshot/patch/ack/command/resync | Roundtrip completo | M | Must |
| PQ-032 | Validar dirección | Host solo emite snapshot/patch; guest solo command | Tests de abuso | S | Must |
| PQ-033 | Rate limit de comandos | Token bucket por conexión | Flood rechazado sin panic | M | Must |
| PQ-034 | Tamaño y chunk safety | 4 KB; rechazo limpio | Test 5 KB | S | Must |
| PQ-035 | Compatibilidad v1/v2 | `protocol_version`/`features` | Matriz 4 combinaciones | M | Must |
| PQ-036 | Relay de test local | `wrangler dev` o axum | `just relay-dev` < 5 s | S | Must |
| PQ-037 | Tests de integración | 2 clientes WebSocket | 10 escenarios | M | Must |
| PQ-038 | Deploy staging | Worker v2 en URL staging | Smoke test | S | Must |
| PQ-039 | Kill switch | `QUEUE_ENABLED=0` | Degrada a playback | S | Should |

---

## Épica E4 — Integración del invitado

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-040 | `PartyMirror` en `App` | Estado del guest | Se actualiza con patch/snapshot | M | Must |
| PQ-041 | Aplicar snapshot al unirse | Pedir y recibir | Cola visible al entrar | M | Must |
| PQ-042 | Enviar `QueueAdd` | Con metadata y op_id | Host lo recibe y ack | M | Must |
| PQ-043 | Recibir ack/errores | Mostrar feedback | Sin acciones silenciosas | S | Must |
| PQ-044 | Resync tras gap | `queue_resync` | Converge en < 5 s | M | Must |
| PQ-045 | Remove propio | Si el modo lo permite | Solo sus items | S | Must |
| PQ-046 | Vote (P1) | -1/0/+1 | Host agrega y muestra | M | Could |
| PQ-047 | Reconexión guest | Rejoin manual | Snapshot correcto | M | Should |
| PQ-048 | Tests de guest | Unitarios + integración | Cobertura > 80 % | M | Must |

---

## Épica E5 — UI TUI

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-050 | Pantalla Party completa | Cola + now playing + miembros | Snapshot 80×24/120×40/200×60 | L | Must |
| PQ-051 | Selección y scroll | j/k, PageUp/Down, Home/End | Funciona con 100 items | M | Must |
| PQ-052 | Overlay de búsqueda | Input, resultados, Enter | Añadir en < 10 s | L | Must |
| PQ-053 | Acciones de host | x, J/K, Enter, clear | Keymap configurable | M | Must |
| PQ-054 | Permisos en UI | Ocultar/deshabilitar según rol | Sin acciones inválidas | S | Must |
| PQ-055 | Estados de error | Relay, sala llena, sin cola | Mensajes accionables | M | Must |
| PQ-056 | Miembros | Lista con rol y estado | 8 miembros legibles | S | Should |
| PQ-057 | Configuración de sala | Modo, límites, stop when empty | Persistida en config | M | Should |
| PQ-058 | i18n | Strings en en/zh | Sin hardcode | M | Must |
| PQ-059 | Docs de teclas | `docs/keybindings.md` | Actualizado | S | Must |
| PQ-060 | Snapshots UI | `insta` | 10 escenarios | M | Must |
| PQ-061 | Accesibilidad | 80 cols, sin color-only | Checklist | S | Must |
| PQ-062 | Paridad GUI/desktop | Página Party con cola y acciones | Misma semántica que TUI | M | Could |

---

## Épica E6 — Calidad y CI

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-070 | `party-sim` | Host + N guests + mock playback | 8 guests, 10k ops | L | Could |
| PQ-071 | Chaos tests | Latencia, churn, duplicates, patch loss | Sin divergencia | M | Must |
| PQ-072 | Compat matrix CI | Old/new clients | 4 combinaciones verdes | M | Must |
| PQ-073 | Workflow PR | Jobs de party | Bloqueante | M | Must |
| PQ-074 | Nightly fuzz/soak | 1 h + 1 h | Verdes 7 días | M | Must |
| PQ-075 | Load test relay | 100 salas × 8 | p95 < 200 ms | M | Should |
| PQ-076 | Security checklist | Revisión manual | 0 altos | S | Must |
| PQ-077 | Coverage | `llvm-cov` | > 70 % global, > 80 % party | S | Should |

---

## Épica E7 — Documentación y release

| ID | Título | Descripción | Criterio de aceptación | Est. | Prioridad |
|---|---|---|---|---|---|
| PQ-080 | Guía de usuario | `docs/party-queue.md` | Crea/une/añade/moderar | M | Must |
| PQ-081 | Guía self-hosting | Worker + Rust + TLS | Probada en limpio | M | Must |
| PQ-082 | Troubleshooting | Errores comunes | 10 casos | S | Must |
| PQ-083 | CHANGELOG | Release notes | `v0.49.0-jam.1` | S | Must |
| PQ-084 | Build matrix | Linux/macOS/Windows | Artefactos | M | Must |
| PQ-085 | Firma/SBOM | cosign + sha256 | Verificable | S | Should |
| PQ-086 | Smoke post-release | Instalar y probar | Verde | S | Must |
| PQ-087 | PR upstream | Draft + commits | Abierto | M | Must |

---

## Épica E8 — P1

| ID | Título | Descripción | Prioridad |
|---|---|---|---|
| PQ-100 | Votación y orden por score | Agregar votos, modo de orden | Should |
| PQ-101 | Aprobación previa | Cola de pending y approve/reject | Could |
| PQ-102 | Exportar a playlist | Al cerrar la sala | Could |
| PQ-103 | Relay Rust + Docker | Self-host completo | Should |
| PQ-104 | Invitado web de solo lectura | Ver cola/now playing | Could |
| PQ-105 | Invitado web para añadir | Sin cuenta, search proxy | Could |
| PQ-106 | Reconexión del host | Gracia 60 s, token de host | Should |
| PQ-107 | Historial de sala | Tracks reproducidos | Could |
| PQ-108 | Chat de sala | Mensajes efímeros | Won't (v1) |
| PQ-109 | QR de invitación | En terminal | Could |
| PQ-110 | Modo jukebox host-only | Sin playback en guests | Should |

---

## Épica E9 — P2 / exploratorio

| ID | Título | Descripción | Prioridad |
|---|---|---|---|
| PQ-120 | Puente `spotify_cli` | macOS/Windows, flag experimental | Won't |
| PQ-121 | GraphQL interno | Prohibido por Terms | Won't |
| PQ-122 | Sync fino < 100 ms | Requiere control de reloj | Won't |
| PQ-123 | Fuentes no Spotify | Requiere misma fuente en todos | Won't |
| PQ-124 | App web de host | Segundo frontend | Won't |
| PQ-125 | Múltiples hosts / transferencia | Complejo | Won't |

---

## 10 primeros issues (orden de ejecución)

1. **PQ-001** — Reproducir party con 2 clientes.
2. **PQ-002** — Medir límites del relay.
3. **PQ-003** — Spike mensaje nuevo.
4. **PQ-004** — Spike `native_queue`.
5. **PQ-010** — Tipos de cola.
6. **PQ-011** — Acciones/op.
7. **PQ-012** — Validación.
8. **PQ-013** — Mirror.
9. **PQ-016** — Property tests.
10. **PQ-020** — Integrar en `App`.

---

## Grafo de dependencias (MVP)

```
PQ-001/002/003/004
        │
        ▼
PQ-010 → PQ-011 → PQ-012 → PQ-016 → PQ-020 → PQ-023 → PQ-027
   │                 │                 │                 │
   └──► PQ-013 ──────┘                 │                 ▼
        │                             │              PQ-040 → PQ-042
        ▼                             ▼                 │
     PQ-014/015                  PQ-022/024             ▼
        │                             │              PQ-050/052
        └──────────────► PQ-030/031 ──┘                 │
                              │                         ▼
                              ▼                    PQ-054/055
                          PQ-037/038                    │
                              │                         ▼
                              └──────────────────► PQ-070/071 → release
```

---

## Criterios de “listo para PR”

- La historia tiene tests que fallan sin el cambio.
- El cambio está detrás de una rama y no mezcla capas.
- Si toca protocolo, hay fixture y nota de compatibilidad.
- Si toca UI, hay snapshot y captura.
- Si toca relay, hay test de integración y plan de rollback.
- La documentación y el CHANGELOG se actualizan en el mismo PR o en uno inmediato.

---

## Estimación agregada

| Épica | Días | Semanas |
|---|---|---|
| E0 | 5–7 | 1 |
| E1 | 8–10 | 2 |
| E2 | 5–7 | 1 |
| E3 | 5–7 | 1 |
| E4 | 4–5 | 1 |
| E5 | 7–9 | 1.5 |
| E6 | 4–5 | transversal |
| E7 | 4–5 | 1 |
| **Total** | **42–55** | **7–9** |

P1 añade 3–5 semanas; no se estima hasta cerrar el MVP.
