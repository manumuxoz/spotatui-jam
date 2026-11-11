# ADR-0001 — Extender Listening Party en el core

- **Estado:** aceptada (sujeta a G0)
- **Fecha:** 2026-11-11
- **Decisores:** autor del proyecto

## Contexto

spotatui 0.48 ya implementa Listening Party: relay WebSocket, código de 6 caracteres, presencia, sincronía de playback y control compartido de transporte. Lo que falta es una cola compartida. Existen alternativas fuera del core (plugin Lua + sidecar, daemon con MCP, app web) y dentro (fork/upstream).

## Decisión

Extender el core de spotatui en un fork upstream-first, añadiendo la cola al protocolo de Listening Party y una pantalla TUI dedicada. No se construye un daemon separado ni se empieza de cero.

## Alternativas

1. **Plugin Lua + sidecar HTTP:** no tiene WebSocket, no puede quitar/reordenar la cola nativa y los timers dependen del tick de UI. Sirve para prototipar, no para la versión final.
2. **Daemon + MCP/CLI:** añade un proceso, autenticación local, empaquetado y una segunda UI; no reutiliza el protocolo de party.
3. **App web independiente:** resuelve el problema, pero no es una extensión de spotatui y duplica auth y playback.
4. **No hacer nada:** el gap queda cubierto por jukebox web, pero no en la TUI.

## Consecuencias

- Hay que tocar `src/infra/network`, `src/core/app`, `src/tui` y `worker-relay`.
- El PR upstream es grande; se parte en commits atómicos.
- Si el core cambia, el fork requiere rebase.
- Se reutilizan auth, relay, sync y UI existentes; el time-to-market es mucho menor.
- La feature puede vivir en spotatui si el maintainer la acepta.

## Criterios de revisión

Si el spike de `native_queue` (Fase 0) demuestra que el hook es inviable, se reconsidera esta decisión y se evalúa el plan B (cola virtual solo display) o el plugin.
