# 03 · Viabilidad técnica de una jam multiusuario para spotatui

**Fecha:** 11 de noviembre de 2026
**Alcance:** evaluar una cola compartida multiusuario sobre spotatui 0.48 (Rust + Ratatui + librespot), con reproducción sincronizada entre participantes.
**Método:** lectura de documentación oficial de Spotify, del código de spotatui 0.48 y de su relay, y de implementaciones de referencia (librespot, go-librespot). No se clonó ningún repositorio; se leyeron archivos publicados y documentación web el 2026-11-11.

**Convenciones**

| Etiqueta | Significado |
|---|---|
| **[H]** | Hecho verificado en documentación oficial o en código fuente publicado. |
| **[S]** | Suposición razonada, no confirmada por una fuente primaria. |
| **[I]** | Inferencia técnica a partir de hechos verificados. |

> **Veredicto corto:** sí es viable, pero no como “Spotify Jam para spotatui”. La vía correcta es extender el **Listening Party** que ya existe en 0.48 con una **cola autoritativa en el host** y un protocolo v2 compatible. La API pública de Spotify no permite construir la cola (solo append, sin push, sin reordenar, con cuota de 5 usuarios en desarrollo). La Jam nativa no tiene API pública. Connect no es multi-cuenta. El transporte y la sincronización ya están resueltos; el trabajo real es el modelo de cola, permisos y validación.

---

## 0. Resumen ejecutivo

