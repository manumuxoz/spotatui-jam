# Checklist — Fase 0

Objetivo: validar en 5–7 días que la arquitectura elegida es viable antes de escribir la feature. Cada casilla debe tener evidencia (comando, captura, log o nota), no solo “hecho”.

Fecha de inicio: __________
Fecha objetivo: __________
Responsable: __________

---

## 0. Preparación

- [ ] Clonar spotatui en una ruta de trabajo:
  ```bash
  git clone https://github.com/LargeModGames/spotatui.git
  cd spotatui
  git checkout v0.48.0
  ```
- [ ] Verificar toolchain: `rustc --version` ≥ 1.90, `cargo --version`.
- [ ] Instalar dependencias de audio/TUI del README.
- [ ] Compilar la feature set de fiesta:
  ```bash
  cargo build --locked --no-default-features \
    --features telemetry,tui,streaming,mcp-server
  ```
- [ ] Instalar `wrangler`/Node para el relay:
  ```bash
  cd worker-relay && npm ci
  ```
- [ ] Tener 2 cuentas Spotify Premium (host y guest) y 2 terminales.
- [ ] Anotar versiones: spotatui, Rust, Node, wrangler, SO.

**Evidencia:** salida de `spotatui --version`, `rustc --version`, `node --version`.

---

## 1. Reproducir Listening Party (PQ-001)

- [ ] Arrancar el relay local:
  ```bash
  cd worker-relay
  npx wrangler dev --local --port 8787
  ```
- [ ] Configurar el cliente host:
  ```yaml
  behavior:
    relay_server_url: "ws://127.0.0.1:8787/ws"
  ```
  (En local se permite `ws://`; en producción debe ser `wss://`.)
- [ ] Host: `Ctrl+P` → crear sala.
- [ ] Guest: `Ctrl+P` → unirse con código y nombre.
- [ ] Verificar:
  - [ ] El guest cambia de track cuando el host cambia.
  - [ ] El guest corrige el drift al hacer seek.
  - [ ] `shared_control`: el guest puede play/pause/skip.
  - [ ] Al salir el host, el guest recibe `room_closed`.
- [ ] Medir el drift percibido en 3 cambios de track.
- [ ] Anotar limitaciones observadas.

**Evidencia:** captura de las dos terminales, log del relay, nota de drift.

---

## 2. Medir límites del relay (PQ-002)

- [ ] Confirmar 8 participantes máximos (unirse con 9).
- [ ] Confirmar 4 KB por mensaje: enviar un JSON de 3 KB, 4 KB y 5 KB; observar.
- [ ] Confirmar rate limit de creación/unión por IP.
- [ ] Confirmar expiración por inactividad (5 min).
- [ ] Confirmar que el host no puede duplicarse.
- [ ] Documentar en una tabla: límite, valor, cómo se probó.

**Evidencia:** tabla en `research/notes/relay-limits.md` o en el issue.

---

## 3. Spike de mensaje nuevo (PQ-003)

- [ ] Añadir una variante temporal `QueuePing { n: u64 }` a `SyncMessage`.
- [ ] Añadir su reenvío en `worker-relay/src/index.ts` (solo host → guests).
- [ ] Enviar desde el host cada 2 s.
- [ ] Verificar:
  - [ ] El cliente nuevo lo parsea.
  - [ ] El cliente 0.48 no se rompe (lo ignora y sigue).
  - [ ] El relay viejo lo descarta sin cerrar la conexión.
- [ ] Revertir el spike o convertirlo en el primer commit de la feature.

**Evidencia:** diff, log de ambos clientes.

---

## 4. Spike de `native_queue` (PQ-004)

