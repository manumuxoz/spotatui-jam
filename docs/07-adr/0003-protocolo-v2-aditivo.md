# ADR-0003 — Protocolo v2 aditivo con snapshots y deltas

- **Estado:** aceptada
- **Fecha:** 2026-11-11
- **Decisores:** autor del proyecto

## Contexto

El protocolo actual (`SyncMessage`) no tiene cola, identidad de participante ni revisión. El relay limita cada mensaje a 4 KB y debe seguir funcionando con clientes 0.48. Una cola de 100 items no cabe en un mensaje.

## Decisión

Añadir mensajes aditivos v2:

- `queue_snapshot` troceado por presupuesto de bytes (3.500 B, máx. 3.800 B por mensaje).
- `queue_patch` con `revision` monotónica.
- `queue_command` con `op_id`, `base_rev` y `from` asignado por el relay.
- `queue_ack`, `queue_resync`, `party_config`, `party_members`.
- `Move` usa `after: Option<ItemId>` en vez de un índice; los snapshots y acks pueden dirigirse con `to`.

No se elimina ni se cambia el formato de ningún mensaje v1. La negociación es por `protocol_version`/`features` en `room_created`/`joined_room`.

## Consecuencias

- Clientes viejos ignoran los mensajes nuevos y siguen sincronizando playback.
- El relay v2 reenvía `queue_*`; el relay v1 los descarta.
- El chunking añade complejidad y tests de tamaño.
- La revisión permite detectar gaps y pedir resync sin CRDT.
- Los acks hacen que la UI no sea optimista; la latencia debe ser < 500 ms.

## Alternativas descartadas

- **Subir el límite a 64 KB:** rompe relays viejos y aumenta abuso; se puede permitir en el relay Rust, no en el protocolo base.
- **Cola en el relay:** stateful, más complejo de auto-hospedar.
- **Mensajes binarios:** innecesario para 100 items; JSON es depurable y compatible.

## Validación

Golden tests, property tests, fuzz y un test que serialice el peor chunk posible.