1. **La Web API sirve para controlar reproducción, no para construir una cola compartida.** `POST /me/player/queue` solo añade al final; no hay endpoint para borrar, mover ni reordenar; `GET /me/player/queue` no es una foto completa del contexto y no hay eventos push. [H]
2. **El modo desarrollo de Spotify limita a 5 usuarios autenticados y exige Premium al dueño de la app.** El modo extendido es solo para organizaciones con ~250 000 MAU y revisión de hasta 6 semanas. Un servidor self-hosted para amigos cabe en desarrollo; un producto público, no. [H]
3. **Spotify Connect no es una sesión multi-cuenta.** Un dispositivo reproduce la sesión de una cuenta a la vez; otra cuenta requiere transferir el playback. Jam es una capa social de Spotify encima de Connect, no una funcionalidad de Connect que se pueda replicar con `device_id`. [H/S]
4. **La Jam nativa no es programable desde la API pública.** Existe ingeniería inversa suficiente para que un cliente librespot *reproduzca* una Jam iniciada desde la app oficial (go-librespot #397), pero no está demostrado que se pueda *crear o unirse* a una Jam desde librespot. [H]
5. **spotatui 0.48 ya tiene resuelta la parte más difícil:** relay WebSocket, host autoritativo de playback, estado cada 2 s, corrección de drift > 3 s, control compartido, código de 6 caracteres y límites de sala. Falta la cola. [H]
6. **El límite de 4 KB por mensaje del relay no es un blocker, pero obliga a un wire format compacto y a snapshots por chunks.** Un `TrackInfo` completo ronda 400–600 bytes; 8–10 por mensaje. Un item compacto ronda 110–160 bytes; 25 por chunk. [H/I]
7. **Recomendación v1:** `Party Queue v2` **in-process** en spotatui, host autoritativo, relay extendido, sin Web API para la cola, sin daemon separado, sin CRDT. Esfuerzo estimado: **6–9 semanas** de un desarrollador Rust con experiencia en el código; se puede entregar en fases.

---

## 1. La API pública de playback

### 1.1 Endpoints exactos

Todos los endpoints de reproducción requieren OAuth 2.0 con token de usuario. Salvo que se indique lo contrario, **solo funcionan con Premium** y devuelven `204 No Content` cuando el comando se acepta (no cuando se ejecuta). [H]

| Método y ruta | Función | Scope | Premium | Respuesta | Notas |
|---|---|---|---|---|---|
| `GET /me/player` | Estado de reproducción y dispositivo activo | `user-read-playback-state` | No para leer; sí para controlar | `200`, `204` si no hay dispositivo | Devuelve `device`, `timestamp`, `progress_ms`, `is_playing`, `item`, `actions`. |
| `GET /me/player/queue` | Cola del usuario | `user-read-currently-playing` + `user-read-playback-state` | No para leer | `200` | `currently_playing` + `queue`; la cola puede estar vacía; sin paginación documentada. |
| `POST /me/player/queue` | Añadir item a la cola | `user-modify-playback-state` | **Sí** | `204` | `uri` debe ser track o episode; `device_id` opcional; añade al final en la práctica; orden no garantizado con otros endpoints. |
| `PUT /me/player/play` | Iniciar/reanudar | `user-modify-playback-state` | **Sí** | `204` | Acepta `context_uri`, `uris`, `offset`, `position_ms`; `device_id` opcional. |
| `POST /me/player/next` | Siguiente | `user-modify-playback-state` | **Sí** | `204` | `device_id` opcional. |
| `POST /me/player/previous` | Anterior | `user-modify-playback-state` | **Sí** | `204` | `device_id` opcional. |
| `PUT /me/player/seek` | Buscar posición | `user-modify-playback-state` | **Sí** | `204` | `position_ms`; `device_id` opcional. |
| `PUT /me/player/pause` | Pausar | `user-modify-playback-state` | **Sí** | `204` | — |
| `PUT /me/player` | Transferir playback | `user-modify-playback-state` | **Sí** | `204` | `device_ids` es un array, pero **solo se admite un id**; más de uno devuelve `400`. |
| `GET /me/player/devices` | Dispositivos disponibles | `user-read-playback-state` | No para leer | `200` | `id` no garantizado persistente; `is_restricted` implica que el dispositivo rechaza comandos. |
| `PUT /me/player/volume` | Volumen | `user-modify-playback-state` | **Sí** | `204` | `volume_percent` 0–100; `supports_volume`. |
| `PUT /me/player/repeat` | Repetición | `user-modify-playback-state` | **Sí** | `204` | `off`, `track`, `context`. |
| `PUT /me/player/shuffle` | Shuffle | `user-modify-playback-state` | **Sí** | `204` | `state` booleano (`true`/`false`). |

**Scopes mínimos para una jam de solo cola y playback:** `user-read-playback-state`, `user-modify-playback-state`, `user-read-currently-playing`. Si se leen playlists o biblioteca, añadir los scopes correspondientes. `streaming` solo aplica al Web Playback SDK; `app-remote-control` solo a los SDK móviles. [H]

### 1.2 Rate limits, cuotas y 429 (situación 2026)

| Aspecto | Hecho verificado |
|---|---|
| Ventana | El límite se calcula sobre una **ventana móvil de 30 segundos** por app. [H] |
| Número exacto | Spotify **no publica** el número; varía entre modo desarrollo y modo extendido. [H] |
| 429 | Incluye normalmente `Retry-After` en segundos; hay que esperar ese valor, no reintentar inmediatamente. [H] |
| Cuota separada | Los límites de tasa y las **cuotas de modo desarrollo** son mecanismos distintos; la cuota puede devolver `429` con `reason: "QUOTA_EXCEEDED"`. [H] |
| Modo desarrollo | Dueño con Premium; **5 usuarios autenticados**; allowlist; 1 Client ID (ampliado a **25** en julio de 2026); conjunto de endpoints reducido para apps nuevas desde febrero de 2026. Para apps existentes, la reducción de endpoints se pospuso el 9 de marzo de 2026, pero el Premium, el tope de 5 usuarios y el límite de Client IDs sí se aplican. [H] |
| Modo extendido | Solo **organizaciones** desde el 15 de mayo de 2025; servicio activo, ~**250 000 MAU**, presencia en mercados clave, viabilidad comercial; revisión de hasta **6 semanas**. [H] |
| Endpoints de playback | Siguen disponibles tras los cambios de febrero de 2026. [H] |
| Búsqueda | `GET /search` bajó el máximo de `limit` de 50 a **10** y el default de 20 a 5. [H] |
| Batch de metadatos | `GET /tracks`, `/albums`, `/artists`, `/episodes`, `/shows`, `/chapters` fueron **eliminados**; hay que pedir uno por id. [H] |
| Campos eliminados | `popularity`, `available_markets`, `linked_from`, `followers`, `country`, `email`, `product`, etc. [H] |

**Manejo correcto de 429** (pseudocódigo):

```text
on 429:
    retry_after = parse(Retry-After) or backoff_base * 2^attempt
    jitter = random(0, retry_after / 2)
    sleep(retry_after + jitter)
    if attempt >= 5: abrir circuito 60 s y degradar la UI
    nunca reintentar sin idempotencia (la operación pudo aplicarse)
```

Para una jam, la defensa práctica es **no usar la Web API en el camino caliente de la cola**. El host ya tiene una sesión librespot con API interna; la cola puede vivir en memoria y materializarse en playback sin gastar cuota. La Web API queda para login, metadatos puntuales y funciones auxiliares. [I]

### 1.3 Límites estructurales de la cola pública

| Limitación | Consecuencia para una jam |
|---|---|
| Solo `POST` para añadir; no hay `DELETE`, `PUT` ni reorder. [H] | No se puede borrar, mover ni votar con la API pública. La cola compartida tiene que ser **virtual**, propiedad de la app. |
| `POST` añade al final, no “next”. La descripción oficial dice “next”, pero el comportamiento observado y el modelo de `next_tracks` la sitúan al final de la cola de usuario. [H/S] | No sirve para insertar “ahora” ni para mantener orden social. |
| `GET /me/player/queue` no devuelve el contexto completo: si suena un álbum o playlist, la cola de usuario puede estar vacía y los próximos temas vienen del contexto. [S] | La app no puede reconstruir de forma fiable “lo que va a sonar” desde la API. |
| La cola devuelta puede estar limitada en la práctica (~20 items en reportes de comunidad), aunque la documentación no fija un máximo. [S] | No es una fuente de verdad para una cola de 100–200 items. |
| No hay push en la Web API: es pull. No hay webhooks, SSE ni WebSocket de playback públicos. [H] | Todo es polling; el estado llega con latencia y hay que estimar la posición. |
| “The order of execution is not guaranteed when you use this API with other Player API endpoints.” [H] | Enviar `play`, `queue` y `next` en rápida sucesión produce carreras; hay que serializar por dispositivo. |
| `device_id` no es persistente y puede cambiar. [H] | Cachear el id es un error; hay que refrescar `GET /me/player/devices` y tolerar `404/403`. |
| `is_restricted=true` rechaza comandos. [H] | Hay dispositivos que aparecen pero no se pueden controlar; la UI debe excluirlos. |
| `PUT /me/player` solo acepta un `device_id`. [H] | No existe “reproducir en dos dispositivos a la vez” desde la API. |

**Implicación:** la API pública no puede ser la capa de cola de una jam. Como mucho, es la capa de **materialización** en el dispositivo del host: la app mantiene su propia cola y empuja el siguiente track con `PUT /me/player/play` o con la API interna de librespot. [I]

Matiz importante para spotatui: la **sesión interna de librespot sí tiene un canal dealer** con actualizaciones de Connect, y el playback nativo ya recibe eventos. Eso no es API pública ni un contrato estable, pero explica por qué el host puede reaccionar a cambios de playback sin polling de la Web API. Para la cola de la party, la fuente sigue siendo el estado interno del host, no `GET /me/player/queue`. [H/I]

---

## 2. Spotify Connect y por qué Jam no es Connect

### 2.1 Modelo de dispositivos

- Un dispositivo Connect se registra con un `device_id` y un nombre; aparece en `GET /me/player/devices` para la cuenta que lo tiene disponible, e `is_active` indica cuál reproduce. [H]
- El `device_id` “es único y persistente hasta cierto punto”, pero **no está garantizado**; hay que refrescarlo. [H]
- El dispositivo tiene un estado (`is_active`, `is_private_session`, `is_restricted`, `volume_percent`, `supports_volume`). [H]
- La reproducción se transfiere con `PUT /me/player` y un solo `device_id`; el array existe por compatibilidad, pero no admite varios. [H]
- La descubribilidad por mDNS puede mostrar dispositivos de otras cuentas en la misma red, pero **controlar** requiere que el dispositivo esté en la sesión de esa cuenta o que se transfiera. [H/S]

### 2.2 ¿Pueden varias cuentas controlar el mismo dispositivo?

**No simultáneamente, con el modelo Connect estándar.** [S, apoyado en H]

El modelo es de **una sesión activa por dispositivo**. Si la cuenta B selecciona el mismo altavoz, se produce una transferencia: el dispositivo pasa a reproducir la sesión de B y la cuenta A deja de controlarlo. Algunos fabricantes (Sonos, por ejemplo) permiten vincular varias cuentas a su propia app, pero eso es gestión de cuentas del fabricante, no una sesión Connect multi-cuenta simultánea. [S]

Secuencia típica de “dos cuentas hacen play en el mismo dispositivo”: [S/I]

```text
Cuenta A (dueña de la sesión)          Dispositivo Connect          Cuenta B
        │                                     │                       │
        │  play ─────────────────────────────>│  reproduce A          │
        │                                     │<───── selecciona ─────│
        │                                     │  recibe transfer      │
        │<──── A deja de ser activa ──────────│  pasa a sesión B      │
        │  (la UI de A muestra otro device)   │  reproduce B          │
```

Lo relevante para el diseño: **el dispositivo no multiplexa cuentas**. La segunda cuenta no “añade” una sesión; **sustituye** la existente. Por eso una jam multi-cuenta no se puede construir con `device_id` + tokens de varias cuentas: el backend de Spotify tendría que mediar la sesión, que es exactamente lo que hace Jam. [H/I]

La excepción es Jam: cuando el dispositivo ya está en una Jam, la segunda cuenta **no lo transfiere**; se une a la sesión social y sus comandos se enrutan al host a través de `social-connect`. Esa es la diferencia funcional que hace que Jam sí permita varias cuentas sobre el mismo audio, y la razón por la que no se puede replicar solo con Connect. [H]

Lo que **no** existe en la API pública:

- Un `device_id` compartido por varias cuentas con permisos concurrentes.
- Una operación “añadir a la cola del dispositivo X” que acepte el token de un invitado y el dispositivo del host.
- Un evento que diga “el invitado Y añadió un tema” dentro de Connect.

### 2.3 Qué hace Jam por dentro

La documentación de soporte confirma el contrato de producto: [H]

- El host necesita Premium; los usuarios Free pueden unirse y añadir canciones en una Jam presencial; unirse en remoto requiere Premium.
- Los invitados pueden ver y **reordenar** la cola; el host puede desactivar “Let others change what’s playing” para que solo añadan.
- El host puede expulsar invitados y controlar el volumen; si el host se va, la Jam termina.

La implementación no es Connect estándar. La ingeniería inversa de go-librespot (#397, merged 2026-09-25) describe el mecanismo: [H]

- **`social-connect` actúa como controlador**: no es el altavoz quien habla el protocolo Jam.
- Las acciones de los participantes llegan al dispositivo como **comandos y transferencias Connect ordinarias**, enviadas desde un dispositivo virtual `social-connect-<session id>`.
- Cada edición de un invitado llega como una **transferencia de toda la Jam** con `retain_session: retain_original`, nombrando el track que ya suena; el dispositivo debe aplicarla sin recargar el stream.
- El inicio y fin de la Jam llegan como `set_options {"modes":{"jam":"on"|"off"}}`.
- El stream se reporta con `jam_session_id` (campo 64 de `RawCoreStream`) para que las métricas de Spotify sepan que es una escucha de Jam.

**Conclusión:** Jam es una **sesión social del backend de Spotify** que crea un controlador virtual y concede permisos a las cuentas invitadas. No es una funcionalidad de Connect que se pueda activar con `device_id`. Replicarla con la API pública es imposible; replicarla con la API interna exige crear/unir la sesión social, algo que go-librespot **no** hace: solo reproduce una Jam ya iniciada desde la app oficial. [H]

---

## 3. Opciones de reproducción para una jam

### 3.1 Opción A — Un host reproduce; los invitados solo controlan la cola

```text
  Invitado A ─┐
  Invitado B ─┼──> Servidor de sala ──> Host spotatui ──> Altavoz / DAC
  Invitado C ─┘         (cola)              (una cuenta Premium)
```

| Dimensión | Evaluación |
|---|---|
| Sincronía | **Perfecta por construcción**: hay un solo stream y un solo reloj de audio. |
| Latencia de cola | La que tenga el servidor; 100–400 ms es alcanzable. |
| Consistencia | Trivial: una sola cola, un solo escritor. |
| Cuentas | Solo el host necesita Premium; los invitados pueden no tener cuenta. |
| Complejidad | Baja en reproducción; media en servidor y permisos. |
| Riesgo legal | **Alto si se productiza**: la política prohíbe “non-interactive internet webcasting” y “plays content from a single source to several simultaneous listeners” (III.4), y “broadcast” de contenido (nota de los endpoints). Una sala privada en casa es un caso más gris, pero el texto es amplio. [H] |
| Single point | El host es el punto único: si se duerme, se cae o pierde red, la jam se detiene. |
| ¿Varios usuarios en la misma cola? | Sí, la cola virtual; no la cola de Spotify. |

**Veredicto:** técnicamente la más simple y la de mejor sincronía, pero la más expuesta en ToS si se convierte en producto. Para un uso privado y local es defendible; para una feature de spotatui, no es la recomendación.

### 3.2 Opción B — Cada usuario reproduce en su dispositivo, cola virtual y sincronización por reloj

```text
  Host spotatui ──┐
                  ├──> Relay WebSocket ──> estado de playback cada 1–2 s
  Invitado A ─────┤                          y operaciones de cola
  Invitado B ─────┘
       │
       └── cada uno reproduce en su propio dispositivo/cuenta Premium
```

| Dimensión | Evaluación |
|---|---|
| Sincronía | **Aproximada**: no es audio sample-accurate; depende de relojes, buffering y del arranque de cada player. Objetivo realista: p50 < 300 ms, p95 < 1,5 s. |
| Latencia de cola | 100–400 ms; la operación se ve antes de oírla. |
| Consistencia | Host autoritativo; los invitados replican. |
| Cuentas | Cada participante necesita su propia cuenta Premium y su propio dispositivo. |
| Complejidad | Media: relojes, drift, reconexión, presencia. |
| Riesgo legal | **Moderado**: no hay un único stream para varios oyentes, pero la nota “Do not synchronize Spotify content” de los endpoints es amplia. Es el mismo riesgo que ya asume el Listening Party de 0.48. [H/S] |
| Single point | El host sigue siendo autoridad, pero el audio de cada invitado es local; si el host cae, el audio de los invitados continúa, aunque la sala se cierra (0.48 no elige otro host). |
| ¿Varios usuarios en la misma cola? | **Sí, es el modelo natural**: una cola lógica compartida, N reproducciones. |

**Veredicto:** es la base correcta para spotatui. No promete la sincronía de Jam, pero ofrece la experiencia que el usuario pide (“varios usuarios en la misma cola”) sin depender de una sesión multi-cuenta que Spotify no expone.

> **Corrección de premisa:** Jammix no es un sistema de sincronización de playback por reloj. Su README dice explícitamente que “Spotify is the playback destination, not the source of truth” y que empuja las canciones a la cola del host; es decir, se parece más a la opción A con cola virtual. Echo Nest es una plataforma de datos musicales, no un protocolo de reproducción sincronizada. Ninguno de los dos aporta un modelo directo para B. [H]

### 3.3 Opción C — Jam nativa vía API no oficial

```text
  spotatui ──> endpoints internos de Spotify (socialsession / connect-state)
       │              │
       │              └── crea/une sesión social (no documentado)
       └──> playback librespot en el dispositivo del host
```

| Dimensión | Evaluación |
|---|---|
| Sincronía | La de Jam: coordinada por Spotify, buena, pero no necesariamente sample-accurate entre dispositivos. [S] |
| Latencia | La del backend de Spotify; buena. |
| Consistencia | La de Spotify; la app no controla la cola. |
| Cuentas | Host Premium; invitados con cuenta (remoto Premium). |
| Complejidad | **Alta e incierta**: hay que crear/unir la sesión social, mantener tokens, manejar versiones. No hay SDK Rust. |
| Riesgo legal | **Alto**: acceso no documentado, ingeniería inversa y posibles cambios unilaterales; los Developer Terms no cubren el acceso a endpoints internos. |
| Fragilidad | **Máxima**: endpoints internos cambian sin aviso; ya ha habido roturas de librespot por cambios de backend. |
| ¿Varios usuarios en la misma cola? | Sí, la de Jam, pero no la controla spotatui. |

**Veredicto:** no recomendable para v1. Además, el estado del arte no demuestra que se pueda **iniciar** una Jam desde librespot: go-librespot reproduce una Jam ya iniciada; la pregunta “Is there a way to start jam session from librespot?” sigue sin respuesta en el issue #1576 (julio de 2026). [H]

### 3.4 Opción D — Implementar Jam a nivel librespot/Connect

| Dimensión | Evaluación |
|---|---|
| Sincronía | La de Jam, si se replica el protocolo social. |
| Latencia | Buena, pero depende de un protocolo no documentado. |
| Consistencia | Compleja: transferencias `retain_original`, `set_options`, `jam_session_id`, uids de cola. |
| Cuentas | Host e invitados con cuenta; el dispositivo sigue siendo de una cuenta. |
| Complejidad | **Alta**: el lado playback está demostrado (go-librespot #397: 9 commits, varios fixes de transferencias, cola, modos y reporting). El lado “crear/unir Jam” no está demostrado. Portar a Rust y mantenerlo es un proyecto aparte. |
| Riesgo legal | Alto: mismo que C, más la obligación de mantener compatibilidad con un protocolo privado. |
| ¿Varios usuarios en la misma cola? | Sí, pero solo dentro de una Jam iniciada por la app oficial. |

**Veredicto:** es la única vía para “audio sincronizado oficial” sin depender de la app oficial para reproducir, pero **no resuelve el producto**: no hay forma conocida de crear la Jam desde spotatui, y el mantenimiento es caro. Si el objetivo fuese “que spotatui sea un altavoz de una Jam iniciada en el móvil”, el esfuerzo razonable sería **2–4 semanas** de port y pruebas; si el objetivo es “que spotatui inicie y gestione la Jam”, el esfuerzo es **desconocido** y el riesgo legal, alto. [S]

### 3.5 Comparativa

| Criterio | A · Host único | B · Cada uno reproduce | C · Jam no oficial | D · Jam en librespot |
|---|---|---|---|---|
| Sincronía de audio | Excelente (1 stream) | Aproximada (reloj) | Buena (Spotify) | Buena (Spotify) |
| Varios usuarios en la misma cola | Sí (virtual) | Sí (virtual) | Sí (la de Jam) | Sí (la de Jam) |
| Invitados sin cuenta | Sí | No para oír; sí para cola si se relaja el gate | No | No |
| Premium requerido | Solo host | Todos | Host + invitados remotos | Host + invitados |
| Complejidad de implementación | Baja-media | Media | Alta | Alta |
| Fragilidad | Baja | Media | Muy alta | Alta |
| Riesgo legal | Alto si se productiza | Moderado | Alto | Alto |
| Encaja con spotatui 0.48 | No (no hay modo “un solo altavoz” multi-cuenta) | **Sí, es el Listening Party** | No | Parcial |
| Recomendación | Solo caso privado | **V1** | No | No en v1 |

---

## 4. Diseño de sincronización

### 4.1 Relojes: qué usar y qué no

**No usar el `timestamp` de `GET /me/player` para extrapolar la posición.** Ese campo es “Unix ms cuando el estado cambió por última vez”, no “cuando se tomó la muestra”. Si un tema lleva 60 s sonando, `progress_ms` ya es ~60 000 y `timestamp` es ~60 s atrás; sumar ambos duplica el tiempo. [H/I]

Lo que sí funciona:

1. **Reloj monótono local** (`Instant` en Rust) para medir intervalos; nunca `SystemTime` para elapsed.
2. **Offset host↔invitado estilo NTP**, estimado sobre el propio WebSocket:
   - invitado → host: `t1` (envío), host recibe `t2`, host responde `t3`, invitado recibe `t4`;
   - `offset = ((t2 - t1) + (t3 - t4)) / 2`;
   - `rtt = (t4 - t1) - (t3 - t2)`;
   - mediana de 8 muestras, descartar outliers, repetir cada 60 s.
3. **PTP no aplica** en este contexto: no hay timestamping de hardware ni una red controlada; NTP + offset de aplicación es suficiente. Para LAN de estudio se podría usar PTP, pero Spotify no expone el reloj del DAC. [S]
4. **El relay puede ser una referencia común**: host e invitado estiman su offset contra el relay; es una alternativa si no se quiere ping directo, pero añade dos piernas y no elimina el RTT. [I]

En 0.48, `SyncState.timestamp` es `sync::now_ms()` del host (hora de envío), **no** el `timestamp` de Spotify. Eso evita el error de doble conteo descrito arriba, pero deja el skew entre relojes y la edad del snapshot sin corregir. La mejora de v1 es añadir `sampled_at_ms` y el offset estimado, no cambiar el transporte. [H/I]

### 4.2 Estimación de posición

Modelo correcto:

```text
pos_guest(t) = pos_sample
             + (t_guest_now + offset_host→guest - t_sample_host)
             - latencia_de_arranque_local
```

Donde `t_sample_host` es el instante en que el host **observó** `progress_ms`, no el instante en que envió el mensaje. Si el snapshot se toma cada 250 ms y se envía 100 ms después, esos 100 ms deben contarse. [I]

En el código actual de 0.48, `SyncState` lleva `position_ms` (progreso observado), `timestamp` (hora de envío del host) y el invitado suma `now - timestamp`, con tope de 5 s. Es una buena primera aproximación, pero: [H]

- usa relojes de pared de ambas máquinas, no un offset estimado;
- no descuenta la edad del snapshot;
- el invitado compara contra `song_progress_ms`, que puede estar stale por su propio polling;
- el umbral de 3 s permite un error visible antes de corregir.

Mejoras de v1:

| Cambio | Efecto |
|---|---|
| Añadir `sampled_at_ms` a `SyncState` (o calcular `position_ms` en el instante de envío). | Elimina el error por edad del snapshot. |
| Añadir `TimeSync`/`TimeSyncReply` y mantener `clock_offset_ms`. | Elimina el skew entre relojes. |
| Enviar estado cada **1 s** en vez de 2 s (configurable). | Reduce la staleness máxima. |
| Calcular la posición del invitado desde el player nativo, no desde `song_progress_ms`. | Evita falsos positivos de drift. |
| Umbral de seek **> 1,5 s** con deadband; track distinto → restart. | Menos seeks audibles, mejor alineación. |
| Medir la latencia de arranque local (comando → primer sample) y compensarla. | Mejora la alineación inicial. |

### 4.3 Corrección de drift

No se puede cambiar la velocidad de reproducción de Spotify (no hay `rate` en la API). Las opciones son:

1. **No hacer nada** si el drift es pequeño (< 250 ms): imperceptible para música.
2. **Seek** si el drift supera el umbral: produce un salto audible, pero alinea.
3. **Reiniciar el track** en la posición estimada si el track no coincide o el drift es enorme.
4. **Pausar y reanudar** a la vez: no garantiza alineación; el arranque de cada player tiene jitter.

Política recomendada:

| Drift | Acción |
|---|---|
| < 250 ms | Nada. |
| 250 ms – 1,5 s | Nada; registrar métrica. |
| 1,5 s – 5 s | `seek` a la posición compensada. |
| > 5 s o track distinto | `play` del track correcto en la posición compensada; si falla, `seek`. |
| Invitado pausado y host reproduce | `play` + `seek`. |
| Host pausado | `pause` en todos. |

### 4.4 Cola: autoridad, revisión e idempotencia

**Modelo recomendado: servidor autoritativo único (el host), no CRDT.**

```text
Invitado                     Relay                     Host (autoridad)
   │                          │                          │
   │ QueueOpRequest(op_id,    │                          │
   │   base_rev, op) ────────>│ ────────────────────────>│ valida
   │                          │                          │ aplica
   │                          │<──── QueueOpResult ──────│ rev++
   │<──── QueueDelta(rev) ────│                          │
   │                          │                          │
   │ (si gap de rev)          │                          │
   │ QueueSyncRequest(last_rev)──────────────────────────>│
   │<──── QueueSnapshot(chunks)──────────────────────────│
```

Reglas:

- `rev` es un entero monótono que incrementa el host por cada operación aceptada.
- `op_id` es ULID/UUIDv7; el host guarda los últimos 256 `op_id` para deduplicar reintentos.
- `base_rev` es opcional; si el invitado está muy atrasado (`rev - base_rev > 128`), el host responde con snapshot en vez de delta.
- El invitado **no aplica optimistamente** en v1: muestra “pendiente” hasta `QueueOpResult`/`QueueDelta`. La latencia típica (< 400 ms) no justifica el coste de rollback.
- Si el host no puede aplicar (URI inválida, cola llena, sin permiso, rate limit), devuelve `accepted: false` con `reason` y no incrementa `rev`.
- Un invitado que reconecta envía `QueueSyncRequest { last_rev }`; si el host conserva el log, replay de deltas; si no, snapshot.

### 4.5 CRDT vs servidor autoritativo

| Criterio | Servidor autoritativo (host) | CRDT |
|---|---|---|
| Consistencia | Fuerte, orden total por `rev`. | Eventual; requiere merge. |
| Complejidad | Baja: una máquina decide. | Alta: metadatos, tombstones, relojes vectoriales. |
| Edición offline | No (la cola no se puede reproducir offline de todos modos). | Sí, pero el playback no. |
| Conflictos | Llegan en orden; last-write-wins por operación. | Resolución automática, a veces contraintuitiva. |
| Escala | 8 participantes, un host: sobra. | Diseñado para muchos escritores; aquí no hace falta. |
| Auditoría | Log de operaciones simple. | Estado distribuido difícil de explicar. |
| Recomendación | **Sí para v1.** | Solo si en el futuro hay multi-host o edición offline real. |

Un CRDT (OR-Set + LWW por item) tendría sentido si la cola pudiera editarse sin host, o si varios hosts compitieran. En una jam, el audio ya depende de un host; no hay beneficio. [I]

**Conflictos concretos y resolución v1:**

| Conflicto | Resolución |
|---|---|
| Dos invitados mueven el mismo item | Llegan en orden al host; gana la última operación aplicada; el primero recibe `QueueDelta` con el resultado final. |
| Un invitado quita un item que otro está moviendo | Si el remove se aplica primero, el move se rechaza (`reason: "missing_item"`); si el move se aplica primero, el remove lo borra. |
| Dos añadidos del mismo URI | Se permiten duplicados por defecto (un tema puede repetirse); si `dedupe=true`, el segundo se rechaza con `reason: "duplicate"`. |
| Voto y reordenación simultáneos | El voto actualiza `score`; el host recalcula el orden solo al empujar el siguiente item, no en medio del actual. |
| Reintento de la misma op | `op_id` ya aplicado → se devuelve el `QueueOpResult` original, sin volver a aplicar. |
| Invitado atrasado | `base_rev` fuera de la ventana → snapshot completo en vez de delta. |

### 4.6 Presencia y reconexión

- El relay ya detecta `close` y emite `guest_left`/`room_closed`. [H]
- Añadir `participant_id` inmutable y `last_seen`; heartbeat cada 10 s; marcar stale a los 30 s. [I]
- El host mantiene `HashMap<participant_id, Participant>` con nombre, rol, última actividad y votos.
- Reconexión: el invitado reenvía `participant_id` + `resume_token`; el relay permite reattachear si el host sigue y el id no está activo; si no, nuevo id y el host lo trata como nuevo (pierde votos, no la cola).
- Si el host cae, la sala se cierra (comportamiento actual). Para v1 es aceptable; para v2 se puede elegir un nuevo host entre los invitados, pero eso ya requiere un modelo de consenso o un relay autoritativo. [I]

### 4.7 Modelo concreto para v1

| Decisión | Elección v1 | Motivo |
|---|---|---|
| Autoridad | Host spotatui | Un solo escritor; coincide con el código actual. |
| Transporte | WebSocket del relay existente | Ya funciona; bidireccional; bajo coste. |
| Estado de playback | `SyncState` cada 1 s + `TimeSync` | Mejor staleness y skew. |
| Estado de cola | `QueueSnapshot` por chunks + `QueueDelta` | 4 KB; late join; reconexión. |
| Orden | `rev` + `op_id` + `base_rev` | Idempotencia y detección de gaps. |
| Permisos | `queue_mode` separado de `control_mode` | Se puede permitir añadir sin permitir saltar. |
| Votos | 1 voto por participante e item, toggle | Simple, idempotente, sin sybil perfecto. |
| Persistencia | Ninguna en v1 | La sala vive mientras el host vive. |
| CRDT | No | Complejidad sin beneficio. |

---

## 5. Autenticación multiusuario

### 5.1 Cómo lo hace spotatui hoy

- Login con **cliente propio embebido** y **pairing page**: muestra un código, abre `spotify.com/pair`, el usuario aprueba y la app recibe credenciales. No requiere developer app ni client ID propio. [H]
- El login y la caché de audio se guardan en el directorio de caché; la escritura es atómica y privada (`0600` en Unix). [H]
- Spotify en spotatui requiere **Premium**; el login rechaza cuentas Free. [H]
- No es un flujo apto para un servidor: el cliente embebido identifica a la app, no a un servidor multi-usuario; usar ese client ID desde un backend sería impersonar la app y quedaría fuera de los términos. [H/I]

### 5.2 Comparativa de flujos

| Flujo | Secreto | Redirect | Multiusuario servidor | Almacenamiento | Uso recomendado |
|---|---|---|---|---|---|
| PKCE (authorization code + PKCE) | No | URI registrada (o loopback) | Sí, por usuario | Refresh token cifrado | Cliente web/móvil propio; servidor que guarda tokens. |
| Authorization code | Sí (client secret) | URI registrada | Sí | Refresh token cifrado | Backend confiable. |
| Client credentials | Sí | No | No (no hay usuario) | — | Daemons, no playback de usuario. |
| Pairing de spotatui | No | `spotify.com/pair` | No | Caché local | Desktop/TUI; no servidor. |
| Invitado sin cuenta | — | — | — | Solo sesión de sala | Cola y presencia, sin playback. |

PKCE, en pasos: el cliente genera `code_verifier` y `code_challenge`; abre la URL de autorización con los scopes; Spotify redirige a la URI registrada (o a `http://127.0.0.1:PUERTO/callback` en una app nativa) con un `code`; el cliente canjea `code + code_verifier` por access token y refresh token; el servidor guarda el refresh token cifrado y renueva el access token cuando expira (1 h). El client secret no se usa. [H]

### 5.3 Recomendación

**V1: no custodiar tokens de Spotify en el servidor.**

- El host usa su sesión local de spotatui (librespot).
- Cada invitado que quiera oír usa su propia sesión local de spotatui (Premium).
- El relay solo conoce `participant_id`, nombre y sala; no ve tokens de Spotify.
- Los invitados sin cuenta pueden **ver la cola y votar** si se relaja el gate actual; para añadir, o pegan un enlace/URI de Spotify, o usan una búsqueda proxy del host. No se les pide login.

Esto evita: almacenar refresh tokens, convertir el relay en un custodio de credenciales, y ampliar el radio de exposición si el relay se compromete. [I]

**Si más adelante hay cliente web:** PKCE con redirect registrado, refresh token cifrado con XChaCha20-Poly1305 y clave en OS keyring/KMS, rotación, revocación y “disconnect” visible. Nunca tokens en logs ni en el estado de la sala. [I]

**Esquema concreto si se custodiasen tokens (fase 2):**

- Registro `{user_id, spotify_user_id, scopes, created_at, last_used_at, token_ciphertext}`; sin email ni perfil.
- Cifrado AEAD con nonce por registro; clave maestra en OS keyring (Secret Service, Keychain, Credential Manager) o KMS.
- Rotación en cada refresh si Spotify devuelve un refresh token nuevo; revocación en “disconnect”; borrado a los 30 días de inactividad.
- Acceso solo desde el proceso servidor; prohibido en logs, métricas, trazas y en el Durable Object del relay.

**Búsqueda proxy del host:** si un invitado sin cuenta quiere añadir, el host puede buscar con su propia sesión. Cuidado: la Web API de desarrollo tiene 5 usuarios y `search` devuelve como máximo 10 resultados; la sesión interna de librespot evita esa cuota; conviene confirmar en la implementación qué camino usa el buscador de spotatui, pero la arquitectura ya permite resolver catálogo sin la Web API. [H/I]

### 5.4 Código de sala, QR e identidad

- Código de 6 caracteres `A–Z0–9`: 36⁶ ≈ **2,18 × 10⁹** combinaciones. Con 10 intentos de join por hora e IP, la fuerza bruta es inviable; el riesgo real es compartir el código. [H/I]
- El código es un **bearer token**: quien lo tiene entra. Para v1 es aceptable; para salas sensibles, añadir un `join_secret` en el fragmento del QR/link o un PIN de 4 dígitos. [I]
- QR: generar en la TUI con bloques Unicode o mostrar un deep link `spotatui://party/ABC123`; el relay no necesita saber el formato. [I]
- Identidad: `participant_id` inmutable + `display_name` mutable (máx. 32 en cliente, 64 en relay). El relay ya evita que un invitado se llame como el host; añadir sufijo si hay duplicados. [H]
- El nombre no es identidad: la atribución de cola y los votos van por `participant_id`. [I]

---

## 6. Backend

### 6.1 Lo que ya existe (y sus límites)

El relay de spotatui es un **Cloudflare Worker + Durable Object** por sala: [H]

| Parámetro | Valor actual | Implicación |
|---|---|---|
| Mensaje máximo | **4 KB** (`raw.length`, unidades UTF-16 de JS) | JSON no ASCII puede ocupar hasta ~3× en bytes; el presupuesto real es menor. |
| Participantes | **8** (host + 7 invitados) | Suficiente para una jam de amigos; no para un evento. |
| Inactividad | **5 min** sin mensajes → sala cerrada | El host manda heartbeat si no puede publicar playback. |
| Rate limit por IP | 10 creates/h, 10 joins/h; 1 host y 2 guests por IP | Mitiga abuso básico; no identifica usuarios. |
| Persistencia | Ninguna; estado en memoria del DO | Si el DO se reinicia, la sala muere. |
| Auth | Ninguna; el código es el secreto | No hay identidad fuerte ni permisos por usuario. |
| Relayed types | `sync_state`, `playback_command`, `set_control_mode`, `set_name`, `heartbeat` | Cualquier otro tipo se descarta en `default`. |
| Broadcast | `sync_state` del host a todos; `playback_command` del invitado al host si `shared_control` | No hay mensajes dirigidos a un solo invitado. |

### 6.2 Opciones de backend

| Opción | Pros | Contras | Veredicto |
|---|---|---|---|
| **Extender el Worker/DO actual** | Ya desplegado; mismo protocolo; coste casi cero; latencia global; TLS y DDoS incluidos | 4 KB; sin persistencia; vendor lock-in; límites de CPU del DO | **V1** |
| **Daemon Rust separado** (axum + tokio + tungstenite) | Control total; self-hosted; puede correr sin TUI; sin límite de 4 KB | Otro proceso, otro despliegue, otra auth; conflicto de dispositivo si embebe librespot; +1 hop | Solo si “host siempre encendido” es requisito |
| **Relay autoritativo con cola en el servidor** | Late join trivial; host puede dormir; multi-host | El servidor se vuelve fuente de verdad; auth, persistencia, privacidad, ToS; el host igual debe materializar | No en v1 |
| **Postgres** | Historial, cuentas, métricas | Ops y latencia para una sala de 5 min | No; SQLite si acaso |
| **CRDT** | Edición offline/multi-host | Complejidad | No |

**WebSocket vs SSE:** SSE solo empuja servidor→cliente; las operaciones de invitado exigirían un `POST` aparte, dos caminos de autenticación y más estado en el relay. WebSocket ya está implementado, es bidireccional y permite ping/offset y presencia en el mismo canal. Para v1, WebSocket; SSE no aporta nada. [I]

**SQLite/Postgres:** la sala vive mientras el host vive; no hay entidad durable que justifique una base de datos. SQLite sería útil solo para métricas locales o para un log de operaciones que sobreviva a un reinicio del host; Postgres, para cuentas e historial en una fase posterior. [I]

### 6.3 Topología recomendada

```text
                         Internet / LAN
  ┌──────────────┐        wss://relay        ┌──────────────────┐
  │ Host spotatui│◄─────────────────────────►│ Cloudflare Worker│
  │  - playback  │   sync_state / queue_*    │  Durable Object  │
  │  - PartyQueue│                           │  room ABC123     │
  └──────┬───────┘                           └────────┬─────────┘
         │ librespot                                  │
         ▼                                            │
   Spotify Connect / DAC                              │
                                                      │
  ┌──────────────┐        wss://relay                │
  │ Invitado A   │◄───────────────────────────────────┘
  │  - playback  │
  │  - réplica   │
  └──────────────┘
```

- **LAN:** el relay puede ser local (axum) o el Worker; para LAN pura, `ws://` con TLS opcional; para internet, `wss://` obligatorio.
- **Self-hosted:** axum + `tokio-tungstenite`, un actor por sala (`mpsc`), `DashMap` de salas, TLS con `rustls` o Caddy; SQLite solo para métricas/op log opcional.
- **Túnel:** Cloudflare Tunnel para exponer un axum local sin IP pública; Tailscale para un grupo privado. Ngrok no para producción.
- **Privacidad:** el relay no debe ver tokens de Spotify ni historiales; solo `participant_id`, nombre, URIs y estado de playback. [I]

---

## 7. Extensión del protocolo Listening Party 0.48 (vía recomendada)

Esta sección evalúa la vía recomendada como **extensión del protocolo existente**, no como greenfield: el host sigue siendo el único que reproduce y decide; el relay sigue siendo un enrutador sin estado de cola (no guarda los items, solo metadatos de sala); los invitados replican y proponen. La cola es un añadido aditivo al `SyncMessage`, no un segundo sistema.

### 7.1 Inventario actual

`SyncMessage` en `src/infra/network/sync.rs`: [H]

| Mensaje | Dirección | Relay | Tamaño típico |
|---|---|---|---|
| `room_created { code, control_mode }` | relay → host | Generado por el relay | ~80 B |
| `joined_room { host_name, control_mode? }` | relay → invitado | Generado por el relay | ~100 B |
| `guest_joined { name }` / `guest_left { name }` | relay → todos | Broadcast | ~60 B |
| `sync_state { track_uri, position_ms, is_playing, timestamp }` | host → invitados | Broadcast | ~150 B |
| `playback_command { action, from? }` | invitado → host | Solo si `shared_control` | ~100 B |
| `set_control_mode { control_mode }` | host → todos | Broadcast | ~60 B |
| `set_name { name }` | ambos → relay | El relay lo aplica | ~60 B |
| `heartbeat` | host → relay | Ignorado (solo mantiene viva la sala) | ~20 B |
| `error { message }` / `room_closed` | relay → cliente | — | ~60 B |

El relay **descarta silenciosamente** cualquier tipo que no esté en su `switch`; los clientes viejos **fallan al parsear** un tipo desconocido y lo ignoran con un warning. [H]

### 7.2 Mensajes nuevos propuestos (protocolo v2)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncMessage {
    // ... existentes ...

    /// Handshake de capacidades; el relay lo guarda y lo publica en joined_room.
    ParticipantHello {
        participant_id: String,
        protocol: u8,
        features: Vec<String>,
    },

    /// El invitado pide la cola completa desde una revisión.
    QueueSyncRequest { last_rev: u64 },

    /// Snapshot por chunks; `to` permite enviarlo solo a un invitado.
    QueueSnapshot {
        rev: u64,
        chunk: u16,
        chunks: u16,
        #[serde(default)]
        to: Option<String>,
        items: Vec<QueueItemWire>,
    },

    /// Operación de invitado; el host la valida y la aplica.
    QueueOpRequest {
        op_id: String,
        base_rev: u64,
        op: QueueOp,
    },

    /// Resultado dirigido al autor (aceptado o rechazado).
    QueueOpResult {
        op_id: String,
        rev: u64,
        accepted: bool,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        to: Option<String>,
    },

    /// Delta broadcast a todos; `by` es el participante que la originó.
    QueueDelta {
        rev: u64,
        op_id: String,
        by: ParticipantRef,
        op: QueueOp,
    },

    /// Reordenación completa por votos; lista de ids en orden.
    QueueReorder { rev: u64, item_ids: Vec<String> },

    /// Cambio de política de cola (solo host).
    SetQueueMode { mode: String },

    /// Error de cola no fatal.
    QueueError {
        #[serde(default)]
        op_id: Option<String>,
        code: String,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueueOp {
    Add { item: QueueItemWire },
    Remove { item_id: String },
    Move { item_id: String, after: Option<String> },
    Vote { item_id: String, up: bool },
    Clear,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueItemWire {
    pub iid: String,          // id estable del host, p.ej. "q17"
    pub uri: String,          // "spotify:track:..."
    pub title: String,        // máx. 120
    pub artist: String,       // máx. 120
    pub duration_ms: u32,
    pub added_by: String,     // participant_id
    #[serde(default)]
    pub votes: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParticipantRef {
    pub id: String,
    pub name: String,
}
```

Notas de diseño:

- `Move { after: Option<String> }` es más robusto que un índice: dos movimientos concurrentes no se pisan por el desplazamiento de índices; el host resuelve `after` a posición. [I]
- `QueueReorder` con una lista de ids es más simple que enviar N movimientos y cabe en 4 KB para 200 items (`"q123",` ≈ 6 B → ~1,2 KB). [I]
- `Vote` va dentro de `QueueOp`; el host conoce al votante por `ParticipantRef.id`, no por el nombre. [I]
- `QueueOpResult` se dirige solo al autor; `QueueDelta` se broadcastea. El relay necesita distinguir “a uno” de “a todos”: añadir `to` y usarlo también en `QueueOpResult`. [I]

**Mapeo con los nombres propuestos inicialmente:**

| Nombre propuesto | En este diseño | Motivo |
|---|---|---|
| `QueueState` | `QueueSnapshot` + `QueueDelta` | Un estado completo no cabe en 4 KB; se parte en chunks y se mantiene con deltas. |
| `QueueAdd` | `QueueOp::Add { item }` | La operación lleva el item compacto y el host le asigna `iid`. |
| `QueueRemove` | `QueueOp::Remove { item_id }` | Usa `iid`, no el índice, para sobrevivir a reordenaciones. |
| `QueueMove` | `QueueOp::Move { item_id, after }` | Ancla relativa; el host resuelve el índice. |
| `QueueVote` | `QueueOp::Vote { item_id, up }` | Un voto por participante e item; el host agrega. |
| `QueueSnapshot` | `QueueSnapshot { rev, chunk, chunks, items }` | Igual, con chunking y `to` opcional. |

### 7.3 El límite de 4 KB

Medición con JSON compacto (sin espacios), campo cortos: [I]

| Contenido | Tamaño | Chunks a 4 KB |
|---|---|---|
| `QueueItemWire` compacto (título/artista cortos) | ~120 B | — |
| `QueueItemWire` con título/artista largos | ~180 B | — |
| Snapshot de 10 items | ~1,2 KB | 1 |
| Snapshot de 25 items | ~3,0 KB | 1 |
| Snapshot de 50 items | ~5,9 KB | 2 |
| Snapshot de 100 items | ~11,7 KB | 3 |
| Snapshot de 200 items | ~23,4 KB | 6 (hasta 9 con textos largos) |
| `QueueOpRequest` | ~150–250 B | 1 |
| `QueueDelta` | ~200–300 B | 1 |
| `QueueReorder` 200 ids | ~1,2 KB | 1 |

Reglas:

1. **Nunca serializar `TrackInfo` completo** (20+ campos, `image_url`, `artist_refs`, etc.): un item ronda 400–600 B y solo caben 8–10 por mensaje. [H/I]
2. Limitar `title` y `artist` a 120 caracteres; truncar en el host; no enviar `image_url` en el snapshot (cada cliente la resuelve si la necesita).
3. Chunk de **20–25 items** o **3,8 KB** como máximo, lo que ocurra primero; `chunks` se calcula al construir el snapshot; el invitado ensambla por `rev` y `chunk`; si falta uno, pide reenvío.
4. El relay ya descarta mensajes > 4 KB **sin error**; el cliente debe medir `serde_json::to_string(&msg).len()` antes de enviar y partir. [H]
5. El relay mide `raw.length` en JS (unidades UTF-16), no bytes UTF-8. Para un presupuesto conservador, contar bytes UTF-8 y mantener el JSON en ASCII (URIs, ids); limitar los nombres a ASCII o contarlos aparte. [H/I]

### 7.4 `native_queue` como almacén: sí, con matices

La propuesta inicial es “host autoritativo usando `native_queue: Vec<TrackInfo>`”. Es la dirección correcta, pero hay que separar **estado de protocolo** de **estado de reproducción**: [H/I]

| Aspecto | `native_queue` actual | `PartyQueue` propuesta |
|---|---|---|
| Tipo | `Vec<TrackInfo>` | `Vec<PartyItem>` + `HashMap<iid, ...>` |
| Identidad | URI + posición | `iid` estable + `rev` |
| Atribución | No | `added_by`, `votes` |
| Fuente | Cross-source (local, Subsonic, radio, Spotify) | Solo Spotify (`spotify:track:`/`episode:`) |
| Consumo | El playback la drena (`drain`, `remove`, `move`) | El host la drena y **materializa** en `native_queue` |
| Serialización | Pesada | Wire compacto |

**Recomendación:** `PartyQueue` es la fuente de verdad; `native_queue` es su **materialización de playback**. Cuando el host está en modo “party queue”, el siguiente item se empuja a `native_queue` (o se llama a `start_playback_uris`) y el playback lo consume. Así se conserva la semántica de `queue_owns_playback`, el suspend/resume de contexto y el resto del código, sin contaminar `native_queue` con atribución y revisiones. [I]

Dos detalles de implementación:

- En builds sin la feature `queue`, `native_queue` no existe; la materialización debe caer a `start_playback_uris` y engancharse al evento de fin de track. La UI de cola de la party puede seguir funcionando aunque el playback se haga por URI.
- El avance no debe ser un timer: se engancha al mismo evento que ya usa `native_queue` (`AdvanceNativeQueue`) para saber cuándo el item sonó y recién entonces incrementar `rev`, quitarlo de `PartyQueue` y emitir `QueueDelta`. Un timer se desincroniza con pausas, seeks y buffering. [I]

Si se insiste en usar `native_queue` directamente como store, habría que añadir un mapa paralelo `iid → metadata` y aceptar que el playback la muta; es más frágil. No lo recomiendo.

Si el objetivo es minimizar el diff inicial, se puede empezar con `native_queue` como store y un mapa `iid → metadata`; funciona para una demo, pero el coste de migrar después a una cola con atribución y revisiones es mayor que separarlas desde el principio. La separación es barata porque `PartyQueue` es un módulo puro y `native_queue` ya existe.

**Semántica de reproducción:** para que la cola sea autoritativa, el host debe reproducir la cola, no un contexto de playlist/álbum. Al iniciar la party queue:

1. guardar el contexto actual (si lo hay) para restaurarlo al cerrar;
2. pausar el contexto;
3. reproducir el head de `PartyQueue` con `start_playback_uris(vec![uri])`;
4. al terminar, avanzar al siguiente item; si la cola está vacía, restaurar el contexto o pausar.

Esto es un cambio de producto deliberado: la jam sustituye la fuente de reproducción mientras está activa. [I]

### 7.5 Permisos y validación

**Política de cola separada del control de transporte:**

| `queue_mode` | Añadir | Quitar | Mover | Votar |
|---|---|---|---|---|
| `host_only` | Host | Host | Host | Host |
| `guests_add` | Host + invitados | Host | Host | Host |
| `guests_edit` | Host + invitados | Autor de su item | Autor de su item | Host |
| `vote` | Host + invitados | Host | Host (por ranking) | Todos |

- El host elige el modo al crear la sala y lo puede cambiar con `SetQueueMode`; el relay lo aplica y lo publica en `joined_room`. [I]
- El relay aplica el `queue_mode` que fijó el host y no acepta un modo enviado por un invitado; el host además revalida cada operación. [H/I]
- El relay debe añadir/sobrescribir el emisor (`from_id`) de cada `QueueOpRequest` con el `participant_id` de la sesión, no aceptar el que mande el cliente (igual que hace hoy con `from` en `playback_command`). El `by` del `QueueDelta` lo rellena el host desde su registro. [H/I]
- El host valida la URI:
  - solo `spotify:track:<22 base62>` o `spotify:episode:<22 base62>`;
  - rechazar `spotify:local:`, `spotify:playlist:`, `spotify:artist:`, URLs `open.spotify.com`, espacios, `javascript:`, etc.;
  - canonizar con el parser de ids de spotatui (`ids::playable_id`);
  - límite de longitud (128), cola máxima (200), dedupe configurable;
  - rate limit por participante (10 ops/min) y global (60 ops/min);
  - opcional: comprobar `is_playable` en el mercado del host antes de aceptar; si no, rechazar con `reason: "not_playable"`.
- El invitado no envía URIs directamente a la API de Spotify del host: solo el host las valida y las materializa en su playback. El invitado usa la URI para su propia reproducción, pero solo después de que el host la haya aceptado. [I]
- Si el invitado pega una URI, el host resuelve `title`, `artist` y `duration_ms` con la sesión interna (o `GET /tracks/{id}` como fallback) antes de aceptar; si no puede resolverla, la rechaza con `reason: "metadata_unavailable"`. [I]

### 7.6 Late join

```text
Invitado nuevo                Relay                     Host
     │                          │                        │
     │ join?code=ABC123&v=2 ───>│                        │
     │<── joined_room ──────────│ (host_features,        │
     │    {host_name,           │  participant_id,       │
     │     host_features,       │  queue_rev)            │
     │     participant_id,      │                        │
     │     queue_rev}           │                        │
     │                          │                        │
     │ QueueSyncRequest{0} ────>│───────────────────────>│
     │                          │                        │ snapshot chunks
     │<── QueueSnapshot(chunk0)─│<───────────────────────│
     │<── QueueSnapshot(chunk1)─│                        │
     │<── SyncState ────────────│                        │
     │                          │                        │
     │ (ensambla, pinta cola,   │                        │
     │  sigue playback)         │                        │
```

- El relay puede guardar `queue_rev` y `queue_size` de los últimos `QueueDelta`/`QueueSnapshot` para informar al nuevo invitado, pero **no la cola completa**; el host es la fuente. [I]
- Si el snapshot se envía a todos, un late join genera N×chunks de tráfico; usar `to: participant_id` para dirigirlo. [I]
- Si el invitado no recibe todos los chunks en 5 s, reintenta `QueueSyncRequest` con `last_rev`; el host reenvía desde el chunk que falte. [I]

### 7.7 Compatibilidad y migración

**Matriz de compatibilidad:**

| Cliente | Relay | Resultado |
|---|---|---|
| 0.48 (v1) | actual | Listening Party actual; sin cola. |
| 0.49 (v2) | actual (v1) | `room_created` sin `features` → la UI de cola se desactiva; playback sync sigue. |
| 0.48 (v1) | nuevo (v2) | Sigue funcionando; ignora campos nuevos; no recibe `queue_*` porque el host no los anuncia. |
| 0.49 (v2) | nuevo (v2) | Cola completa; los invitados v1 en la misma sala quedan en “solo escucha”. |

**Reglas de compatibilidad:**

1. **Campos nuevos, nunca renombrados:** serde ignora campos desconocidos por defecto; añadir `#[serde(default)]` a todo lo nuevo. [H/I]
2. **Tipos nuevos, nunca reutilizados:** un cliente v1 falla al parsear un tipo desconocido y lo descarta; no rompe, pero genera warning. Añadir una variante `Unknown` con `#[serde(other)]` si se quiere silenciar. [I]
3. **Handshake de capacidades:** `ParticipantHello { protocol: 2, features: ["queue_v1","vote_v1"] }`; el relay guarda las del host y las publica en `joined_room`; el invitado solo manda ops si el host las anuncia. [I]
4. **Versión en la URL:** `?v=2` en create/join; el relay sin `v` asume 1. [I]
5. **Relay primero, cliente después:** desplegar el relay v2 (retrocompatible) antes de publicar el cliente 0.49; el cliente detecta relay v1 por ausencia de `features`. [I]
6. **Feature flag:** `behavior.party_queue` (default `false` en 0.49, `true` en 0.50 tras métricas). [I]
7. **No romper v1:** el relay v2 debe seguir relayando `sync_state` y `playback_command` exactamente como hoy; los mensajes de cola son aditivos. [I]
8. **Migración de sala:** una sala creada por un host v1 no se puede “actualizar” a v2 sin recrearla; el host v2 debe crear una sala nueva. [I]

**Rollout sugerido:**

| Fase | Versión | Qué se despliega |
|---|---|---|
| 0 | relay v2 | Campos `features`, `participant_id`, `queue_*`; retrocompatible. |
| 1 | spotatui 0.49 | Handshake, snapshot/delta, cola host-only, feature flag off. |
| 2 | spotatui 0.50 | Cola para invitados (añadir/quitar/mover), permisos, feature flag on. |
| 3 | spotatui 0.51 | Votos, moderación, métricas; posible daemon headless. |

### 7.8 In-process vs daemon separado

| Criterio | In-process (recomendado) | Daemon separado |
|---|---|---|
| Acceso a `PartyQueue`/`native_queue` | Directo | Solo por IPC/CLI/MCP |
| Latencia | 0 hops extra | +1 hop |
| Auth | La sesión local del host | Duplicada o delegada |
| Si se cierra la TUI | La sala muere | La sala sigue |
| Despliegue | Un binario | Dos binarios, servicio, logs |
| Riesgo de conflicto de dispositivo | Bajo | Alto si embebe librespot |
| Testabilidad | Buena (módulo puro) | Media |
| Veredicto | **V1** | Solo para “altavoz siempre encendido” |

Si se opta por el daemon, que sea un **host headless** que reutilice el mismo `PartyQueue` y el mismo protocolo, no un segundo cerebro que controle a spotatui por CLI. La extracción es posible si el núcleo de cola se escribe como módulo puro (`apply(op, state) -> Result<Delta>`), sin dependencias de Ratatui. [I]

---

## 8. Métricas de éxito y plan de pruebas

### 8.1 Métricas

| Métrica | Definición | Objetivo v1 | Cómo se mide |
|---|---|---|---|
| Desviación de playback p50 | `abs(pos_host − pos_invitado)` en el instante de muestreo | **< 300 ms** | `SyncState` con `sampled_at` + reloj del invitado. |
| Desviación p95 | Percentil 95 | **< 1,5 s** | Muestras cada 5 s durante 30 min. |
| Desviación máxima | Peor caso | **< 3 s** | Misma muestra; excluir buffering inicial. |
| Latencia de operación de cola p95 | `QueueOpRequest` → `QueueDelta` visible | **< 400 ms** | `op_id` + timestamps cliente/relay. |
| Añadir visible | Botón → item en la lista de todos | **< 500 ms** | Igual. |
| Late join | Join → cola + playback pintados | **< 1 s** | Medir desde `joined_room`. |
| Reconexión | Corte → estado recuperado | **< 3 s** | `tc netem` + cierre de socket. |
| Pérdida de mensajes | Mensajes > 4 KB o descartados | **0** | Assert en cliente + logs del relay. |
| 429 de Web API | Ratio de respuestas 429 | **< 0,5 %** | Contador por sesión. |
| Carga de relay | Mensajes/min por sala | **< 120** | 8 participantes, 1 sync/s + ops. |
| Memoria del host | RSS extra por cola de 200 | **< 2 MB** | `heaptrack`/`/proc`. |

### 8.2 Plan de pruebas

1. **Dos clientes en LAN** (mismo host, misma hora): validar p50/p95 con 30 min de música.
2. **Tres clientes, uno remoto** con `tc netem delay 100ms 20ms loss 1%`: validar reconexión y drift.
3. **Skew de reloj** ±5 s en un invitado: validar `TimeSync`; sin él, el drift debe ser visible y corregible.
4. **Host duerme/pierde red**: la sala se cierra; el invitado debe mostrar “party ended” y no colgarse.
5. **Relay reinicia**: idem; el cliente debe reconectar o cerrar limpio.
6. **Tormenta de cola**: 100 ops en 10 s; validar rate limit, dedupe y orden.
7. **Replay de op**: mismo `op_id` dos veces; una sola aplicación.
8. **URI maliciosa**: `spotify:local:`, `file://`, `javascript:`, string de 10 KB; rechazo sin crash.
9. **Mensaje > 4 KB**: el relay lo descarta; el cliente debe chunkear y no perder la cola.
10. **Late join** con 200 items: 6–8 chunks; validar ensamblado y timeout.
11. **Invitado v1 en sala v2**: sigue oyendo; no ve cola; no rompe.
12. **Host v1 con relay v2**: la sala funciona como v1; no se envían ops de cola.

---

## 9. Recomendación y roadmap

### 9.1 Arquitectura v1 recomendada

```text
┌──────────────────────────── Host spotatui 0.49 ────────────────────────────┐
│  PartySession                                                              │
│   ├── PartyQueue (autoridad): Vec<PartyItem>, rev, op_log, voters          │
│   ├── materializador: PartyQueue.head() -> native_queue / start_playback   │
│   └── SyncState cada 1 s + TimeSync                                        │
│                                                                            │
│  src/infra/network/sync.rs: SyncMessage v2 + QueueItemWire + QueueOp       │
│  src/infra/network/mod.rs: handlers, validación, permisos                  │
└───────────────────────────────┬────────────────────────────────────────────┘
                                │ wss
┌───────────────────────────────▼────────────────────────────────────────────┐
│  worker-relay v2 (Cloudflare DO)                                           │
│   ├── room state: host, guests, participant_id, queue_mode, queue_rev      │
│   ├── relay: sync_state, playback_command, queue_op_request, queue_delta   │
│   ├── direct: queue_snapshot/queue_op_result con `to`                      │
│   └── límites: 4 KB, 8 participantes, 5 min, rate limit por IP/sesión      │
└───────────────────────────────┬────────────────────────────────────────────┘
                                │ wss
┌───────────────────────────────▼────────────────────────────────────────────┐
│  Invitado spotatui 0.49                                                    │
│   ├── réplica de PartyQueue (rev + deltas)                                 │
│   ├── playback local (librespot) + SyncState                               │
│   └── UI: cola, votos, “añadir”, atribución                                │
└────────────────────────────────────────────────────────────────────────────┘
```

**Decisiones cerradas:**

- Host autoritativo; sin CRDT; sin Web API para la cola.
- `PartyQueue` separada de `native_queue`; `native_queue` es la materialización.
- Relay extendido, no daemon; protocolo v2 con handshake y feature flags.
- Invitados con su propia cuenta Premium para oír; cola visible para todos; añadir/votar según `queue_mode`.
- Sin persistencia; la sala vive mientras el host vive.

### 9.2 Roadmap

| Fase | Semanas | Entregable | Criterio de salida |
|---|---|---|---|
| 0 · Spike | 1 | `SyncMessage` v2 + relay v2 en local; `TimeSync`; snapshot chunked | 200 items en < 1 s; 0 mensajes > 4 KB. |
| 1 · Cola host | 2 | `PartyQueue`, `apply`, materialización, UI host | Añadir/quitar/mover en host; playback estable. |
| 2 · Invitados | 2 | `QueueOpRequest/Result/Delta`, permisos, UI invitado | p95 op < 400 ms; sin duplicados. |
| 3 · Late join/reconnect | 1 | Snapshot dirigido, `last_rev`, heartbeat, presencia | Join < 1 s; reconexión < 3 s. |
| 4 · Votos/moderación | 1 | `Vote`, `QueueReorder`, kick, rate limit | 1 voto/persona/item; orden estable. |
| 5 · Hardening | 1–2 | Tests de red, skew, 429, seguridad, métricas | Tabla de métricas cumplida. |

**Total:** 8–9 semanas de trabajo efectivo; 6–8 de calendario si se solapan las fases 0–2. Recorte a 2–3 semanas para “host-only + añadir al final”, sin votos ni reorder.

### 9.3 Qué no hacer en v1

- **No** usar `POST /me/player/queue` como store: no permite borrar/mover y no refleja el contexto.
- **No** intentar crear/unir una Jam nativa: no hay API y el riesgo legal/fragilidad es alto.
- **No** construir un daemon separado: duplica auth y playback; se puede extraer después.
- **No** introducir CRDT: un host y 8 invitados no lo necesitan.
- **No** serializar `TrackInfo` completo: rompe el límite de 4 KB y expone campos innecesarios.
- **No** prometer sincronía sample-accurate: es reproducción coordinada, no un reloj de audio compartido.

### 9.4 Riesgos y mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|---|---|---|---|
| Cambio de política/ToS de Spotify | Media | Alto | No comercializar; no llamarlo “Jam”; no usar marcas; mantener la función local/self-hosted; revisión legal antes de un rollout amplio. |
| Rotura de la API interna de librespot | Media | Alto | Pin del fork de librespot; tests de integración; feature flag para desactivar la party; fallback a playback por URI. |
| Abuso de salas (código compartido, spam) | Media | Medio | Rate limit por IP y por participante; validación de URIs; cola máxima; kick; códigos de 6 chars con expiración. |
| Deriva de sync por buffering | Alta | Medio | Deadband; seek solo > 1,5 s; métricas; no prometer sample-accurate. |
| Host único | Media | Alto | Documentar; para v2, failover o daemon headless. |
| Mensajes > 4 KB | Baja | Medio | Chunking 20–25 items / 3,8 KB; assert en cliente; test de 200 items. |
| Fuga de tokens al relay | Baja | Alto | El relay no recibe tokens; solo participant_id, nombre y URIs. |
| Confusión con Spotify Jam | Media | Medio | Nombre propio (“Party Queue”), atribución, no replicar la UX oficial. |

### 9.5 Cuándo cambiar de decisión

| Si el requisito es… | Entonces… |
|---|---|
| “Un solo altavoz para una sala, invitados sin cuenta” | Opción A, con revisión legal por “webcasting/broadcast”. |
| “Audio oficial de Jam, sin depender de la app oficial para reproducir” | Opción D, portar el soporte de go-librespot #397; asumir mantenimiento. |
| “Iniciar/gestionar Jam desde spotatui” | No es viable hoy con API pública; la vía interna es un proyecto de investigación aparte. |
| “Más de 5 usuarios con Web API” | Bloqueado por cuota de desarrollo; la vía es API interna o modo extendido (no aplicable a hobby). |
| “Host siempre encendido, TUI cerrada” | Daemon headless que reutilice `PartyQueue` y librespot. |

---

## Apéndice A · Hechos vs supuestos

### Hechos verificados [H]

- Endpoints, scopes, Premium y respuestas de playback de la Web API.
- Rate limit en ventana de 30 s, `Retry-After`, cuota separada y `QUOTA_EXCEEDED`.
- Modo desarrollo: 5 usuarios, Premium del dueño, 1 Client ID (25 desde julio de 2026); endpoints reducidos para apps nuevas y reducción pospuesta para las existentes (9 de marzo de 2026).
- Modo extendido: solo organizaciones, ~250k MAU, revisión de hasta 6 semanas.
- Eliminación de batch `GET /tracks` y límite de search 10.
- `GET /me/player/queue` puede devolver cola vacía; no hay endpoint de borrado/reorder.
- `PUT /me/player` solo acepta un `device_id`.
- Jam: host Premium; Free puede unirse y añadir en presencial; remoto Premium; invitados reordenan; host restringe; host se va → termina.
- go-librespot #397: `social-connect`, `social-connect-<session id>`, `retain_session`, `set_options modes`, `jam_session_id`.
- librespot #1576: Jams reproducen en Raspotify; iniciar Jam desde librespot no está resuelto.
- spotatui 0.48: 2 s, 3 s, 8 participantes, 4 KB, 5 min, código 6, relay URL, rate limits, `SyncMessage` y relay actuales.
- spotatui auth: cliente embebido, pairing, Premium, caché 0600.
- `native_queue: Vec<TrackInfo>`, `TrackInfo` pesado, `add_track_to_native_queue` y fallback a Web API en dispositivo externo.
- `party_yields_to_local_playback` y el gate de Spotify en join/start.

### Supuestos [S]

- Una segunda cuenta que selecciona el mismo dispositivo Connect lo transferencia y desplaza a la primera; el UX exacto varía por fabricante.
- `GET /me/player/queue` devuelve una ventana limitada (~20) y no incluye el contexto; no está documentado.
- La Jam de Spotify no es sample-accurate entre dispositivos; coordina por backend.
- RTT típico relay↔cliente: 20–80 ms en región, 100–250 ms intercontinental.
- El esfuerzo de 6–9 semanas para un dev Rust familiarizado con spotatui.
- El relay de Cloudflare es aceptable para un grupo privado; no se evalúa coste a escala.

### Inferencias [I]

- El límite de 4 KB en JS es de unidades UTF-16; para JSON no ASCII el presupuesto de bytes es menor.
- `PartyQueue` separada de `native_queue` reduce el acoplamiento y evita que el playback mutile la cola de protocolo.
- Un host autoritativo es suficiente para 8 participantes; un CRDT no aporta.
- Enviar estado cada 1 s y medir offset NTP-like mejora la sync sin cambiar el transporte.
- La cola virtual puede materializarse sin Web API usando librespot; la cuota de Spotify deja de ser el cuello de botella.

---

## Apéndice B · Fuentes consultadas (2026-11-11)

**Spotify oficial**

- Get Playback State — <https://developer.spotify.com/documentation/web-api/reference/get-information-about-the-users-current-playback>
- Get the User's Queue — <https://developer.spotify.com/documentation/web-api/reference/get-queue>
- Add Item to Playback Queue — <https://developer.spotify.com/documentation/web-api/reference/add-to-queue>
- Start/Resume Playback — <https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback>
- Skip To Next — <https://developer.spotify.com/documentation/web-api/reference/skip-users-playback-to-next-track>
- Transfer Playback — <https://developer.spotify.com/documentation/web-api/reference/transfer-a-users-playback>
- Get Available Devices — <https://developer.spotify.com/documentation/web-api/reference/get-a-users-available-devices>
- Scopes — <https://developer.spotify.com/documentation/web-api/concepts/scopes>
- Rate Limits — <https://developer.spotify.com/documentation/web-api/concepts/rate-limits>
- Quota Modes — <https://developer.spotify.com/documentation/web-api/concepts/quota-modes>
- Authorization — <https://developer.spotify.com/documentation/web-api/concepts/authorization>
- API Calls / errores — <https://developer.spotify.com/documentation/web-api/concepts/api-calls>
- Developer Policy — <https://developer.spotify.com/policy>
- February 2026 changelog — <https://developer.spotify.com/documentation/web-api/references/changes/february-2026>
- February 2026 migration guide — <https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide>
- July 2026 changelog — <https://developer.spotify.com/documentation/web-api/references/changes/july-2026>
- Blog 2026-02-06 — <https://developer.spotify.com/blog/2026-02-06-update-on-developer-access-and-platform-security>
- Start or join a Jam — <https://support.spotify.com/us/article/jam/>
- Spotify Connect — <https://support.spotify.com/us/article/spotify-connect/>

**Código y proyectos**

- spotatui 0.48 — <https://github.com/LargeModGames/spotatui>
- `src/infra/network/sync.rs` — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/src/infra/network/sync.rs>
- `src/infra/network/mod.rs` — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/src/infra/network/mod.rs>
- `src/core/app/party.rs` — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/src/core/app/party.rs>
- `src/core/app/queue.rs` — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/src/core/app/queue.rs>
- `src/core/plugin_api.rs` (TrackInfo) — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/src/core/plugin_api.rs>
- `worker-relay/src/index.ts` — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/worker-relay/src/index.ts>
- `docs/listening-party.md` — <https://raw.githubusercontent.com/LargeModGames/spotatui/main/docs/listening-party.md>
- go-librespot #397 (Jam support) — <https://github.com/devgianlu/go-librespot/pull/397>
- go-librespot #73 (queue uids) — <https://github.com/devgianlu/go-librespot/issues/73>
- librespot #1576 (Jam session support) — <https://github.com/librespot-org/librespot/issues/1576>
- Jammix (cola social, no sync de playback) — <https://github.com/junga3/jammix>

**Referencias técnicas**

- RFC 5905, Network Time Protocol v4 — <https://datatracker.ietf.org/doc/html/rfc5905>
- IEEE 1588, Precision Time Protocol — <https://standards.ieee.org/ieee/1588/6825/>
- Shapiro et al., *A comprehensive study of Convergent and Commutative Replicated Data Types*, INRIA RR-7506, 2011.
- Kleppmann, *Designing Data-Intensive Applications*, cap. 5 y 9 (replicación y consistencia).

---

### Nota final

El proyecto no necesita “replicar Jam”. Necesita una **cola compartida con reproducción coordinada**, que es un problema más pequeño y ya tiene resuelta la parte más difícil del código. La decisión que más impacta el esfuerzo no es el transporte ni el límite de 4 KB: es **aceptar que el host es la autoridad y que la cola es virtual**, en lugar de intentar que la cola pública de Spotify sea la fuente de verdad. Esa aceptación convierte un problema imposible en uno de 6–9 semanas.
