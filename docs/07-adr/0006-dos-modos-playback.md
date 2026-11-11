# ADR-0006 — Dos modos de playback y postura ante la política

- **Estado:** aceptada
- **Fecha:** 2026-11-11
- **Decisores:** autor del proyecto

## Contexto

La investigación de política (ver `research/02-existing-projects.md`) concluye que el sync multi-dispositivo se parece a la Jam oficial y activa el riesgo III.11 (“no replicar una experiencia core”), mientras que el playback en un solo dispositivo (jukebox) es el modelo más limpio y permite invitados sin Premium.

Listening Party ya implementa sync multi-dispositivo. La cola puede funcionar con ambos modos.

## Decisión

La cola se diseña independiente del modo de playback. Se soportan dos modos:

1. **Follow-host (sync):** el existente; cada participante reproduce el track del host con su cuenta Premium. Es el modo del MVP porque reutiliza todo.
2. **Host-only (jukebox):** solo el host reproduce; los invitados ven y modifican la cola. Es el modo recomendado para uso público/eventos y la base de un futuro invitado web sin cuenta.

La UI etiqueta el modo sync como “sala privada” y no se opera como servicio público. El modo por defecto de una release pública amplia debe ser host-only; el fork personal puede usar sync.

## Consecuencias

- El protocolo debe separar `control_mode` (transporte) de `playback_mode` (quién reproduce).
- En host-only, los invitados no necesitan reproducir; puede que ni Premium. Requiere validar login y búsqueda sin playback.
- El invitado web sin cuenta es P1, no MVP.
- La documentación debe explicar los dos modos y sus requisitos.
- No se usan marcas de Spotify ni se promete interoperabilidad con Jam.

## Alternativas

- **Solo sync:** máximo valor para remoto, pero mayor exposición y requiere Premium a todos.
- **Solo host-only:** más limpio, pero no sirve para amigos remotos y no reutiliza Listening Party.
- **Decidir después:** se difiere, pero se deja la arquitectura preparada.

## Revisión

Se revisa antes de una release pública o si el proyecto se distribuye como servicio. La decisión final depende del uso previsto (personal vs público).
