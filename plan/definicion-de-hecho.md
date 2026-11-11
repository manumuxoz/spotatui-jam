# Definición de hecho (Definition of Done)

Aplica a toda historia, PR y release. Una casilla sin evidencia no está hecha.

## Código

- [ ] Compila en la feature set del release y en `--no-default-features`.
- [ ] `cargo fmt --all -- --check`.
- [ ] `cargo clippy --locked --all-targets -- -D warnings`.
- [ ] Sin `unwrap()`/`expect()` en rutas de red, input de usuario o parseo de mensajes.
- [ ] Errores con contexto accionable; no se silencian.
- [ ] No se añaden dependencias sin justificar (licencia, tamaño, mantenimiento).
- [ ] Los logs no incluyen tokens, nombres completos ni URIs completas en nivel info.

## Tests

- [ ] Unitarios del camino feliz y de cada rechazo.
- [ ] Si toca protocolo: golden fixture + compatibilidad old/new.
- [ ] Si toca cola: property test de convergencia.
- [ ] Si toca parser: fuzz smoke.
- [ ] Si toca UI: snapshot a 80×24 y 120×40.
- [ ] Si toca relay: integración con 2 clientes.
- [ ] Si toca `native_queue`: test de consumo y no divergencia.
- [ ] El test falla sin el cambio (verificado localmente).

## Protocolo

- [ ] Mensaje aditivo; no se rompe v1.
- [ ] `revision` monotónica.
- [ ] Tamaño máximo verificado (< 3.800 B por chunk).
- [ ] `op_id` idempotente.
- [ ] Documentado en `docs/protocolo-party-v2.md`.
- [ ] Matriz de compatibilidad actualizada.

## UI/UX

- [ ] Funciona a 80×24.
- [ ] No depende solo del color.
- [ ] Teclas documentadas y reconfigurables.
- [ ] Estados: loading, error, vacío, lleno, sin permiso, reconexión.
- [ ] Feedback para cada acción (ack/error).
- [ ] Texto sanitizado (sin ANSI/control chars).
- [ ] i18n en todas las lenguas soportadas o fallback explícito.

## Documentación

- [ ] `docs/party-queue.md` actualizado si es user-facing.
- [ ] `docs/keybindings.md` actualizado.
- [ ] `CHANGELOG.md` con entrada.
- [ ] Guía de self-hosting si toca relay.
- [ ] Capturas/gif si es UI.

## Seguridad

- [ ] Permisos verificados en host y relay.
- [ ] Input no confiable validado (URI, strings, tamaños).
- [ ] Rate limit probado.
- [ ] Sin nuevo secreto en el repo.
- [ ] `cargo audit` y `cargo deny` verdes.

## Operación

- [ ] Si toca relay: plan de rollback y kill switch.
- [ ] Si toca deploy: staging + smoke test.
- [ ] Métricas/alertas si aplica.
- [ ] No se requiere intervención manual para un caso normal.

## Release

- [ ] Soak de 1 h verde.
- [ ] Prueba manual con Spotify (15 casos) verde.
- [ ] Build matrix verde.
- [ ] Artefactos firmados y checksums.
- [ ] Smoke post-release con el binario descargado.
- [ ] Release notes publicadas.
- [ ] Rollback probado.

## Criterio final

> Si un usuario nuevo puede crear una sala, invitar a alguien, añadir un track, verlo sonar y salir sin leer código ni pedir ayuda, la historia está hecha.
