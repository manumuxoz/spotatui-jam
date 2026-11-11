# ADR-0005 — Upstream-first, fork como fallback

- **Estado:** aceptada
- **Fecha:** 2026-11-11
- **Decisores:** autor del proyecto

## Contexto

spotatui es MIT y está muy activo (v0.48.0, noviembre 2026). Un fork permanente duplica mantenimiento, releases y documentación. Pero el PR de cola toca varias capas y puede no encajar en el roadmap del maintainer.

## Decisión

1. Desarrollar en un fork, pero con ramas y commits pensados para un PR upstream.
2. Abrir un PR draft temprano con el design doc y buscar feedback antes de escribir la UI.
3. Si el maintainer acepta, merge upstream y archivar el fork.
4. Si lo rechaza o no responde en 3 semanas, mantener el fork con releases propias y rebase mensual.

## Consecuencias

- Los commits deben ser atómicos y no mezclar packaging del fork.
- La feature no puede depender de cambios internos que el upstream no quiera.
- Hay que reservar tiempo para revisión y rebase.
- El fork puede publicar binarios con la marca `-jam.N`.
- La licencia MIT permite ambas vías; se conservan avisos de copyright.

## Métricas de decisión

- Feedback del maintainer en 3 semanas.
- Tamaño del PR: < 1.500 líneas netas si es posible.
- Conflictos de rebase: < 1 día al mes.

## Alternativas

- **Fork privado desde el día 1:** más rápido, pero pierde revisión y comunidad.
- **Plugin:** no cubre la cola nativa; se mantiene como prototipo.
