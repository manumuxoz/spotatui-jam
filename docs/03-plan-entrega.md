# 03 — Plan de entrega

## 1. Resumen

| Dato | Valor |
|---|---|
| Alcance | MVP de cola colaborativa dentro de Listening Party |
| Duración estimada | 7–9 semanas a tiempo completo (1 dev) |
| Equipo mínimo | 1 dev Rust/TUI + apoyo de QA manual |
| Camino crítico | protocolo → cola host → relay → integración invitado → UI → hardening |
| Primer entregable usable | Semana 4 (alpha interna, LAN) |
| Release pública | Semana 8–9 (`v0.49.0-jam.1` o PR upstream) |
| Riesgo principal | Que el upstream no acepte el parche y haya que mantener un fork |

La estimación asume familiaridad con Rust y con el código de spotatui. Si se parte de cero, añadir 1–2 semanas de ramp-up.

---

## 2. Estrategia de entrega

### Principios

1. **Vertical slices:** cada fase termina con algo demostrable, no con “infraestructura a medias”.
2. **Host primero, invitado después:** la cola autoritativa es el corazón; sin ella, la UI del invitado no tiene sentido.
3. **Relay local desde el día 1:** no depender del Worker público para desarrollar; un relay de test permite iterar rápido.
4. **Compatibilidad continua:** cada PR debe pasar la matriz old/new; no romper Listening Party clásico.
5. **Upstream-first:** el diseño y los commits se preparan para un PR revisable; el fork es el fallback, no el objetivo.

### Flujo de trabajo

```
Issue (backlog) → rama corta → PR pequeño → CI → review → squash a main
                                                      │
                                                      ├─ nightly: fuzz/soak/load
                                                      └─ tag: release + relay
```

- Ramas: `feat/party-queue-*`, `fix/party-*`, `docs/party-*`, `relay/*`.
- Commits convencionales (`feat(party): ...`, `fix(relay): ...`).
- Un PR = una historia o un cambio de protocolo; no mezclar refactor y feature.
- Todo PR de protocolo incluye: tipos, tests golden, actualización de la doc y compatibilidad.

---

## 3. Fases

### Fase 0 — Validación y diseño (semana 0–1)

**Objetivo:** eliminar las incógnitas que pueden invalidar el plan.

| # | Tarea | Entregable | Criterio de salida |
|---|---|---|---|
| 0.1 | Reproducir Listening Party con 2 instancias y relay local | Notas + capturas | Se observa sync y control compartido |
| 0.2 | Medir límites reales del relay (4 KB, 8 participantes, rate limits) | Informe | Confirmado con un test |
| 0.3 | Spike: añadir un mensaje `queue_ping` al protocolo y al relay | Rama desechable | Un cliente nuevo lo recibe; uno viejo no se rompe |
| 0.4 | Spike: leer/escribir `native_queue` y hook de consumo | Rama desechable | Se puede añadir, quitar y avanzar sin divergencia |
| 0.5 | Validar búsqueda y metadata de un track en el invitado | Notas | Se obtiene URI + título + artista + duración |
| 0.6 | Revisar ToS/Developer Policy con el caso de uso | Nota legal | No hay bloqueo evidente; riesgos documentados |
| 0.7 | Contactar al maintainer de spotatui con un design doc breve | Issue/PR draft | Feedback o silencio con fecha límite |
| 0.8 | Decidir: fork vs plugin vs upstream | ADR-0005 actualizado | Decisión firmada |

**Gate G0:** los spikes 0.3 y 0.4 funcionan; el maintainer no ha vetado la idea; el caso de uso no viola ToS de forma evidente. Si falla 0.4, replantear la autoridad de la cola (ver riesgo R5).

**Duración:** 5–7 días. **Buffer:** 2 días.

---

### Fase 1 — Protocolo y dominio (semana 1–3)

**Objetivo:** cola autoritativa y réplica convergentes, sin UI ni relay real.

| # | Tarea | Entregable | Criterio de salida |
|---|---|---|---|
| 1.1 | Tipos `PartyQueue`, `PartyQueueItem`, `PartyQueueConfig`, `PartyAction`, `QueueOp` | Crate/módulo | Compila; serialización estable |
| 1.2 | `apply_action` con validación (URI, límites, permisos, duplicados, rate) | Métodos + tests | 100 % de ramas cubiertas |
| 1.3 | `PartyMirror::apply_patch` / `apply_snapshot` / `needs_resync` | Métodos + tests | Convergencia tras secuencias aleatorias |
| 1.4 | Chunking de snapshot y test de tamaño máximo | Helpers | Peor chunk < 3.800 bytes |
| 1.5 | Idempotencia por `op_id` | Estructura + tests | Reintento no duplica |
| 1.6 | Property tests: ops aleatorias host↔mirror | `proptest` | 10.000 secuencias sin divergencia |
| 1.7 | Golden JSON fixtures de todos los mensajes | `tests/fixtures/party/*.json` | Parseo y roundtrip |
| 1.8 | Fuzz del parser de mensajes | `cargo-fuzz` target | 1 h sin panic |
| 1.9 | Documento de protocolo v2 | `docs/protocolo-party-v2.md` | Revisado contra la implementación |

