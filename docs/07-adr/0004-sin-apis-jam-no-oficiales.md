# ADR-0004 — No depender de APIs de Jam no oficiales

- **Estado:** aceptada
- **Fecha:** 2026-11-11
- **Decisores:** autor del proyecto

## Contexto

La Jam oficial de Spotify no tiene API pública a noviembre de 2026. Existen vías no oficiales:

- `spotify_cli` incluido en Spotify Desktop (macOS/Windows; `jam create/status/leave/end/members/kick/permissions`, sin `join`).
- GraphQL interno del Web Player (`socialsession`).
- `librespot` puede recibir una Jam como dispositivo Connect, pero no crearla ni gestionarla (issue #1576). El fork de spotatui ya incluye `social_connect_v2.proto` y Spirc escucha `social-connect/v2/session_update`, pero no hay llamadas create/join/leave ni resolución de códigos.

## Decisión

El núcleo de spotatui-jam no usa ninguna de estas vías. La cola y la sala son propias. Cualquier puente con la Jam nativa se implementa, si acaso, como adaptador opcional P2, detrás de un flag, solo en plataformas donde exista y con un aviso de “no soportado”.

## Razones

- `spotify_cli` no es una API contractual, cambia sin aviso y no está confirmado en Linux.
- No hay subcomando `join`; unirse a una Jam ajena no está expuesto.
- GraphQL interno viola los Terms (reverse engineering).
- Depender de ello haría que la feature principal no funcione en la plataforma natural de spotatui.
- El coste de mantener el adaptador supera el valor para el MVP.

## Consecuencias

- No se promete interoperabilidad con la Jam oficial.
- La UI se llama “Party Queue”, no “Spotify Jam”.
- Si Spotify publica una API, se evalúa un adaptador en un ADR nuevo.
- El adaptador opcional no contamina el protocolo ni el modelo de cola.
