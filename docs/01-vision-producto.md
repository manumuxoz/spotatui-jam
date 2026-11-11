# 01 — Visión de producto

## 1. Problema

Escuchar música en grupo desde la terminal tiene dos partes:

1. **Sincronía:** que todos oigan el mismo track en la misma posición.
2. **Cola compartida:** que cualquiera pueda proponer qué suena después, sin pelearse con el portátil del dueño de la cuenta.

spotatui 0.48 resuelve la primera con **Listening Party**: host e invitados se conectan a un relay WebSocket, el host publica track/posición/estado cada 2 segundos y los invitados corrigen el drift con un seek. También existe un modo de control compartido para play/pause/skip.

Lo que no existe es la segunda. La cola del host sigue siendo privada: los invitados no pueden verla, añadir tracks ni modificar el orden. En la práctica, una jam termina siendo “pásame el portátil” o “dime qué canción pongo”.

## 2. Propuesta de valor

> **Listening Party sincroniza el oído. spotatui-jam sincroniza la decisión.**

Una extensión de spotatui que añade una cola colaborativa en tiempo real a Listening Party, con el host como árbitro y la terminal como interfaz. Sin apps web, sin cuentas extra, sin depender de la Jam cerrada de Spotify.

### Diferenciadores

- **Nativa en la TUI:** misma sesión, mismos keybindings, misma estética. No es otra pestaña del navegador.
- **Self-hostable:** el relay se puede ejecutar en Cloudflare Workers o en una LAN con el relay Rust.
- **Reglas de sala:** límites, moderación, aprobación y atribución; no es un caos de “cualquiera pone cualquier cosa”.
- **Upstream-friendly:** el objetivo es que la función viva en spotatui, no en un fork eterno.

## 3. Usuarios y trabajos por hacer

### Persona A — Host técnico

Tiene spotatui y Spotify Premium. Organiza la escucha en casa, en la oficina o en un servidor de Discord. Quiere:

- Crear una sala en dos teclas y compartir un código.
- Ver quién ha añadido cada track y quién está conectado.
- Poner límites para que un invitado no copie 40 canciones.
- Poder quitar, reordenar y saltar sin cambiar de aplicación.

### Persona B — Invitado con terminal

Tiene spotatui y Premium. Quiere:

- Unirse con un código, sin configurar un cliente de Spotify.
- Ver qué está sonando y qué viene después.
- Buscar y añadir tracks desde su propia cuenta.
- Votar o reordenar si el host lo permite.
- Recibir feedback inmediato: “añadido”, “cola llena”, “sin permiso”.

### Persona C — Invitado sin terminal (P1)

No tiene spotatui instalado. Quiere un enlace web mínimo para ver la cola y añadir tracks. **No está en la v1**; requiere un segundo frontend y un modelo de autenticación de invitado. Se planifica como extensión futura.

## 4. Flujos principales

### 4.1 Crear una jam

```
Ctrl+P → "1" (crear)
  → relay crea sala, devuelve código ABC123
  → host elige política de cola:
       - Solo host añade
       - Invitados añaden (por defecto)
       - Invitados gestionan (añadir/quitar/reordenar)
  → pantalla Party con:
       cabecera: ABC123 · 3 oyentes · cola 12/100
       now playing
       cola con [posición] título · artista · añadido por · votos
       footer de teclas
```

### 4.2 Unirse

```
Ctrl+P → "2" → ABC123 → nombre → Enter
  → recibe snapshot de playback + snapshot de cola
  → su reproductor salta al track del host y ajusta posición
  → ve la cola y el estado de la sala
```

### 4.3 Añadir un track

```
Invitado: "a" → overlay de búsqueda → escribe "burial archangel"
  → resultados con título/artista/duración
  → Enter → envía QueueAdd{uri, metadata, op_id}
  → host valida: URI Spotify, cola no llena, rate limit, duplicado
  → host aplica, incrementa revision, difunde QueuePatch
  → todos ven el track al final; el invitado recibe ack
```

### 4.4 El host salta o reproduce un item

```
Host selecciona item → Enter ("reproducir ahora")
  → el track se mueve al head de la cola nativa y suena
  → SyncState inmediato a todos
  → invitados hacen play_uri(track) + seek
```

### 4.5 La cola se vacía

Política configurable:

- **Continuar contexto** (por defecto): al terminar la cola, el host retoma la playlist/álbum que estaba sonando.
- **Parar:** pausa al terminar el último track.
- **Loop de cola:** repite la cola desde el principio.

La v1 implementa “continuar contexto” y “parar”. Loop queda como P1.

## 5. Historias de usuario (MVP)

