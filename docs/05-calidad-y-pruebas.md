# 05 — Calidad y pruebas

## 1. Pirámide

```
                 ┌─────────────────────┐
                 │ Manual con Spotify  │  pocas, reales, checklist
                 ├─────────────────────┤
                 │ E2E simulados       │  host + 8 invitados, relay local
                 ├─────────────────────┤
                 │ Integración relay   │  WebSocket, compatibilidad
                 ├─────────────────────┤
                 │ Property + fuzz     │  convergencia, parser
                 ├─────────────────────┤
                 │ Unitarios           │  cola, permisos, límites, chunking
                 └─────────────────────┘
```

Objetivo: que los bugs de estado y protocolo se detecten sin necesidad de dos cuentas de Spotify y oído humano. La prueba manual valida lo que ningún test puede: audio real, latencia de red real y usabilidad.

---

## 2. Unitarios

### 2.1 Cola

| Test | Caso |
|---|---|
| `add_appends_item` | Add válido al final; revisión +1 |
| `add_rejects_bad_uri` | `http://`, `spotify:album:`, vacío, `spotify:track:` sin id |
| `add_rejects_duplicate` | Mismo URI con `dedupe=true` |
| `add_allows_duplicate_when_disabled` | `dedupe=false` |
| `add_rejects_queue_full` | 100 items |
| `add_rejects_per_guest_limit` | 5 pendientes del mismo guest |
| `add_rejects_cooldown` | Segundo add antes de 2 s |
| `remove_by_owner` | Guest quita su item |
| `remove_by_other_guest_denied` | Sin permiso |
| `remove_by_host` | Host quita cualquier item |
| `move_updates_order` | Move a índice válido |
| `move_rejects_out_of_range` | `to >= len` |
| `move_rejects_stale_id` | Item consumido |
| `clear_only_host` | Guest no puede |
| `play_now_moves_to_front` | Orden + playback dispatch |
| `consume_head_moves_to_current` | Hook de avance: head → current, current anterior → history |
| `play_now_moves_current_to_history` | `set_current` no duplica el item |
| `remove_current_rejected` | El track sonando no se quita con remove |
| `revision_monotonic` | Secuencia de ops |

### 2.2 Permisos

Matriz rol × modo × acción:

| Rol | HostOnly | GuestsAdd | GuestsManage |
|---|---|---|---|
| Host | todo | todo | todo |
| Guest | nada de cola | add + remove propio | add + remove propio + move |
| Guest (control playback) | play/pause/skip | play/pause/skip | play/pause/skip |

Los tests deben cubrir cada celda y verificar el error exacto (`forbidden`, `queue_full`, etc.).

### 2.3 Metadata y sanitización

- Título con `\x1b[31m` → se elimina o se escapa; nunca se interpreta.
- Título de 10.000 chars → truncado a 200 (120 en wire).
- Artistas: máximo 8; cada uno truncado.
- Duración 0, negativa, > 60 min → rechazo.
- URI con espacios o mayúsculas → rechazo (IDs base62).
- `op_id` vacío, > 64 chars, con control chars → rechazo.

### 2.4 Chunking

- 0 items → un chunk vacío válido.
- 1 item → 1 chunk.
- 100 items → 3–6 chunks según el presupuesto de bytes; cada uno < 3.800 bytes.
- Título/artista en el límite superior → sigue cabiendo.
- Chunks fuera de orden → el receptor no aplica hasta completar.
- Chunk perdido → timeout de 5 s → resync.

---

## 3. Property-based testing

Con `proptest`:

```rust
proptest! {
    #[test]
    fn host_and_mirror_converge(ops in prop::collection::vec(queue_op_strategy(), 0..500)) {
        let mut host = PartyQueue::new(config());
        let mut mirror = PartyMirror::new();
        for op in ops {
            let patch = host.apply(op);
            mirror.apply(patch).unwrap();
        }
        prop_assert_eq!(host.snapshot(), mirror.snapshot());
    }
}
```

Propiedades:

1. **Convergencia:** tras cualquier secuencia de ops válidas, host y mirror tienen el mismo `(revision, items)`.
2. **Idempotencia:** aplicar el mismo `op_id` dos veces produce el mismo estado.
3. **Monotonicidad:** `revision` nunca decrece.
4. **Límites:** `items.len() <= max_items` siempre.
5. **Integridad:** todo item tiene URI Spotify válida y `added_by` no vacío.
6. **Resync:** si se descarta un patch, el mirror detecta el gap y converge tras snapshot.
7. **Consumo:** consumir el head nunca deja un item huérfano ni duplica el siguiente.

Semillas fijas en CI para reproducibilidad; el fallo imprime la secuencia mínima.

