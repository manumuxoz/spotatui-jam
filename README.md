# spotatui-jam

Extensión de [spotatui](https://github.com/LargeModGames/spotatui) para convertir su **Listening Party** en una jam real: reproducción sincronizada **y una cola compartida** que todos los participantes pueden ver y modificar.

> **Estado:** Fase 0 — investigación y planificación. Este repositorio contiene el plan, la arquitectura, el pipeline y el backlog; todavía no hay código de producción.

---

## Veredicto rápido

| Pregunta | Respuesta |
|---|---|
| ¿Existe ya Spotify Jam? | Sí, como función de los clientes oficiales de Spotify. **No tiene API pública documentada** para crear/unirse/gestionar una Jam. |
| ¿Existe ya una extensión así para spotatui? | **No.** spotatui 0.48 ya tiene *Listening Party*: sincroniza playback entre host e invitados por WebSocket, con código de 6 caracteres y control compartido de transporte. Pero **no comparte la cola**. |
| ¿Hay proyectos de cola compartida? | Sí, pero son web/jukebox (OpenDJ, Spartify, Jammix, crowdroom, collabqueue, EchoNest). Ninguno es una extensión TUI de spotatui. |
| ¿Se puede integrar la Jam nativa de Spotify? | Solo por vías no documentadas (`spotify_cli` del cliente de escritorio, endpoints internos). No es viable como base del producto. |
| ¿Cuál es la vía recomendada? | **Extender el protocolo de Listening Party** con una cola autoritativa del host, en un fork upstream-first de spotatui. |

La conclusión completa, con fuentes y fechas, está en [`research/00-sintesis.md`](research/00-sintesis.md).

---

## Qué construimos

**spotatui-jam** convierte el Listening Party en una jam de terminal:

- El host crea una sala y comparte un código de 6 caracteres.
- Todos los participantes escuchan el mismo track, en la misma posición (best-effort, como ya hace Listening Party).
- El host mantiene la **cola compartida autoritativa**; los invitados la ven en tiempo real.
- Los invitados pueden buscar en Spotify y **añadir tracks a la cola**; según permisos, también quitarlos, reordenarlos o votarlos.
- El host modera: límites por invitado, tamaño máximo, filtro de explícitos, aprobación y expulsión.
- La sala y el relay son **self-hostables**; no se depende de una nube propietaria.

Dos modos de playback, misma cola:

| Modo | Quién escucha | Requisitos |
|---|---|---|
| **Follow-host** (MVP) | Cada participante en su dispositivo, sincronizado con el host | spotatui + Spotify Premium para todos |
| **Host-only** (P1) | Solo el host; los invitados controlan la cola | Host Premium; invitados sin cuenta posibles vía web |

### Lo que no es

- No es un cliente de la Jam oficial de Spotify ni promete interoperar con ella.
- No retransmite audio: en follow-host cada participante reproduce desde su propia cuenta; en host-only suena el altavoz del host y los invitados no reciben stream.
- No es un servicio de webcasting ni un producto comercial.

### Nota de política

La Jam oficial no tiene API. El modo follow-host se parece a la Jam de Spotify y puede leerse como una réplica de una experiencia core (Developer Policy III.11); el modo host-only evita la retransmisión, pero sigue siendo un proyecto no autorizado por Spotify. El plan asume **uso personal/self-hosted**, sin monetización ni servicio público, y recomienda revisión legal antes de una distribución amplia. Ver [`docs/06-riesgos.md`](docs/06-riesgos.md).

---

## Decisiones de arquitectura (resumen)

1. **Base:** fork de spotatui, no un daemon paralelo. Listening Party ya resuelve auth, relay, presencia y sincronía de playback; el gap es la cola.
2. **Autoridad:** el host es la única fuente de verdad de la cola. Los invitados envían comandos; el host valida, aplica y difunde un `revision` monotónico.
3. **Playback:** se reutiliza el modelo actual (cada participante reproduce el track del host en su cuenta). La cola se implementa sobre `native_queue: Vec<TrackInfo>` y se restringe a tracks de Spotify en la v1.
4. **Protocolo:** `SyncMessage` v2 con snapshots troceados y deltas; compatible con clientes antiguos. Mensajes pequeños para respetar el límite actual de 4 KB del relay.
5. **Relay:** se actualiza el Worker de Cloudflare incluido en spotatui y se añade un relay Rust auto-hospedable como alternativa. El relay sigue siendo tonto: no guarda la cola, solo enruta.
6. **Jam nativa:** fuera del núcleo. Cualquier puente con `spotify_cli` va detrás de un flag experimental y no se promete soporte.
7. **Upstream-first:** PR pequeño y revisable contra spotatui; si no se acepta, fork mantenido con rebase. La licencia MIT lo permite.

Detalle y alternativas descartadas en [`docs/02-arquitectura.md`](docs/02-arquitectura.md) y [`docs/07-adr/`](docs/07-adr/).

---

## Estructura del repositorio

```
spotatui-jam/
├── README.md                     ← este documento
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
│   └── 09-operacion.md           ← relay, despliegue, observabilidad
├── research/
│   ├── 00-sintesis.md            ← conclusión de la investigación
│   ├── 01-spotatui-architecture.md
│   ├── 02-existing-projects.md
│   └── 03-technical-feasibility.md
├── pipeline/
│   ├── ci.yml                    ← GitHub Actions (PR)
│   ├── nightly.yml               ← fuzz, soak, load
│   └── release.yml               ← artefactos + relay
└── plan/
    ├── checklist-fase-0.md       ← qué validar antes de escribir código
    └── definicion-de-hecho.md
```

---

## Pipeline en una frase

`fmt + clippy + tests` en cada push → simulación de protocolo y relay local en PR → e2e con dos clientes simulados y snapshots TUI en `main` → nightly de fuzz/soak/load → release firmada con relay desplegado a staging y smoke test antes de producción.

El detalle, incluyendo matrices, gates y rollback, está en [`docs/04-pipeline.md`](docs/04-pipeline.md).

---

## Cómo leer este plan

1. **Si solo quieres la respuesta:** [`research/00-sintesis.md`](research/00-sintesis.md).
2. **Si vas a decidir si hacerlo:** [`docs/01-vision-producto.md`](docs/01-vision-producto.md) y [`docs/06-riesgos.md`](docs/06-riesgos.md).
3. **Si vas a implementarlo:** [`docs/02-arquitectura.md`](docs/02-arquitectura.md), [`docs/03-plan-entrega.md`](docs/03-plan-entrega.md) y [`docs/08-backlog.md`](docs/08-backlog.md).
4. **Si vas a mantenerlo:** [`docs/04-pipeline.md`](docs/04-pipeline.md) y [`docs/09-operacion.md`](docs/09-operacion.md).

---

## Siguiente paso

1. Revisar [`docs/02-arquitectura.md`](docs/02-arquitectura.md) y [`docs/01-vision-producto.md`](docs/01-vision-producto.md); confirmar o corregir las decisiones D1–D6.
2. Ejecutar la Fase 0 siguiendo [`plan/checklist-fase-0.md`](plan/checklist-fase-0.md): reproducir Listening Party con relay local, medir límites y hacer los spikes de protocolo y `native_queue`.
3. Celebrar el gate G0: si los spikes pasan, empezar la Fase 1 (protocolo y cola) con el backlog [`docs/08-backlog.md`](docs/08-backlog.md).

## Supuestos de trabajo

- Todos los participantes de la v1 usan spotatui con Spotify Premium; es el modelo que ya exige Listening Party.
- El host reproduce en el dispositivo nativo de spotatui (librespot), no en un Connect externo, para que la cola sea controlable.
- El alcance inicial es una sala de hasta 8 personas y 100 tracks en cola.
- El proyecto se publica con licencia MIT, compatible con spotatui, y se intenta upstream antes de mantener un fork.

Si alguno de estos supuestos cambia (por ejemplo, invitados sin cuenta de Spotify), la alternativa está descrita en [`docs/02-arquitectura.md`](docs/02-arquitectura.md#variante-b-jukebox-con-invitados-sin-cuenta).