| ID | Como | Quiero | Para | Prioridad |
|---|---|---|---|---|
| US-01 | host | crear una sala y ver un código | invitados a unirse | Must |
| US-02 | invitado | unirme con código y nombre | escuchar sincronizado | Must |
| US-03 | invitado | ver la cola y el now playing | saber qué viene | Must |
| US-04 | invitado | buscar y añadir un track | participar | Must |
| US-05 | host | ver quién añadió cada track | moderar | Must |
| US-06 | host | quitar un track de la cola | moderar | Must |
| US-07 | host | reordenar la cola | controlar el flow | Must |
| US-08 | host | limitar la cola (tamaño y por invitado) | evitar abusos | Must |
| US-09 | host | elegir si invitados pueden añadir/gestionar | adaptar la sala | Must |
| US-10 | invitado | recibir ack/error de mi acción | saber si funcionó | Must |
| US-11 | invitado | reconectar y recuperar la cola | tolerar cortes | Should |
| US-12 | invitado | votar tracks | influir en el orden | Could |
| US-13 | host | aprobar tracks antes de que entren | moderar | Could |
| US-14 | host | exportar la cola a playlist | recordar la sesión | Could |
| US-15 | invitado sin terminal | unirse por web | participar sin instalar | Won't (v1) |

## 6. UX de la pantalla Party

### Layout (80×24 mínimo)

```
┌ Party · ABC123 ─────────────── 3 oyentes · cola 12/100 ┐
│ ▶ Burial — Archangel          2:14 / 4:59   host: manu  │
│ ─────────────────────────────────────────────────────── │
│ #  Título                      Artista        Añadido   │
│ 1  Untrue                      Burial         ana       │
│ 2  Halcyon                     Jon Hopkins    host      │
│ 3  Xtal                        Aphex Twin     guest-2   │
│ ...                                                     │
│ ─────────────────────────────────────────────────────── │
│ a añadir · x quitar · J/K mover · Enter poner ahora     │
│ v voto · c control · m miembros · l salir · ? ayuda     │
└─────────────────────────────────────────────────────────┘
```

- La cola muestra solo tracks de Spotify en la v1.
- El item seleccionado se resalta; `Enter` lo pone ahora solo si el usuario tiene permiso de control.
- Los errores se muestran como status message, no como popup modal.
- La pantalla funciona a 80 columnas; a anchos menores se oculta la columna “Añadido”.
- No se usa color como único indicador: hay iconos y texto.

### Teclas propuestas

| Tecla | Acción | Permiso |
|---|---|---|
| `a` | abrir búsqueda para añadir | add_queue |
| `x` | quitar item seleccionado | host o dueño |
| `J` / `K` | bajar/subir item | manage_queue |
| `Enter` | reproducir ahora | control_playback |
| `v` | ciclar voto (-1/0/+1) | vote (P1) |
| `c` | alternar control de transporte | host |
| `m` | ver miembros | todos |
| `Tab` | cambiar foco cola/miembros | todos |
| `l` | salir/terminar | todos |
| `Esc` | cerrar pantalla sin salir | todos |

Las teclas se registran en el keymap existente (`keys.party_*`) para que sean reconfigurables y no colisionen con las globales.

### Estados de la pantalla

- **Desconectado:** crear/unirse, código y nombre.
- **Conectando:** spinner y “conectando al relay…”.
- **Hosting:** cola editable, código visible, miembros.
- **Joined:** cola en solo lectura o editable según permisos.
- **Reconectando:** banner, acciones deshabilitadas, último snapshot visible.
- **Cerrada:** “el host cerró la sala”, opción de volver a unirse.
- **Error:** mensaje accionable (relay caído, código inválido, sala llena, versión incompatible).

## 7. Reglas de sala

### Permisos

Tres niveles, configurables por el host en caliente:

1. **Host only:** solo el host añade y gestiona. Los invitados solo escuchan.
2. **Guests add:** los invitados añaden; el host quita/reordena. **Default.**
3. **Guests manage:** los invitados añaden, quitan sus propios items y, si se habilita, reordenan.

Hay tres ejes independientes; no confundirlos:

| Eje | Valores | Qué controla |
|---|---|---|
| `control_mode` | `host_only` / `shared_control` | Quién puede play/pause/skip (transporte) |
| `queue_mode` | `host_only` / `guests_add` / `guests_manage` | Quién puede añadir, quitar o mover en la cola |
| `playback_mode` | `follow_host` / `host_only` | Quién reproduce el audio (todos o solo el host) |

Se puede permitir añadir a la cola sin permitir saltar, o permitir saltar sin permitir reordenar.

### Límites (defaults)

| Límite | Default | Configurable |
|---|---|---|
| Participantes | 8 | relay |
| Tracks en cola | 100 | host |
| Adds pendientes por invitado | 5 | host |
| Rate limit de adds | 1 cada 2 s | host |
| Longitud de título/artista | 200/200 chars | host |
| Duración máxima de track | 60 min | host |
| Duplicados | rechazar mismo URI | host |
| Explícitos | permitidos | host |

