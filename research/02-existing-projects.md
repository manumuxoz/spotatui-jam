# 02 — Ecosistema existente: Spotify Jam, colas compartidas y spotatui

**Fecha de consulta:** 11 de noviembre de 2026.
**Objeto:** determinar si ya existe un equivalente a una extensión TUI de spotatui para Jams, qué proyectos cubren el espacio y qué gap real queda.
**Método:** documentación oficial de Spotify for Developers, comunidad oficial, API/HTML de GitHub, ficheros `README`/`CHANGELOG`/`docs` y código fuente en la etiqueta `v0.48.0` de spotatui. Cada afirmación lleva marca de verificación cuando corresponde.

Marcas usadas: **[V]** verificado en fuente primaria · **[P]** parcial o inferido · **[?]** no verificado.

---

## Resumen ejecutivo

1. **Spotify sigue sin API pública de Jam a noviembre de 2026.** La Web API no tiene ningún endpoint de Jam; el Web Playback SDK tampoco expone métodos de Jam. Spotify dijo en abril de 2025 que Jam «pertenece más a los playback SDKs que a la Web API», pero a noviembre de 2026 no hay nada publicado. **[V]**
2. **La única vía oficial para varios usuarios sigue siendo la app de Spotify.** Jam es una función de consumidor: el host necesita Premium, el invitado remoto también; los usuarios Free solo pueden unirse en persona y añadir a la cola. **[V]**
3. **La vía no oficial existe y está bastante explorada:** el binario `spotify_cli` que Spotify incluye en su app de escritorio tiene un subcomando `jam` completo (`status`, `create`, `leave`, `end`, `members`, `kick`, `permissions`). Está confirmado en macOS y Windows; en Linux no está confirmado. No está documentado, cambia con las versiones y no es una API contractual. **[V/P]**
4. **`librespot` puede reproducir una Jam como dispositivo Connect, pero no crearla ni gestionarla.** El issue [#1576](https://github.com/librespot-org/librespot/issues/1576) sigue abierto desde el 8 de septiembre de 2025; el último comentario (29 de julio de 2026) pregunta precisamente cómo iniciar una Jam desde librespot y no tiene respuesta. **[V]**
5. **spotatui 0.48.0 ya tiene una "Listening Party"**, no una Jam: sincroniza track/posición/play-pause del host a los invitados por un relay WebSocket, con código de 6 caracteres y modos `host_only` / `shared_control`. Desde 0.48 los invitados con control compartido pueden play/pause/skip. **No comparte la cola**: el protocolo no tiene ningún mensaje de cola y el invitado ve «The host picks the songs». **[V]**
6. **No hay ningún proyecto que sea "Spotify Jam dentro de una TUI de Spotify".** Hay dos competidores cercanos: spotatui Listening Party (TUI, sync, sin cola) y `Sarthakischill/spot-along` (TUI Node, sync por relay, sin cola, prácticamente sin actividad). **[V/P]**
7. **El gap real no es la sincronización: es la cola compartida.** Esa es la primitiva que convierte una "fiesta de escucha" en una Jam: varios usuarios añaden, reordenan y votan una cola única, y el playback la resuelve. Nadie la tiene hoy en una TUI de Spotify. **[V]**
8. **La vía legal/técnica más viable** es extender el relay existente con una cola propiedad de la sala, playback autoritativo en el dispositivo del host (un solo stream) y, opcionalmente, modo remoto con cada invitado Premium en su dispositivo. La integración con Jam nativa vía `spotify_cli` solo como adaptador opcional macOS/Windows, nunca como núcleo. **[P]**

---

## 1. Estado oficial de la API de Spotify (noviembre de 2026)

### 1.1 No hay endpoints públicos de Jam

La referencia de la Web API organiza el *Player* en exactamente estos endpoints: **[V]**

- Get Playback State
- Transfer Playback
- Get Available Devices
- Get Currently Playing Track
- Start/Resume Playback
- Pause Playback
- Skip To Next / Previous
- Seek To Position
- Set Repeat Mode
- Set Playback Volume
- Toggle Playback Shuffle
- Get Recently Played Tracks
- Get the User's Queue
- Add Item to Playback Queue

No hay `jam`, `session`, `group-session`, `socialsession` ni equivalente. La página del endpoint de reproducción actual añade, además, estas notas de política: **[V]**

> «Streaming applications may not be commercial»
> «Keep audio content in its original form»
> «Do not synchronize Spotify content»
> «Spotify content may not be broadcasted»

El **Web Playback SDK** tampoco tiene Jam. Su referencia lista `Spotify.Player` con `connect`, `disconnect`, `getCurrentState`, `setName`, volumen, `pause`, `resume`, `togglePlay`, `seek`, `previousTrack`, `nextTrack` y `activateElement`; no hay creación, unión, miembros ni cola de sesión. **[V]** Su página de concepto solo describe Spotify Connect: «All devices must be connected to the same network» y el SDK sirve para crear un dispositivo Connect dentro del navegador. **[V]**

**Conclusión:** a 11 de noviembre de 2026, Jam no está en la Web API ni en el Web Playback SDK. No es que esté "deprecado": nunca se publicó.

### 1.2 Jam ≠ Spotify Connect ≠ Web Playback SDK

| Concepto | Qué es | Quién lo implementa | API pública | Modelo de reproducción |
|---|---|---|---|---|
| **Jam** | Sesión social: un host reproduce, varios participantes añaden a la cola y (opcionalmente) controlan playback/volumen | App oficial de Spotify (móvil, desktop, TV, Auto, CarPlay) | **No** | Un dispositivo reproduce; los demás escuchan en el suyo (remoto) o por el altavoz (en persona) |
| **Spotify Connect** | Descubrimiento y transferencia de reproducción entre dispositivos de la misma cuenta/red | Apps y dispositivos certificados; librespot/spotifyd como receptores | **Sí**, vía Player API (`devices`, `transfer`) | Un dispositivo activo por cuenta; no es multiusuario |
| **Web Playback SDK** | Reproductor Spotify dentro de una web, registrado como dispositivo Connect | JavaScript en el navegador | **Sí** (es el SDK, no una API REST) | Un dispositivo Connect más, ligado a una cuenta Premium |

La distinción importa para el proyecto: **Connect da control de dispositivo, no colaboración**. Se puede transferir playback y leer la cola de una cuenta, pero no existe el concepto de "sala" ni de "miembros". Jam se implementa por encima, en servidores de Spotify, con un protocolo que no está expuesto.

### 1.3 Límites prácticos de la Web API para una app self-hosted

Aunque Jam no exista, una app de cola compartida podría intentar construirse solo con Web API + Connect. Los límites de 2026 la hacen inviable para grupos grandes:

| Fecha | Cambio | Efecto |
|---|---|---|
| 6 feb 2026 (anuncio) / 11 feb 2026 (apps nuevas) / 9 mar 2026 (apps existentes) | Development Mode: Premium obligatorio para el dueño, **máximo 5 usuarios autenticados**, allowlist, un Client ID | Un grupo de 6+ amigos no cabe en Development Mode **[V]** |
| 15 may 2025 | Extended Quota solo para **organizaciones** (no individuos), servicio lanzado, **≥250 000 MAU**, viabilidad comercial | Un proyecto personal no puede pedir cuota extendida **[V]** |
| Jul 2026 | Client IDs por desarrollador suben de 1 a 25; la cuota de Development Mode se cuenta **por cuenta**, no por Client ID; 429 con `reason: QUOTA_EXCEEDED` | No elimina el límite de 5 usuarios; solo cambia la administración **[V]** |
| Feb 2026 | Eliminación de varios endpoints de metadata y reducción de `search.limit` de 50 a 10 | Más llamadas para el mismo resultado; más presión de cuota **[V]** |

Fuentes: [Quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes), [Changelog febrero 2026](https://developer.spotify.com/documentation/web-api/references/changes/february-2026), [Changelog julio 2026](https://developer.spotify.com/documentation/web-api/references/changes/july-2026).

**Matiz importante sobre spotatui:** desde 0.47.0, spotatui no usa el flujo OAuth normal de la Web API. Su changelog dice: «spotatui now talks to Spotify the way the official app does, over its own Spotify session… the per-app rate limits and the endpoints Spotify removed for Development Mode apps no longer apply». **[V]** Eso esquiva los límites de Development Mode, pero también significa que el proyecto ya opera fuera de la API documentada. Cualquier función de Jam construida sobre spotatui hereda esa situación.

### 1.4 La posición de Spotify en la comunidad

Hilo oficial «Web API info for Jams» ([community.spotify.com/forum/en/discussion/6235408](https://community.spotify.com/forum/en/discussion/6235408/web-api-info-for-jams)): **[V]**

- **2 ago 2024:** un usuario pregunta por una API de Jam.
- **15 ago 2024:** respuesta: «No, no API for Jam I am afraid.»
- **3 sep 2024:** «We are looking at it, but it will not be added this year.»
- **7 abr 2025:** «We concluded that Jam better belongs in playback SDKs than Web API, so you will not be seeing Jam on Web API anytime soon. But if you have a 3rd party client that is using a playback SDK, you might be seeing Jam there.»
- **28 ago 2025 / 3 sep 2025 / 26 nov 2025:** usuarios preguntando por novedades; sin respuesta de Spotify en la página.

Petición formal «Add Spotify API support for Jams» ([community.spotify.com/t5/Spotify-for-Developers/Add-Spotify-API-support-for-Jams/td-p/7169789](https://community.spotify.com/t5/Spotify-for-Developers/Add-Spotify-API-support-for-Jams/td-p/7169789)), publicada el **10 oct 2025**, pide listar/crear/unirse/salir, ver miembros y cola, añadir tracks y controlar playback. **No consta respuesta oficial ni implementación** a 11 nov 2026. **[V]**

También existe un issue abierto en `aome510/spotify-player` desde el **5 feb 2025**, «[Feature]: Implement Spotify Jams (when Spotify supports it via the Web API)» ([#675](https://github.com/aome510/spotify-player/issues/675)), sin avances. **[V]**

**Nota de nomenclatura:** Spotify tiene desde 2025/2026 una función de consumidor llamada **Listening Party** ([support.spotify.com/us/article/listening-party](https://support.spotify.com/us/article/listening-party/)), que es un evento de artista con registro, plazas limitadas y un solo dispositivo por cuenta. **No es Jam** y no se debe confundir con la "Listening Party" de spotatui. **[V]**

---

## 2. Vía no oficial

### 2.1 `spotify_cli`, el binario incluido en Spotify Desktop

Spotify incluye un ejecutable de línea de comandos que usa su propia función «Studio by Spotify Labs». No está documentado como API, pero es real y expone un subcomando `jam`. **[V]**

Rutas y versiones confirmadas:

| Plataforma | Ruta | Versión observada | Estado |
|---|---|---|---|
| macOS | `/Applications/Spotify.app/Contents/MacOS/spotify_cli` | `1.2.94.583` (blog, 23 jul 2026) | **Confirmado** **[V]** |
| Windows (Microsoft Store) | `spotify_cli.exe`, alias en `%LOCALAPPDATA%\Microsoft\WindowsApps` | `1.3.4.258` (Resona, 14 oct 2026) | **Confirmado** **[V]** |
| Windows (instalador clásico) | — | — | No confirmado; Resona lo dejó pendiente **[?]** |
| Linux | — | — | **No confirmado**; `spotify-cli-mcp` lo marca como pendiente **[?]** |

Salida de `jam --help` capturada en [docs/spotify-cli-reference.txt](https://raw.githubusercontent.com/mrvnklm/spotify-cli-mcp/main/docs/spotify-cli-reference.txt): **[V]**

```text
jam
  status (default)  Show current Jam session
  create            Start a new Jam
  leave             Leave the current Jam
  end               End the Jam (host only)
  members           List Jam members
  kick              Remove a member from the Jam
  permissions       View or change Jam permissions
```

`jam permissions` acepta `--queue-only <on|off>` y `--volume-control <on|off>`. `jam members` y `jam status` aceptan `--format json`. **[V]**

**Lo que no hay:** no existe un subcomando `jam join`. El binario crea una Jam «on the current device», pero unirse a una Jam ajena no está expuesto como comando. El README de `spotify-cli-mcp` dice «create/join/manage», pero su tabla de herramientas no incluye `join_jam`; probablemente lo cubre `navigate_to_uri` con el enlace de invitación, lo cual **no está verificado**. **[P/?]**

**Estabilidad y ToS:**

- Es un binario interno, versionado con la app; su interfaz puede cambiar sin aviso. No hay contrato, deprecation policy ni SLA. **[V]**
- Requiere la app de escritorio **abierta y logueada** en la misma máquina; la autenticación es implícita por IPC local. **[V]**
- Los Spotify Developer Terms prohíben «modifying, editing, altering, creating derivative works, disassembling, decompiling, reverse-engineering, or extracting source code from the Spotify Platform» (Section IV.2.1.b). Invocar un binario ya instalado no es, en sentido estricto, ingeniería inversa, pero **no es una vía autorizada** y no hay licencia de uso como API. **[V/P]**
- No es multiplataforma garantizado: macOS y Windows confirmados; Linux, la plataforma natural de una TUI, sin confirmar. **[P]**

**Veredicto:** sirve para un experimento local o un adaptador opcional, no como base de un producto o de una función que deba funcionar en Linux.

### 2.2 `mrvnklm/spotify-cli-mcp`

[https://github.com/mrvnklm/spotify-cli-mcp](https://github.com/mrvnklm/spotify-cli-mcp) — **MIT**, 8 commits, 0 estrellas, macOS únicamente, 51 herramientas MCP, 7 de ellas de Jam: `get_jam_status`, `list_jam_members`, `create_jam`, `leave_jam`, `end_jam`, `kick_from_jam`, `set_jam_permissions`. **[V]**

Es la mejor documentación pública del CLI: incluye la captura completa de `--help` y notas de limitaciones reales (por ejemplo, que `history top --type` falla en la versión 1.2.94.583). No es un producto mantenido ni tiene usuarios; vale como referencia, no como dependencia. **[V]**

### 2.3 Endpoints reverse-engineered del Web Player

El Web Player de Spotify usa GraphQL interno con operaciones de Jam. El proyecto `kota-zo/myspoti` documenta 179 operaciones GraphQL y 89 servicios de página, e incluye explícitamente: «Jam | availability, session, start, join, leave, end, invite link, remove member, queue-only, participant volume». **[V]** Su README añade que, con cuenta Free, Spotify responde `RESTRICTED` al intentar `start`; join y el resto están cableados pero no verificados. **[V]**

Eso confirma dos cosas: (a) Jam existe como superficie GraphQL del cliente web; (b) **no es una API pública ni estable**, y usarla implica saltarse el modelo de acceso autorizado.

Otros proyectos históricos:

- `eta-c/spotify-jam` ([GitHub](https://github.com/eta-c/spotify-jam)): extensión de Spicetify que creaba/cerraba/compartía/unía Jams usando el token de `open.spotify.com/socialsession/<token>`. Está marcada como **deprecated** porque Jam llegó a la app de escritorio. Es la evidencia más clara de la vía `socialsession`. **[V]**
- `dragonhuntr/JamThing` ([GitHub](https://github.com/dragonhuntr/JamThing)): CarThing con API privada y Jam; roto desde que Spotify eliminó la autenticación por usuario/contraseña. **[V]**
- `t1mlange/spotify-group-session` ([GitHub](https://github.com/t1mlange/spotify-group-session)): parcheaba el cliente de escritorio para recuperar Group Sessions; **archivado** desde oct 2024. **[V]**

### 2.4 `librespot` y el issue #1576

[https://github.com/librespot-org/librespot/issues/1576](https://github.com/librespot-org/librespot/issues/1576) — abierto el **8 sep 2025**, etiqueta `enhancement`, sin asignar, sin PR. **[V]**

Cronología:

- **8 sep 2025:** `arawho` pregunta si hay soporte de Jam; usa ncspot.
- **10 sep 2025:** `kliszaj` responde que con **Raspotify** (wrapper de librespot) «Jams work fine for me».
- **10 sep 2025:** un maintainer pregunta qué versión usa.
- **10 sep 2025:** el autor concluye: «it seems librespot supports Jams but the client that I am using, ncspot, does not».
- **29 jul 2026:** `iTomKo` pregunta «Is there a way to start jam session from librespot tho?». **Sin respuesta.** **[V]**

**Interpretación:** librespot, como receptor Connect, **puede recibir la reproducción de una Jam** iniciada desde otro cliente (testimonio de un usuario, no test de integración). Lo que no puede es **crear, unirse o gestionar** la sesión. Para una TUI, eso significa: si el usuario ya está en una Jam desde el móvil, un cliente librespot podría reproducirla; pero la app no puede iniciarla ni administrarla.

---

## 3. Proyectos existentes de cola compartida / jukebox

### 3.1 Tabla comparativa

Ordenada por cercanía al objetivo. «Último commit» es el último commit de la rama por defecto consultado vía GitHub API el 11 nov 2026; no equivale a última release.

| Proyecto | URL | Licencia | Último commit | Enfoque | Premium | Reutilizable |
|---|---|---|---|---|---|---|
| **spotatui — Listening Party** | [LargeModGames/spotatui](https://github.com/LargeModGames/spotatui) | MIT | 2026-11-10 (v0.48.0: 2026-11-07) | Host→invitados: sync de track/posición/play-pause por relay; código de 6 caracteres; `host_only`/`shared_control`; **sin cola compartida** | Todos | Relay, UI de sala, corrección de deriva, docs. **Competidor más cercano** |
| **SAKMZ/syncwave** | [SAKMZ/syncwave](https://github.com/SAKMZ/syncwave) | PolyForm Noncommercial 1.0.0 | 2026-10-27 | Salas self-hosted, playback sincronizado, cola compartida, chat, reacciones, vote-to-skip; reproduce vía YouTube/yt-dlp, los enlaces de Spotify solo se importan | Ninguno | UX de sala/cola, reloj de sync, Docker, PWA. **Licencia no comercial** |
| **Kyzenkms/spicetify-jam** | [Kyzenkms/spicetify-jam](https://github.com/Kyzenkms/spicetify-jam) | README dice MIT; GitHub no detecta LICENSE | 2026-11-05 | Extensión Spicetify: sync de host, cola compartida, controles de invitado, avatares, invitación por código/QR | Todos | Sync, invitaciones, cola. Licencia ambigua |
| **citrus401/spotify-listen-together-2.0** | [citrus401/spotify-listen-together-2.0](https://github.com/citrus401/spotify-listen-together-2.0) | MIT | 2026-11-07 | Spicetify: sync ~10 ms, cola espejo del host, offset por dispositivo | Todos | Técnica de sync fina y drift correction |
| **viscoci/OpenDJ** | [viscoci/OpenDJ](https://github.com/viscoci/OpenDJ) | MIT | 2026-09-28 | Self-host: invitados por QR, host reproduce en un dispositivo Connect, moderación, vote-to-skip, vista TV | Host | Abstracción de provider, contratos de sala realtime, Docker, lógica de cola. **Muy reutilizable** |
| **jaherhum/crowdroom** | [jaherhum/crowdroom](https://github.com/jaherhum/crowdroom) | AGPL-3.0 | 2026-10-28 | FastAPI + Vue: salas, invitaciones, cola, votos, playback en dispositivo del host | Host | Solo bajo AGPL; arquitectura y flujos |
| **Dbochman/EchoNest** | [Dbochman/EchoNest](https://github.com/Dbochman/EchoNest) | Sin licencia | 2026-11-02 | Cola compartida + **playback individual**: cada usuario reproduce en su dispositivo con su cuenta | Todos | Modelo conceptual exacto; **no reutilizable legalmente** |
| **uhteddy/spartify** | [uhteddy/spartify](https://github.com/uhteddy/spartify) | Sin licencia | 2026-03-15 | App macOS host, invitados por navegador, búsqueda, peticiones, votos, auto-queue a Spotify | Host | Flujo QR/invitado; no reutilizable |
| **mattiasahlsen/spotify-queue** | [mattiasahlsen/spotify-queue](https://github.com/mattiasahlsen/spotify-queue) | Sin licencia | 2021-02-25 | `collabqueue.com`: cola compartida, playback del host, invitados por enlace sin cuenta | Host | Histórico; muerto y sin licencia |
| **junga3/jammix** | [junga3/jammix](https://github.com/junga3/jammix) | Sin licencia | 2026-04-26 | Cola generada por afinidad del grupo (top tracks/artists), host reproduce, vote skip | Todos | Concepto de mezcla por gustos; no reutilizable |
| **niclaszll/spotify-listening-party** | [niclaszll/spotify-listening-party](https://github.com/niclaszll/spotify-listening-party) | Apache-2.0 | 2021-03-11 | Proyecto de curso: salas, cola compartida, playback sincronizado en navegador, chat | Todos | Referencia de arquitectura con licencia permisiva |
| **AugustoQueiroz/Shared-Sessions** | [AugustoQueiroz/Shared-Sessions](https://github.com/AugustoQueiroz/Shared-Sessions) | MIT | 2019-06-10 | Go: unirse a sesiones y controlar playback vía Connect | Premium | Histórico; ideas de control Connect |
| **t1mlange/spotify-group-session** | [t1mlange/spotify-group-session](https://github.com/t1mlange/spotify-group-session) | GPL-3.0 | 2024-10-12 (archivado) | Parche al cliente de escritorio para Group Sessions | — | Obsoleto; evidencia histórica |
| **Sarthakischill/spot-along** | [Sarthakischill/spot-along](https://github.com/Sarthakischill/spot-along) | Sin licencia verificada | Creado 2025-07-27; actividad no verificada | **TUI Node**: salas, código, sync del playback del host, backend Cloudflare Workers/Durable Objects | Todos | **Competidor TUI directo**; inviable por Development Mode (5 usuarios) |
| **jabezpauls/aura-tui** | [jabezpauls/aura-tui](https://github.com/jabezpauls/aura-tui) | Sin verificar | Creado 2026-03-05 | TUI de YouTube Music con «Listen Party» (host DJ, sync por WebSocket, relay propio) | No (YouTube) | UX de fiesta en terminal, no Spotify |
| **eta-c/spotify-jam** | [eta-c/spotify-jam](https://github.com/eta-c/spotify-jam) | Sin verificar | Deprecated | Spicetify: crear/unir Jam con token `socialsession` | Todos | Evidencia de la vía reverse-engineered |
| **dragonhuntr/JamThing** | [dragonhuntr/JamThing](https://github.com/dragonhuntr/JamThing) | Sin licencia | 2026-02-05 | CarThing con API privada y Jam; roto tras eliminar password auth | — | Obsoleto |

### 3.2 Las tres arquitecturas

Conviene separar los proyectos por dónde ocurre el playback, porque cambia por completo el requisito de Premium y el riesgo legal:

1. **Playback en un solo dispositivo (host/jukebox).** El host reproduce; los invitados solo añaden a una cola. Ejemplos: OpenDJ, Spartify, CrowdRoom, collabqueue. **Solo el host necesita Premium**; los invitados pueden ser usuarios Free o no tener cuenta. Es el modelo más limpio y el más fácil de operar en persona.
2. **Playback individual sincronizado.** Cada participante reproduce en su propio dispositivo con su propia cuenta, y la app sincroniza track/posición. Ejemplos: spotatui Listening Party, spicetify-jam, listen-together, niclaszll, syncwave. **Todos necesitan Premium.** Es el modelo de Jam remota.
3. **Cola virtual sin playback propio.** La app solo gestiona la cola; el playback lo pone Spotify (Web API/Connect) o el host. Ejemplos: Jammix, E.A.R., la parte de cola de OpenDJ. Es el complemento natural de (1) y (2).

**spotatui está en (2), y le falta la cola de (3).** Ahí está el hueco.

### 3.3 Qué se puede reutilizar (y qué no)

- **De spotatui (MIT):** el relay `worker-relay/` con Durable Objects, el enum `SyncMessage`, la máquina de estados `PartyStatus`, la UI de sala y la corrección de deriva. Es la base más directa para añadir cola sin empezar de cero.
- **De OpenDJ (MIT):** la separación provider/realtime/queue, los contratos de sala con snapshot + delta, el Docker Compose y la vista TV. Muy buen diseño para copiar ideas, no necesariamente código.
- **De syncwave (PolyForm NC):** la UX de cola con «quién añadió qué», upvotes, vote-to-skip y el reloj de sincronización. **La licencia impide uso comercial**, pero permite uso personal/hobby.
- **De listen-together (MIT):** la técnica de sync fino (~10 ms) y el offset por dispositivo; útil si se quiere mejorar el drift actual de spotatui (>3 s).
- **De EchoNest:** el modelo «cola compartida, playback individual» descrito con claridad; el código no es reutilizable por falta de licencia.
- **No reutilizar:** collabqueue, Spartify, EchoNest, Jammix, spot-along y JamThing no declaran licencia. Sin licencia, el código no se puede copiar legalmente, aunque el repositorio sea público.

---

## 4. spotatui: estado real a 0.48.0

### 4.1 Qué hace la Listening Party

Documentación oficial: [docs/listening-party.md](https://github.com/LargeModGames/spotatui/blob/main/docs/listening-party.md). Release: [v0.48.0](https://github.com/LargeModGames/spotatui/releases/tag/v0.48.0), **7 nov 2026**. **[V]**

- Se abre con `Ctrl+p` (rebindable como `listening_party`).
- El host crea una sala en un relay WebSocket y obtiene un **código de 6 caracteres**.
- El host publica `track_uri`, `position_ms`, `is_playing` y `timestamp` **cada 2 segundos**.
- El invitado sigue al host; si se desvía **más de ~3 segundos**, hace seek compensando la latencia.
- Modos: **`host_only`** (por defecto) y **`shared_control`**.
- Relay por defecto: `wss://spotatui-party.spotatui.workers.dev/ws`; configurable con `behavior.relay_server_url`.
- Requiere login de Spotify y **Premium en todos los participantes**.
- Solo sigue tracks de Spotify; si el host reproduce Local Files, Subsonic, Radio, YouTube o Qobuz, la sala sigue abierta pero los invitados no reciben nada nuevo.
- Hay una **Listening Room** distinta (`r`), que es la vista de álbum + letras; no es la fiesta. No confundir ambas.

### 4.2 Qué no hace: la cola

Esto es lo importante y está verificado a nivel de protocolo. En `src/infra/network/sync.rs` de la etiqueta `v0.48.0`, el enum completo es: **[V]**

```rust
pub enum SyncMessage {
  RoomCreated { code, control_mode },
  JoinedRoom { host_name, control_mode },
  GuestJoined { name },
  GuestLeft { name },
  SyncState { track_uri, position_ms, is_playing, timestamp },
  PlaybackCommand { action, from },
  SetControlMode { control_mode },
  SetName { name },
  Heartbeat,
  Error { message },
  RoomClosed,
}
```

No hay `QueueAdd`, `QueueRemove`, `QueueReorder`, `QueueVote`, `QueueState` ni nada equivalente. **La cola no se transmite.** **[V]**

El PR que introdujo la pantalla lo dice explícitamente: «The host queue uses the drawer keys (j/k, Enter, x, J/K); a guest sees "The host picks the songs."» y en las notas de alcance: «Left out: an activity log, votes, invite links and guest transport.» ([PR #684](https://github.com/LargeModGames/spotatui/pull/684), fusionado el 30 sep 2026). **[V]** El transporte de invitado se añadió después, en 0.48; las votaciones y la cola compartida siguen fuera.

### 4.3 El control compartido sí funciona desde 0.48 (la doc está desactualizada)

El changelog de 0.48.0 incluye: **[V]**

> «A guest with shared control in a listening party plays, pauses and skips for the room, from the keys, the GUI and the OS media keys ([#442](https://github.com/LargeModGames/spotatui/issues/442)).»

El issue #442, «Party shared-control mode is inert: PartyPlaybackCommand is never dispatched», se cerró el **28 oct 2026**. **[V]**

Sin embargo, `docs/listening-party.md` sigue diciendo: «spotatui guests do not send any yet, so this changes nothing for them today». **La documentación contradice al changelog.** Para el informe y para cualquier trabajo posterior, hay que tratar la doc como obsoleta en ese punto. **[V]**

### 4.4 Issues/PRs de jam o cola compartida

Búsquedas en GitHub el 11 nov 2026: **[V]**

- `is:issue jam` en `LargeModGames/spotatui` → **0 resultados**.
- `is:issue "shared queue"` → 1 resultado, pero es un issue cerrado sobre descargas de la cola nativa (#491), no sobre cola compartida.
- `is:issue party queue` → 4 resultados, todos de documentación/teclas, ninguno pide una cola de sala.
- No hay PR abierto ni mergeado que añada cola a la party.
- No hay mención de Jam de Spotify en docs, issues ni PRs.

**Interpretación:** la Listening Party se diseñó deliberadamente como *host-follow*, no como Jam. La cola compartida nunca ha estado en el roadmap público. El hecho de que nadie la haya pedido todavía es una señal de que el gap es real y no está cubierto por la comunidad.

---

## 5. Política de desarrollador aplicable

Documentos vigentes: **Spotify Developer Terms, versión 10, efectiva 15 may 2025** ([developer.spotify.com/terms](https://developer.spotify.com/terms)) y **Spotify Developer Policy, efectiva 15 may 2025** ([developer.spotify.com/policy](https://developer.spotify.com/policy)). No se observó una versión posterior a nov 2026. **[V]**

### 5.1 Textos citados

**Definición de Streaming (Terms, II.14):** **[V]**

> «"Streaming" means using the Spotify Platform to enable playback of sound recordings available through the Spotify Service, including using the Spotify Platform to control a background Spotify application.»

Esto es clave: controlar la app de Spotify de fondo **ya cuenta como Streaming**, aunque la app no reproduzca audio ella misma.

**Premium obligatorio (Policy, IV.1):** **[V]**

> «Streaming of music sound recordings through the Spotify Platform shall only be made available to subscribers to the Premium Spotify Service.»

**Webcasting no interactivo (Policy, III.4):** **[V]**

> «Do not create any product or service which includes any non-interactive internet webcasting service. For example, you can't create an application which plays content from a single source to several simultaneous listeners.»

**Replicar la experiencia core (Policy, III.11):** **[V]**

> «Do not build products or services that mimic, or replicate or attempt to replace a core user experience of Spotify or its group companies without our prior written permission. Your product or service must add independent value or functionality that improves users' interactions with Spotify.»

**Uso comercial (Policy, IV.2):** **[V]**

> «Except for the limited commercial uses for Non-Streaming SDAs (set out below) commercial uses are not permitted for SDAs.»

Ejemplos prohibidos: vender el SDA o el acceso, e-commerce iniciado en el SDA, venta de publicidad/patrocinios en el SDA.

**Sincronización con vídeo (Policy, III.6):** **[V]**

> «Do not synchronize any sound recordings with any visual media, including any advertising, film, television program, slideshow, video, or similar content.»

Esto es sincronización **audio↔vídeo**, no sincronización de audio entre dispositivos. No aplica directamente a una Jam, pero sí a un modo "watch party" o a visualizadores que pretendan sincronizar audio con vídeo.

**Mezcla/solape (Policy, III.7):** **[V]**

> «Do not permit any device or system to segue, mix, re-mix, or overlap any Spotify Content with any other audio content (including other Spotify Content).»

Aplica a crossfade, gapless "inteligente" y mezclas entre tracks. Un crossfade de usuario en el cliente de Spotify no es lo mismo que una app que mezcla streams; pero conviene no implementar transiciones automáticas entre canciones.

**Límites de rate (Terms, VI.2):** **[V]**

> «Spotify may limit the number of service calls that your SDA may make, the volume of Spotify Content that may be accessed, or anything else about the Spotify Service as Spotify deems appropriate, in its sole discretion, without notice. Spotify may use technical measures to prevent over-usage or stop usage of the Spotify Platform.»

**Reverse engineering (Terms, IV.2.1.b):** **[V]**

> «…modifying, editing, altering, creating derivative works, disassembling, decompiling, reverse-engineering, or extracting source code from the Spotify Platform (including any client libraries), Spotify Service, or Spotify Content (except to the extent such restrictions are expressly prohibited by law).»

**Notas del Web Playback SDK:** **[V]**

> «Streaming applications may not be commercial»
> «Keep audio content in its original form»
> «Do not synchronize Spotify content»
> «Spotify content may not be broadcasted»

La frase «Do not synchronize Spotify content» es ambigua: en el contexto del SDK suele leerse como no sincronizar audio con vídeo, pero su literalidad es más amplia. Un abogado de Spotify podría usarla contra un modo de sync multi-dispositivo. Hay que citarla y no ignorarla.

### 5.2 ¿Encaja un proyecto self-hosted sin ánimo de lucro con playback en el dispositivo de cada usuario?

**Respuesta corta:** es defendible, pero **no está autorizado**. El riesgo principal no es el webcasting, es III.11.

| Cuestión | Lectura favorable | Lectura de riesgo | Peso |
|---|---|---|---|
| ¿Hay retransmisión de audio? | No: cada usuario recibe su propio stream de Spotify en su dispositivo; la app solo intercambia mensajes de control y metadatos | III.4 dice «plays content from a single source to several simultaneous listeners»; la fuente es Spotify y los oyentes son varios, aunque cada uno tenga su licencia | Medio |
| ¿Es webcasting no interactivo? | No: los usuarios eligen, añaden y votan; es interactivo | La definición de la Policy no depende de la interactividad, sino del ejemplo «single source → several listeners» | Medio |
| ¿Replica la experiencia core? | Añade valor independiente: TUI, cola con votos, estadísticas, self-hosting, integración con otras fuentes | Jam es una experiencia social core de Spotify; un clon self-hosted la replica y compite con ella | **Alto** |
| ¿Premium? | Todos los que reproducen tienen Premium | Los invitados Free no pueden reproducir; la app debe bloquearlo, no sortearlo | Bajo si se implementa bien |
| ¿Comercial? | Sin ánimo de lucro, sin venta de acceso, sin publicidad | Donaciones, Patreon o una instancia pública con cuotas podrían considerarse comercial | Medio |
| ¿API documentada? | Usa Web API/Connect para el playback del host | spotatui ya usa una sesión nativa no documentada desde 0.47; `spotify_cli` y GraphQL son aún más grises | **Alto** |

**Conclusión política:** el modelo **más limpio** es el de un solo dispositivo que reproduce (host/jukebox) y una web o TUI donde los invitados solo añaden a la cola. Ahí no hay varios streams simultáneos, no hay sync de audio y los invitados ni siquiera necesitan cuenta. El modelo **más expuesto** es el sync multi-dispositivo con playback en cada usuario, porque se parece demasiado a Jam y activa III.11 y, según cómo se argumente, III.4.

**Mitigaciones realistas** (no eliminan el riesgo, lo reducen):

- No usar «Jam» en el nombre, ni el logo/marks de Spotify. La Policy VI prohíbe nombres que empiecen por «Spot» o que confundan.
- No retransmitir audio ni vídeo; solo metadatos y comandos.
- Exigir Premium a quien reproduzca y bloquear cuentas Free.
- No operar como servicio público ni cobrar por acceso.
- Añadir valor independiente visible: TUI, cola con votos, historial, estadísticas, self-hosting, fuentes múltiples.
- No usar endpoints reverse-engineered ni automatizar cuentas.
- Documentar que es un proyecto personal, no afiliado, y que cada usuario usa su propia cuenta.

---

## 6. Conclusión

### 6.1 ¿Existe ya un «Spotify Jam dentro de spotatui»?

**No.** Lo que existe desde 0.48.0 es una **Listening Party**, que es un mecanismo distinto: **[V]**

| Capacidad de Jam | ¿La tiene spotatui 0.48? | Evidencia |
|---|---|---|
| Crear una sesión Jam de Spotify | **No** | No hay llamada a Jam; el relay es propio |
| Unirse a una Jam por enlace `socialsession` | **No** | No hay manejo de ese esquema |
| Ver miembros de la Jam de Spotify | **No** | La sala tiene nombres de spotatui, no de Spotify |
| Permisos de Jam (`queue-only`, volumen) | **No** | Solo `host_only`/`shared_control` propios |
| Cola compartida: añadir, reordenar, quitar | **No** | `SyncMessage` no tiene mensajes de cola; el invitado ve «The host picks the songs» |
| Varios usuarios escuchan el mismo track a la vez | **Sí** | `SyncState` cada 2 s + seek si drift >3 s |
| Invitado puede play/pause/skip | **Sí** (desde 0.48) | Changelog 0.48, issue #442 |
| Invitado necesita Premium | **Sí** | docs/listening-party.md |

Si por «Spotify Jam» se entiende **"varios usuarios escuchando sincronizados con una sala y un código"**, spotatui ya lo tiene. Si se entiende **"la Jam de Spotify, con su cola compartida y su sesión"**, no lo tiene y no puede tenerlo con la API pública.

### 6.2 El gap real

El gap no es la sincronización. Es la **cola compartida**, y detrás de ella hay cuatro piezas:

1. **Cola propiedad de la sala, no del host.** Hoy la cola es la cola de Spotify del host. Los invitados no pueden añadir, reordenar ni quitar. Es la diferencia funcional más grande con Jam.
2. **Resolución de la cola a playback.** Alguien tiene que convertir esa cola en reproducción: el host con `add to queue`, o un dispositivo Connect, o el motor nativo de spotatui. Es el trabajo de ingeniería real.
3. **Interoperabilidad con Jam nativa.** Solo posible por `spotify_cli` (macOS/Windows) o GraphQL reverse-engineered (todos, pero ilegal según Terms IV.2.1.b). No es una base seria para Linux/TUI.
4. **Modo invitado sin Premium.** Jam permite unirse en persona a usuarios Free; una app de terceros no puede hacerles reproducir, pero sí puede dejarles añadir a la cola sin cuenta. Eso abre un modo "jukebox" que spotatui hoy no tiene.

Gaps secundarios: votos, historial de sala, invitaciones por enlace/QR (solo código), descubrimiento de salas, persistencia de la sala sin host, y reproducción independiente en cada invitado en vez de follow-host.

### 6.3 La vía legal/técnica más viable

**Recomendación: extender la Listening Party con una cola de sala, no intentar integrar Jam.**

Arquitectura propuesta:

1. **Relay:** añadir al `SyncMessage` mensajes `QueueState`, `QueueAdd`, `QueueRemove`, `QueueMove`, `QueueVote`. La sala mantiene la cola canónica con IDs estables y un `revision` monotónico. El host es el único que resuelve la cola a playback.
2. **Host:** al cambiar la cola, el host llama a `add to queue` de Spotify (Web API o sesión nativa) o a su cola nativa de spotatui. La reproducción sigue siendo del host: un solo stream, sin sync de audio entre dispositivos.
3. **Invitados:** search con su propia cuenta (Premium) o, en modo jukebox, search proxy con la cuenta del host y sin reproducción. Añaden a la cola; no reproducen.
4. **Modo remoto opcional:** mantener el `SyncState` actual para quienes quieran escuchar en su dispositivo. Es el modo más expuesto políticamente; etiquetarlo como experimental y no convertirlo en el único.
5. **Adaptador Jam nativa (opcional, no núcleo):** detectar `spotify_cli`, habilitar `jam create/status/members/permissions` solo en macOS/Windows, con un aviso claro de que es una función no documentada y no soportada. Nunca en Linux, nunca como dependencia dura.
6. **No hacer:** GraphQL reverse-engineered, automatización de cuentas, `socialsession` tokens, retransmisión de audio, uso comercial.

**Por qué esta vía:** reutiliza el 80 % de lo que spotatui ya tiene (relay, sala, sync, UI, licencia MIT), evita depender de una API que no existe, y mantiene el playback en un solo dispositivo, que es la configuración con menor exposición legal. La cola compartida es, además, la funcionalidad que ningún competidor de TUI ofrece.

### 6.4 Veredicto final

- **¿Existe ya el proyecto?** No como "Jam en spotatui". Existe una fiesta de escucha sincronizada (spotatui Listening Party) y existen muchos jukebox web (OpenDJ, CrowdRoom, Spartify, collabqueue) y varios sync multi-dispositivo (spicetify-jam, listen-together, syncwave).
- **¿Qué gap queda?** Una **cola compartida dentro de una TUI de Spotify**, con playback resuelto por el host y, opcionalmente, sync remoto. Es un gap estrecho pero real, y es exactamente la primitiva que define una Jam.
- **¿Merece la pena?** Sí, si el alcance se limita a la cola y se evita depender de Jam nativa. No, si el objetivo es "crear una Jam de Spotify" desde spotatui: eso requiere una API que Spotify no ha publicado y que, según sus propias palabras, no piensa publicar en la Web API.

---

## Anexo A — Verificaciones y datos no confirmados

**Verificado en fuente primaria (11 nov 2026):**

- Web API sin endpoints de Jam; lista completa del Player.
- Web Playback SDK sin métodos de Jam; notas de política.
- Developer Policy y Developer Terms, versiones y textos citados.
- Quota modes: 5 usuarios en Development Mode; extended solo organizaciones con ≥250k MAU.
- Changelogs de febrero y julio 2026.
- `spotify_cli jam` con 7 subcomandos, sin `join`.
- `spotify-cli-mcp`: MIT, 51 herramientas, 7 de Jam, macOS.
- librespot #1576: abierto, cronología y último comentario sin respuesta.
- spotatui 0.48.0: release, changelog, docs, PR #684, issue #442, enum `SyncMessage` sin cola.
- Búsquedas de issues: 0 resultados para `jam`, ninguno de cola compartida.

**Parcial o no confirmado:**

- `spotify_cli` en Linux: sin ruta ni binario confirmado.
- `spotify_cli` en el instalador clásico de Windows: no confirmado.
- `jam join` en `spotify-cli-mcp`: el README lo insinúa, la tabla de herramientas no lo lista.
- `librespot` reproduciendo una Jam: testimonio de un usuario de Raspotify, sin test de integración.
- `spot-along`: creado el 27 jul 2025, pero sin fecha de último commit verificada; licencia no verificada.
- `aura-tui`: fork creado el 5 mar 2026; licencia y actividad no verificadas.
- `Kyzenkms/spicetify-jam`: el README dice MIT, pero GitHub no detecta un archivo LICENSE.
- `infpdev/syncify`: el repositorio no fue accesible vía API en la consulta; se cita solo por el resultado de búsqueda y su propio README.
- La frase «Do not synchronize Spotify content» del WSDK: interpretación contextual, no aclarada por Spotify.

**Proyectos descartados por falta de licencia o por estar muertos:** collabqueue, Spartify, EchoNest, Jammix, spot-along, JamThing. Se pueden estudiar, no copiar.