- [ ] Localizar `App.native_queue`, `add_track_to_native_queue`, `remove_queue_item`, `move_queue_item`, `AdvanceNativeQueue`.
- [ ] Añadir un track Spotify a la cola nativa y verificar que suena al terminar el contexto.
- [ ] Quitarlo antes de que suene.
- [ ] Moverlo de posición.
- [ ] Verificar qué pasa al consumirlo: ¿se elimina de `native_queue`? ¿Dónde?
- [ ] Identificar el punto exacto de hook para `PartyQueue::pop_head`.
- [ ] Escribir un test que simule add → consume → cola vacía.
- [ ] Evaluar si la derivación `native_queue = PartyQueue.items` es viable o si hay mutaciones externas que la rompen.

**Evidencia:** notas con rutas/líneas, test, diff.

**Si falla:** activar plan B (cola virtual solo display) o plan C (plugin). Actualizar ADR-0002.

---

## 5. Metadata de búsqueda (PQ-005)

- [ ] Buscar un track desde una sesión de invitado.
- [ ] Verificar que se obtienen `uri`, `name`, `artists`, `duration_ms`.
- [ ] Verificar que el URI es `spotify:track:<base62>`.
- [ ] Verificar que el host puede resolver el URI sin volver a buscar.
- [ ] Probar un track no disponible en la región del guest.
- [ ] Probar un episodio (debe rechazarse).

**Evidencia:** JSON de ejemplo, nota de limitaciones.

---

## 6. Política y ToS (PQ-006)

- [ ] Leer Developer Terms y Developer Policy (versiones vigentes).
- [ ] Revisar III.4 (webcasting), III.11 (experiencia core), IV.1 (Premium), IV.2 (comercial).
- [ ] Decidir el modo de playback por defecto para uso personal y para publicación.
- [ ] Documentar riesgos y mitigaciones en `docs/06-riesgos.md`.
- [ ] Si se planea distribución pública, consultar a alguien con criterio legal.

**Evidencia:** nota firmada con fecha y enlaces.

**No es asesoría legal.** Es una revisión de riesgos.

---

## 7. Contacto con el maintainer (PQ-007)

- [ ] Escribir un design doc de 1 página con:
  - problema;
  - propuesta (cola host-autoritativa, protocolo aditivo);
  - alcance del PR;
  - compatibilidad;
  - preguntas abiertas.
- [ ] Abrir un issue/discusión en el repo de spotatui (no un PR de código todavía).
- [ ] Registrar la fecha y la respuesta.
- [ ] Si no hay respuesta en 3 semanas, decidir fork según ADR-0005.

**Evidencia:** URL del issue, fecha, respuesta.

---

## 8. Decisión final (PQ-008)

- [ ] Revisar los resultados de los spikes.
- [ ] Actualizar ADR-0001/0002/0005/0006 si algo cambió.
- [ ] Confirmar o recortar el MVP.
- [ ] Estimar de nuevo con los datos reales.
- [ ] Escribir la decisión en `plan/decision-fase-0.md`:
  ```markdown
  # Decisión Fase 0
  Fecha:
  Arquitectura elegida:
  Evidencia:
  Riesgos aceptados:
  Cambios al plan:
  ```

**Gate G0:** los spikes 3 y 4 funcionan; la política no bloquea el uso previsto; el maintainer está contactado; la decisión está escrita.

---

## Timebox

| Día | Foco |
|---|---|
| 1 | Preparación + reproducir party |
| 2 | Límites del relay + spike de mensaje |
| 3 | Spike `native_queue` |
| 4 | Metadata + política |
| 5 | Contacto maintainer + decisión |
| 6–7 | Buffer |

Si el día 3 el spike de `native_queue` no está claro, **no seguir**; evaluar plan B/C.

---

## Señales de alarma

- El relay no reenvía mensajes nuevos sin cambios profundos.
- `native_queue` se muta en muchos sitios y no se puede derivar.
- El guest no puede obtener metadata fiable.
- El maintainer veta explícitamente la feature.
- La política se interpreta como bloqueo para el uso previsto.

Cada señal se documenta y se lleva al gate G0; no se ignora.
