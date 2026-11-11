# ADR-0002 — Cola autoritativa del host y native_queue derivada

- **Estado:** aceptada
- **Fecha:** 2026-11-11
- **Decisores:** autor del proyecto

## Contexto

La cola colaborativa necesita una única fuente de verdad. spotatui tiene dos estructuras relevantes:

- `native_queue: Vec<TrackInfo>`, que consume el motor de playback.
- La cola de Spotify del contexto (Spirc), que no es visible ni modificable por la app.

Si se mantienen una cola colaborativa y `native_queue` en paralelo, se producen divergencias en add/remove/move/consume.

## Decisión

El host es la autoridad. `PartyQueue` contiene `current`, `items`, `history`, IDs estables, metadata y revisión. `current` vive en `queue_now`; `native_queue` es la **ventana de materialización** de los próximos items. Si el motor introduce mutaciones externas, se reduce a 1–2 items y se materializa el siguiente al avanzar. El consumo del motor notifica a `PartyQueue`: el `current` anterior pasa a `history` y el head de `items` pasa a `current`.

## Consecuencias

- Un único punto de mutación (`apply_party_op`) actualiza ambos.
- El hook de `AdvanceNativeQueue` es obligatorio; sin él, los items consumidos quedan fantasma.
- Los tracks no Spotify se rechazan en la cola v1.
- Al iniciar la jam, la `native_queue` existente se adopta o se vacía (configurable).
- Los tests de propiedad comparan `PartyQueue` y `native_queue` tras secuencias aleatorias.

## Alternativas descartadas

- **Metadata paralela:** más simple al principio, pero frágil ante reorder/consume.
- **Cola en el relay:** el relay pasaría a ser stateful y dejaría de ser tonto; complica self-hosting y la consistencia.
- **CRDT:** innecesario con un solo host y WebSocket FIFO.

## Riesgos

Si el hook de consumo es invasivo, se puede limitar la v1 a “añadir al final” y posponer reorder. El ADR se revisa en G0.