---

## 4. Fuzz

| Target | Input | Qué busca |
|---|---|---|
| `parse_sync_message` | bytes arbitrarios | panic, allocación desmedida, stack overflow |
| `queue_action` | JSON arbitrario | panic, campos faltantes, tipos raros |
| `apply_patch` | ops aleatorias con IDs inválidos | panic, índice fuera de rango |
| `relay_forward` | mensajes de hasta 4 KB | bypass de tamaño, panic |

Corpus inicial:

- Mensajes v1 reales (fixtures).
- Mensajes v2 válidos.
- Mensajes truncados a la mitad.
- JSON con Unicode, emojis, null bytes.
- `chunk` negativo, `chunks=0`, `revision=u64::MAX`.

En PR: 60 s por target. Nightly: 1 h y corpus persistido. Un crash genera un issue con el input minimizado.

---

## 5. Integración con relay

Se levanta un relay local:

- **Opción A:** `wrangler dev --local` (fiel al Worker, requiere Node).
- **Opción B:** `jam-relay` en modo test (Rust, rápido, sin Node).

Tests con `tokio-tungstenite`:

| Test | Escenario |
|---|---|
| `create_join_two_clients` | Host crea, guest une, ambos reciben `joined_room` |
| `queue_snapshot_on_join` | Guest recibe snapshot completo |
| `queue_command_roundtrip` | Add → host aplica → patch a todos → ack |
| `relay_drops_guest_snapshot` | Guest intenta enviar `queue_patch`; relay lo descarta |
| `relay_assigns_from` | El `from` del cliente se ignora; se usa el del relay |
| `oversized_message_rejected` | 5 KB → cierre o error, sin panic |
| `rate_limit` | 20 comandos en 1 s → algunos rechazados |
| `host_leave_closes_room` | `room_closed` a todos |
| `guest_leave_notifies` | `guest_left` con id |
| `reconnect_guest` | Guest vuelve, recibe snapshot actual |
| `relay_restart` | Las salas se pierden; los clientes muestran error |

Latencia: el relay local no debe añadir > 10 ms p95.

---

## 6. E2E simulados

La integración real host/guest se ejecuta con tests ignorados contra un relay vivo. Un simulador de 8 invitados sin audio (`party-sim`) queda como P1; los tests actuales cubren el camino completo con dos Apps y un relay real:

- Arranca un relay local (o se conecta a uno).
- Crea 1 host y N invitados.
- Cada participante tiene una máquina de estados real (`PartyQueue`/`PartyMirror`).
- Un `MockPlayback` simula el motor: consume items, cambia de track, emite eventos.
- Un `Chaos` inyecta: latencia, desconexión, reordenación de mensajes (no, WebSocket es FIFO), duplicados, resync.

Escenarios:

| Escenario | Config |
|---|---|
| `happy_path` | 8 guests, 1000 ops, 0 fallos |
| `guest_churn` | Guests entran/salen cada 10 s |
| `slow_guest` | Un guest con 500 ms de latencia |
| `queue_full` | 100 items, adds rechazados |
| `host_skip_storm` | Host salta 50 veces rápido |
| `patch_loss` | Se descarta 1 de cada 10 patches; resync |
| `duplicate_ops` | Cada comando se envía 3 veces |
| `long_session` | 1 h, 8 guests, 1 op/s |

Métricas que el simulador debe imprimir:

```
queue_divergence=0
patches_sent=...
resyncs=...
p50_add_to_patch_ms=...
p95_add_to_patch_ms=...
max_revision_gap=...
host_rss_mb=...
```

El test falla si `queue_divergence != 0`, si hay panic, o si p95 > 500 ms en local.

---

## 7. TUI

### 7.1 Snapshot tests

Con `ratatui::backend::TestBackend` e `insta`:

- 80×24 (mínimo), 120×40 (habitual), 200×60 (grande).
- Host con 0, 1 y 100 items.
- Guest en solo lectura.
- Cola con títulos largos, emojis, caracteres CJK.
- Estados: connecting, hosting, joined, reconnecting, closed, error.
- Overlay de búsqueda con 0, 1 y 20 resultados.
- Miembros con 2, 8 y nombres duplicados.

### 7.2 Tests de teclas

- Cada tecla documentada hace lo que dice.
- Las teclas globales (`q`, `?`, espacio, volumen) siguen funcionando fuera del input.
- `Esc` cierra el overlay antes de salir de la pantalla.
- `Enter` en búsqueda añade; en cola reproduce ahora (si permiso).
- No hay colisiones con keybindings existentes.

### 7.3 Accesibilidad