### Moderación

- El host puede quitar cualquier item y vaciar la cola (con confirmación).
- El host puede expulsar a un invitado (P1); en v1 basta con cerrar la sala o pedirle que salga.
- Los nombres se normalizan y se limitan a 32 caracteres; no pueden suplantar al host (ya lo hace el relay).
- Los metadatos enviados por invitados se tratan como no confiables: se muestran como texto plano y se truncan; el host puede resolverlos contra su propia caché.

## 8. Alcance

### MVP (v0.1)

- Protocolo v2 con cola, snapshot troceado, deltas, revisiones y acks.
- Host autoritativo sobre `native_queue` (solo Spotify).
- Invitados: ver cola, buscar, añadir, quitar sus items.
- Host: quitar, reordenar, vaciar, poner ahora, políticas y límites.
- Pantalla Party completa en TUI.
- Paridad de la página Party del desktop/GUI queda en P1; el protocolo es compartido y la TUI es la prioridad.
- Relay Worker actualizado + relay Rust mínimo para LAN (puede ser P1 si el Worker basta).
- Compatibilidad hacia atrás con clientes 0.48.
- Tests unitarios, de propiedad, integración con relay local y snapshots TUI.
- Documentación de usuario y de auto-hosting.

### P1

- Votación y orden por score.
- Aprobación previa de tracks.
- Exportar la cola a playlist de Spotify.
- Invitado web de solo lectura + añadir (sin reproducción).
- Relay Rust completo con TLS y Docker.
- Reconexión del host con gracia de 60 s.
- Chat de sala (quizá; no es el foco).

### P2 / exploratorio

- Integración con Jam nativa vía `spotify_cli` (macOS, no documentada, flag experimental).
- Invitados sin cuenta Spotify en modo jukebox (el host reproduce; los invitados solo controlan).
- Sync de audio de mayor precisión (medición de offset, no solo seek).
- Soporte de fuentes no Spotify en la cola (requiere que todos tengan la misma fuente).
- Aplicación web de host.

### No-goals

- Retransmitir audio de Spotify a otros.
- Interoperar con la Jam oficial sin API.
- Sustituir Spotify como servicio.
- Monetización o servicio público multi-tenant.

## 9. Métricas de éxito

### Producto

- Una sala de 2+ personas completa 30 minutos sin que el host tenga que reiniciar.
- ≥ 80 % de los invitados añaden al menos un track en sesiones de 30 min.
- Tiempo desde “Enter” hasta que el track aparece en todas las colas: p95 < 500 ms en LAN, < 1 s por internet.
- Cero pérdidas de items de cola en una sesión sin caída del host.
- Encuesta informal: “¿Lo usarías otra vez?” ≥ 4/5.

### Técnicas

- Drift de reproducción p95 < 2 s (el modelo actual ya tolera 3 s; mejorar a 2 s es suficiente para una fiesta, no para karaoke).
- Propagación de comandos p95 < 300 ms (relay propio, misma región).
- 8 invitados + 100 items: CPU del host < 5 % en un portátil moderno.
- 0 hallazgos altos en `cargo audit`/`cargo deny` en release.
- Cobertura del crate de cola > 80 %; protocolo con fuzz continuo.
- Compatibilidad verificada old-client/new-relay y new-client/old-host.

## 10. Decisiones abiertas (a confirmar antes de Fase 1)

| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| D1 | ¿Fork o plugin? | Fork core / plugin Lua + sidecar | **Fork core**; plugin no tiene WebSocket ni control total de cola |
| D2 | ¿Invitados con cuenta? | Todos Premium / host-only playback | **Todos Premium** en v1; jukebox en P2 |
| D3 | ¿Relay? | Cloudflare Worker / Rust self-host | **Worker ahora, Rust en P1**; protocolo común |
| D4 | ¿Votación? | Sí / no | **P1**, no bloquea el MVP |
| D5 | ¿Nombre visible? | “Jam” / “Party Queue” | **“Party Queue”** en UI para no implicar la Jam oficial; “spotatui-jam” como nombre de proyecto |
| D6 | ¿Upstream? | PR / fork privado | **PR upstream-first**; fork como fallback |

## 11. Narrativa de demo (para validar con usuarios)

> “Abre spotatui, pulsa Ctrl+P y crea una sala. Comparte el código. Tus amigos entran desde su terminal, ven tu cola y añaden canciones. Tú decides quién puede saltar. Cuando termináis, la cola se puede guardar como playlist.”

La demo debe funcionar en una LAN sin internet salvo Spotify, con el relay Rust local. Ese es el escenario de aceptación emocional del producto.