**Gate G1:** `cargo test party` verde; property tests y fuzz sin fallos; snapshots < 4 KB; compatibilidad de parseo con mensajes v1.

**Duración:** 10–12 días. **Buffer:** 2 días.

---

### Fase 2 — Host y relay (semana 3–5)

**Objetivo:** una sala real donde el host crea, añade y difunde la cola; un cliente de test la recibe.

| # | Tarea | Entregable | Criterio de salida |
|---|---|---|---|
| 2.1 | Integrar `PartyQueue` en `App`; derivar `native_queue` | Código | Invariantes 1–10 de arquitectura |
| 2.2 | Hook de consumo en `AdvanceNativeQueue` | Código + tests | Consumir no deja items fantasma |
| 2.3 | Comandos locales del host (add/remove/move/clear/play now) | Acciones | Funcionan sin party y con party |
| 2.4 | Broadcast de snapshot/patch/ack; coalescing | Red | Un cliente de test recibe todo |
| 2.5 | Relay v2: pasar `queue_*`, ids, rate limit, dirección | Worker | Tests de integración |
| 2.6 | Relay de test local (wrangler dev o axum mínimo) | Script | `just relay-dev` levanta en < 5 s |
| 2.7 | `party-sim`: host + N invitados simulados | Binario | 8 invitados, 100 ops, sin divergencia |
| 2.8 | Matriz de compatibilidad old/new | Tests | 4 combinaciones documentadas |
| 2.9 | Métricas de latencia en el simulador | Informe | p95 add→patch < 100 ms local |

**Gate G2:** el simulador completa una sesión de 30 min con 8 invitados y 100 items sin divergencia ni panic; el relay v2 pasa los tests; los clientes viejos siguen funcionando.

**Duración:** 10–12 días. **Buffer:** 2 días.

---

### Fase 3 — Invitado y UI (semana 5–7)

**Objetivo:** una jam usable por humanos en terminal.

| # | Tarea | Entregable | Criterio de salida |
|---|---|---|---|
| 3.1 | `PartyMirror` en `App`; aplicar snapshot/patch; resync | Código | Convergencia con host real |
| 3.2 | Pantalla completa Party: cola, now playing, miembros | UI | Snapshot tests 80×24/120×40/200×60 |
| 3.3 | Overlay de búsqueda y add | UI | Añadir un track en < 10 s |
| 3.4 | Acciones de host: remove/move/clear/play now | UI + keymap | Sin conflictos de teclas |
| 3.5 | Permisos y límites en UI; estados de error | UI | Mensajes claros |
| 3.6 | Ack/errores y feedback | UI | No hay acciones silenciosas |
| 3.7 | i18n y docs de teclas | Strings | en/zh al menos; docs actualizadas |
| 3.8 | Prueba manual con 2–3 personas | Informe | 30 min sin bloqueos |
| 3.9 | Accesibilidad: 80 columnas, sin color-only | Revisión | Checklist |

**Gate G3:** dos personas completan una jam de 30 min en LAN; pueden añadir, quitar, reordenar y saltar; no hay crash; la UI es legible a 80×24.

**Duración:** 10–12 días. **Buffer:** 3 días.

---

### Fase 4 — Hardening y release (semana 7–9)

**Objetivo:** release pública, documentada y operable.

| # | Tarea | Entregable | Criterio de salida |
|---|---|---|---|
| 4.1 | Soak test 1 h, 8 invitados, red con latencia | Informe | Sin leaks ni divergencia |
| 4.2 | Load test del relay (100 salas, 8 clientes) | Informe | CPU/latencia aceptables |
| 4.3 | Security review: comandos, tamaños, spoofing | Checklist | 0 hallazgos altos |
| 4.4 | Fuzz continuo en nightly | CI | 7 días sin fallos |
| 4.5 | Docs: usuario, self-hosting, troubleshooting | `docs/` | Revisadas |
| 4.6 | CHANGELOG y release notes | Release | `v0.49.0-jam.1` |
| 4.7 | Build matrix y artefactos | CI | Linux/macOS/Windows |
| 4.8 | Relay staging + smoke test | Deploy | Crea/une/añade/salta |
| 4.9 | Relay producción + rollback probado | Deploy | Rollback en < 5 min |
| 4.10 | PR upstream (si aplica) | PR | Abierto y revisable |

**Gate G4:** release instalable, relay en producción, smoke test verde, rollback probado, docs completas, 0 blockers.

**Duración:** 10–12 días. **Buffer:** 3 días.

---

## 4. Cronograma

```
Semana      0   1   2   3   4   5   6   7   8   9
Fase 0     ███
Fase 1       ███████████
Fase 2               ███████████
Fase 3                       ███████████
Fase 4                               ███████████
Gates      G0          G1          G2          G3          G4
```