- La información no depende solo del color.
- Los iconos tienen texto alternativo en la misma línea.
- A 80 columnas no se corta información crítica (se truncA con `…`).
- El contraste cumple con el tema por defecto.
- Todo se puede hacer con teclado.

---

## 8. Pruebas manuales con Spotify

No se pueden automatizar en CI (auth + audio + cuentas). Checklist por release:

### Preparación

- [ ] 2 cuentas Spotify Premium (host e invitado), misma región.
- [ ] spotatui compilado con `streaming` y relay local.
- [ ] Dos terminales en la misma máquina o dos máquinas en LAN.
- [ ] Playlist de prueba con tracks de duración conocida.

### Casos

| # | Caso | Resultado esperado |
|---|---|---|
| M1 | Host crea sala | Código visible; guest une |
| M2 | Guest ve cola vacía | “La cola está vacía”; now playing correcto |
| M3 | Guest añade track | Aparece en < 1 s en ambos; suena tras el actual |
| M4 | Host quita track | Desaparece en ambos; no suena |
| M5 | Host reordena | Ambos ven el nuevo orden; suena en ese orden |
| M6 | Host “play now” | Track suena en ambos en < 2 s |
| M7 | Host salta | Ambos cambian de track; drift < 2 s |
| M8 | Guest con shared control salta | Host salta; ambos siguen |
| M9 | Guest sin permiso intenta quitar | Error visible; nada cambia |
| M10 | Cola llena | Add rechazado con mensaje |
| M11 | Guest se desconecta y vuelve | Snapshot correcto; playback continúa |
| M12 | Host cambia a local/YouTube | Sala sigue; aviso “fuente no soportada” |
| M13 | Host cierra sala | Guests ven “sala cerrada” |
| M14 | Relay caído | Guests muestran reconexión/error; no crash |
| M15 | 30 min de uso | Sin leaks, sin degradación, cola consistente |

### Medición de drift

En cada caso M7/M8, anotar:

- posición del host al cambiar;
- posición del guest tras 3 s;
- diferencia percibida (auricular o metrónomo visual).

Criterio: < 2 s p95; si se supera, revisar el umbral y la compensación de RTT.

---

## 9. Seguridad

| Prueba | Herramienta | Criterio |
|---|---|---|
| Dependencias vulnerables | `cargo audit` | 0 high/critical |
| Licencias | `cargo deny` | Compatibles MIT |
| Secretos en repo | `gitleaks`/`trufflehog` | 0 |
| Fuzz parser | `cargo-fuzz` | 0 crashes |
| Comandos no autorizados | tests de integración | 100 % rechazados |
| Spoofing `from` | test relay | Se ignora el cliente |
| Mensajes gigantes | test relay | Rechazo limpio |
| Inyección ANSI | test unitario | Sin escape |
| Rate limit | test de carga | Sin degradación |
| XSS (futuro web) | CSP + escape | 0 |

Revisión manual antes de release:

- [ ] ¿Se puede añadir una URI que no sea Spotify?
- [ ] ¿Se puede hacer que el host abra una URL?
- [ ] ¿Se puede inundar la cola o el relay?
- [ ] ¿Se puede suplantar a otro invitado?
- [ ] ¿Se filtran tokens en logs o mensajes?
- [ ] ¿El relay permite mensajes de host falsos?

---

## 10. Rendimiento

| Métrica | Objetivo | Cómo se mide |
|---|---|---|
| Add → patch visible | p95 < 500 ms LAN, < 1 s WAN | tests de integración, timestamp |
| Patch → UI render | < 16 ms | tick de 60 fps |
| Snapshot 100 items | < 100 ms | benchmark |
| Memoria host (100 items) | < 2 MB extra | RSS |
| CPU host (8 guests, 1 op/s) | < 5 % | `top` |
| Relay 100 salas × 8 | CPU < 50 % de 1 vCPU | load test |
| Drift de playback | p95 < 2 s | manual + logs |
| Reconexión guest | < 5 s | test |

Benchmarks con `criterion` para serialización, chunking y apply; no para playback.

---

## 11. Criterios de aceptación del MVP

1. Dos usuarios completan una jam de 30 min con 20+ tracks sin reiniciar.
2. El invitado puede añadir, ver, quitar su track y (si se permite) reordenar.
3. El host puede moderar y limitar.
4. La cola converge tras 100 ops aleatorias y tras un resync.
5. Un cliente 0.48 sigue funcionando contra el relay v2.
6. El relay rechaza comandos no autorizados y mensajes > 4 KB.
7. La UI es usable a 80×24 y tiene snapshots.
8. `cargo test`, `clippy`, `fmt`, `audit` y `deny` verdes.
9. Soak de 1 h sin divergencia ni leaks.
10. Documentación de usuario y self-hosting publicada.