Hitos:

| Hito | Fecha relativa | Qué se puede mostrar |
|---|---|---|
| M0 | fin semana 1 | Spikes + decisión de arquitectura |
| M1 | fin semana 3 | Cola host + réplica convergen, sin UI |
| M2 | fin semana 5 | Sala real con cliente de test |
| M3 | fin semana 7 | Jam usable por 2–3 personas |
| M4 | fin semana 9 | Release pública |

---

## 5. MVP fast path (4 semanas)

Si se quiere validar antes de invertir 9 semanas:

- **Semana 1:** tipos + cola host + tests (sin votos, sin chunking fino; snapshots de ≤ 40 items en un mensaje).
- **Semana 2:** relay v2 + host integration + simulador de 2 clientes.
- **Semana 3:** UI mínima (lista + add + remove) + prueba con 2 personas.
- **Semana 4:** hardening + release alpha.

Recorta: votos, aprobación, export, relay Rust, membresía rica, i18n, reconexión. No recorta: validación, permisos, tests de convergencia, compatibilidad.

Riesgo del fast path: el snapshot de 40 items puede rozar 4 KB; usar 20 items o chunking básico.

---

## 6. Dependencias y camino crítico

```
Protocolo (1.1–1.4)
   │
   ├──► Cola host (2.1–2.3)
   │        │
   │        ├──► Relay v2 (2.5)
   │        │        │
   │        │        └──► Integración invitado (3.1)
   │        │                 │
   │        │                 └──► UI (3.2–3.6) ──► Release
   │        │
   │        └──► Simulador (2.7) ──► Soak (4.1)
   │
   └──► Compatibilidad (1.7, 2.8)
```

- El protocolo bloquea todo; por eso se cierra en Fase 1.
- La UI no bloquea el relay; se puede desarrollar en paralelo con datos mock.
- El soak y el load test necesitan el simulador; no dejarlos para el final.
- La decisión upstream (G0) condiciona el formato de release, no la arquitectura.

---

## 7. Estimación por historia (resumen)

| Épica | Esfuerzo | Semanas |
|---|---|---|
| E0 Spikes y validación | 5–7 d | 1 |
| E1 Protocolo | 8–10 d | 2 |
| E2 Cola host | 5–7 d | 1 |
| E3 Relay | 5–7 d | 1 |
| E4 Integración invitado | 4–5 d | 1 |
| E5 UI TUI | 7–9 d | 1.5 |
| E6 Testing/CI | 4–5 d (transversal) | — |
| E7 Docs/release | 4–5 d | 1 |
| **Total** | **42–55 d** | **7–9 sem** |

El detalle está en [`08-backlog.md`](08-backlog.md).

---

## 8. Definición de hecho (por historia)

Una historia está hecha cuando:

- [ ] El código compila en las features afectadas y en `--no-default-features`.
- [ ] Tiene tests unitarios y, si toca protocolo, golden tests.
- [ ] Pasa `cargo fmt --check`, `clippy -D warnings`, `cargo test`.
- [ ] Respeta la compatibilidad old/new documentada.
- [ ] Tiene strings i18n y entrada en docs/CHANGELOG si es user-facing.
- [ ] No introduce `unwrap()`/`expect()` en rutas de red o input no confiable.
- [ ] La UI tiene snapshot test si cambia layout.
- [ ] El PR tiene descripción, capturas/gif si es UI, y plan de rollback si toca relay.
- [ ] Se actualizó el ADR o el protocolo si cambió una decisión.

---

## 9. Riesgos de calendario

| Riesgo | Impacto | Buffer |
|---|---|---|
| El hook de `native_queue` es más invasivo de lo previsto | +1 sem | Fase 2 |
| El maintainer pide cambios grandes en el PR | +1 sem | Fase 4 |
| La sync de playback no baja de 3 s sin tocar librespot | No bloquea MVP | Aceptar 3 s |
| El relay Worker limita mensajes/rate de forma inesperada | +3 d | Fase 2 |
| Pruebas manuales con Spotify dependen de cuentas | +3 d | Fase 3 |
| Falta de tiempo | Recortar P1 | — |

---

## 10. Después del MVP

| Orden | Feature | Por qué |
|---|---|---|
| 1 | Votación y orden por score | Diferenciador social; bajo coste |
| 2 | Exportar cola a playlist | Cierra el ciclo de la sesión |
| 3 | Relay Rust + Docker | Self-hosting real; privacidad |
| 4 | Aprobación de tracks | Salas grandes/eventos |
| 5 | Invitado web de solo lectura | Reduce fricción sin reproductor |
| 6 | Paridad de la página Party del desktop/GUI | El protocolo ya está; falta UI |
| 7 | Reconexión del host | Robustez |
| 8 | Puente Jam nativa (experimental) | Solo si hay demanda y APIs estables |

Cada uno entra por el mismo pipeline; ninguno se implementa antes de que el MVP esté en `main` y probado.
