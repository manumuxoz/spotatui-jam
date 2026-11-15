# 06 — Registro de riesgos

> **Cambio de alcance (15 nov 2026):** la Party Queue implementada es **YouTube-only**.
> Los riesgos R1/R2 sobre la Jam/Developer Policy de Spotify siguen aplicando al
> transporte Spotify legado, pero no a la cola. El riesgo específico de YouTube es
> la fragilidad de `yt-dlp` y los cambios de YouTube, ya asumido por spotatui.

Escala: probabilidad (P) e impacto (I) de 1 a 5. Prioridad = P × I.

| ID | Riesgo | P | I | Prio | Mitigación | Disparador |
|---|---|---|---|---|---|---|
| R1 | Spotify no tiene API de Jam y no la tendrá; el producto no puede interoperar con la Jam oficial | 5 | 3 | 15 | Posicionar como “Party Queue” de spotatui, no como Jam oficial; capa de adaptador futura; no prometer interoperabilidad | Petición de usuario “unirse a Jam de Spotify” |
| R2 | Política de desarrollador: el sync multi-dispositivo puede leerse como réplica de Jam (III.11) o webcasting (III.4) | 3 | 5 | 15 | Modo host-only (jukebox) como opción limpia; uso personal/self-hosted; sin monetización; sin marcas de Spotify; Premium obligatorio; revisión legal antes de release pública | Publicación como servicio o mención de monetización |
| R3 | La sesión nativa de Spotify de spotatui (sin Web API) se rompe por cambios del servidor | 4 | 3 | 12 | Seguir upstream; no añadir endpoints nuevos; tests de humo con cuenta real; degradar a “solo playback” | Falla el login o la reproducción en una release |
| R4 | El upstream rechaza el PR o el fork se desincroniza | 3 | 3 | 9 | Upstream-first; commits pequeños; rebase nocturno; feature flag; plan B plugin/sidecar | Conflicto de rebase > 1 día |
| R5 | La cola nativa y `PartyQueue` divergen (doble fuente de verdad) | 3 | 4 | 12 | `native_queue` derivada de `PartyQueue`; hook de consumo; property tests; invariantes | Cualquier test de convergencia rojo |
| R6 | El relay de 4 KB obliga a chunking y añade complejidad | 4 | 3 | 12 | Chunks por presupuesto de 3.500 B; test de tamaño; resync; límite opcional en relay Rust | Un snapshot supera 3.800 bytes |
| R7 | El drift de playback es > 3 s o audible | 4 | 2 | 8 | Sync inmediato en track change; compensación de RTT; umbral 2 s; aceptar best-effort | Queja en prueba manual |
| R8 | El host se desconecta y la sala muere | 4 | 3 | 12 | Documentar; P1 reconexión con gracia; guardar cola local; export | Corte de red del host |
| R9 | Invitado malicioso inunda o corrompe la sala | 3 | 3 | 9 | Rate limit relay + host; validación; permisos; tamaño; sin URLs | Comando raro o flood en logs |
| R10 | El relay público se usa para spam/abuso | 3 | 4 | 12 | Rate limits por IP; máx. 8; sin persistencia; kill switch; self-host recomendado | Aumento de salas o reportes |
| R11 | La UI no cabe en 80 columnas o es ilegible | 3 | 2 | 6 | Snapshots 80×24; truncado; tests de accesibilidad | Snapshot roto |
| R12 | Se rompe la compatibilidad con clientes 0.48 | 2 | 4 | 8 | Mensajes aditivos; matriz old/new; relay v1 y v2 | Test de compatibilidad rojo |
| R13 | El proyecto se expande a votos, chat, web, Jam nativa y no se termina | 4 | 3 | 12 | MVP con no-goals; gates; P1 separado; recortar antes que incumplir | Nueva feature antes de G3 |
| R14 | Falta de pruebas reales con Spotify en CI | 5 | 2 | 10 | Simulador + checklist manual por release; cuentas de prueba | Release sin prueba manual |
| R15 | La licencia de un proyecto de referencia impide reutilizar código | 3 | 2 | 6 | Solo MIT/Apache para código; ideas y UX se estudian; no copiar sin licencia | Copia de código de projeto sin licencia |
| R16 | El nombre “Jam” o “Listening Party” confunde con Spotify | 3 | 3 | 9 | UI “Party Queue”; nombre de proyecto “spotatui-jam” solo interno; disclaimer; sin logo de Spotify | Publicación de release |
| R17 | El relay Rust no alcanza paridad con el Worker | 3 | 2 | 6 | Protocolo común; tests de conformidad; Worker como referencia | Divergencia en tests |
| R18 | El coste de Cloudflare o los límites de Durable Objects | 2 | 2 | 4 | Self-host; plan gratuito; límites conocidos; métricas | Factura o 429 |
| R19 | La búsqueda del invitado no devuelve metadata fiable | 3 | 2 | 6 | Mostrar lo que devuelve Spotify; host valida URI; placeholder si falta | Track sin título |
| R20 | Problemas de accesibilidad o de terminal (kitty, sixel) | 2 | 1 | 2 | Solo texto; sin dependencia de gráficos | Reporte |

---

## 1. Riesgos políticos y legales (detalle)

### R1 — No hay API de Jam

Hecho verificado a noviembre de 2026: la Web API no tiene endpoints de Jam, el Web Playback SDK tampoco, y Spotify declaró en 2025 que Jam “pertenece más a los playback SDKs”. La petición formal de la comunidad sigue sin respuesta.

**Implicación:** cualquier funcionalidad que prometa “crear/unirse a una Jam de Spotify” es inviable con APIs documentadas. La Jam de spotatui será una **sala propia**, no la de Spotify.

**Mitigación:**

- Nombrar la función “Party Queue” en la UI, no “Spotify Jam”.
- No aceptar enlaces `socialsession` ni prometer miembros de Jam.
- Si en el futuro Spotify publica API, añadir un adaptador; el modelo de cola ya estaría preparado.

### R2 — Developer Policy

La lectura más peligrosa es III.11 (“no replicar una experiencia core”) y, en segundo lugar, III.4 (“no webcasting no interactivo”). III.6 (sincronizar con vídeo) no aplica; III.7 (mezclar) no aplica si no hay crossfade.

**Postura del proyecto:**

- Es una herramienta personal/self-hosted, sin ánimo de lucro ni servicio público.
- Cada usuario reproduce con su propia cuenta Premium; no se retransmite audio.
- No se usan marcas, logos ni nombres que empiecen por “Spot”.
- Se añade valor independiente: TUI, cola con permisos, self-hosting, integración multi-fuente.
- No se usan endpoints reverse-engineered ni `spotify_cli` en el núcleo.

**Decisión de producto:** ofrecer dos modos de playback:

1. **Follow-host (sync):** el modelo actual de Listening Party; todos Premium. Es el más parecido a Jam y el de mayor exposición por III.11. Etiquetarlo como “sala privada” y no operarlo como servicio.
2. **Host-only (jukebox):** solo el host reproduce; los invitados solo ven/añaden a la cola. **No hay retransmisión de audio**: si están en la misma sala, oyen el altavoz del host; si están remotos, no oyen nada. Esto evita el supuesto de “single source to several simultaneous listeners” de III.4 y reduce la exposición. Puede llegar en P1 con un invitado web.

La investigación tiene una tensión honesta: el informe 02 considera host-only el modelo más limpio, mientras que el informe 03 señala que un jukebox con oyentes remotos podría leerse como webcasting. La diferencia se resuelve aclarando que **no se transmite audio**; aun así, ninguno de los dos modos está autorizado explícitamente por Spotify. Si el proyecto se publica ampliamente, el modo por defecto debería ser host-only y la revisión legal previa es obligatoria. El plan construye la cola de forma independiente del modo de playback para permitir este cambio sin reescribir.

### R16 — Nomenclatura

“Listening Party” ya es una función de Spotify (evento de artista), y “Jam” es una marca de producto. Evitar:

- “Spotify Jam” en la UI o el README.
- “Listening Party” como nombre del proyecto.
- Logos o capturas con marcas.

Usar “Party Queue”, “sala” y “cola compartida”. El nombre `spotatui-jam` queda como identificador de repositorio, no como marca de producto.

---

## 2. Riesgos técnicos (detalle)

### R5 — Doble fuente de verdad

El mayor riesgo de ingeniería. `native_queue` la usa el motor de playback y puede mutar por caminos que no pasan por la cola colaborativa (añadir local, limpiar, saltar, consumir). Si `PartyQueue` y `native_queue` se mantienen a mano, aparecen items fantasma o se pierden tracks.

**Mitigación:**

- `PartyQueue` es la autoridad; `native_queue` se reconstruye.
- Un único punto de mutación (`apply_party_op`) que actualiza ambos.
- Hook explícito en `AdvanceNativeQueue` para consumir el head.
- Property test que compara ambas tras secuencias aleatorias.
- Si el refactor se vuelve invasivo, feature flag `party_queue` y fallback a “solo añadir al final”.

### R6 — Límite de 4 KB

El relay actual rechaza mensajes > 4 KB. Un snapshot de 100 items con metadata puede superar 20 KB. Chunking es obligatorio, no opcional.

**Mitigación:**

- Chunks por presupuesto de 3.500 bytes; strings de 120 chars en wire; test de peor caso.
- Si un chunk no cabe, reducir a 4 items; el test lo detecta.
- El relay Rust puede aceptar 64 KB, pero el protocolo base no depende de ello.
- Medir el tamaño real con `serde_json::to_vec` en un test, no estimar.

### R7 — Drift

La sincronía actual es seek-based, cada 2 s, con umbral 3 s. No se puede garantizar sync de muestra sin control del reloj de audio de librespot. Para una fiesta es suficiente; para karaoke no.

**Mitigación:**

- No prometer “sample-accurate”.
- Medir y mostrar el drift en modo debug.
- Enviar sync inmediato en track change y al reanudar.
- Aceptar 2–3 s como objetivo; si el usuario quiere más, evaluar Web Audio/reloj externo (fuera de alcance).

### R8 — Host único

Si el host cierra el portátil, la sala muere y la cola se pierde. Es una limitación del relay actual (la sala se cierra cuando el host se va).

**Mitigación:**

- Aviso claro en la UI.
- Persistir la cola localmente (opcional) para poder reabrir.
- P1: gracia de 60 s y reconexión con token de host.
- P1: export a playlist para no perder la selección.

---

## 3. Riesgos de proyecto

### R4 — Upstream

spotatui es un proyecto activo (v0.48, noviembre 2026) con maintainer y CI estrictos. Un PR de cola toca protocolo, core y UI; es grande.

**Mitigación:**

- PR draft temprano con design doc.
- Commits atómicos y revisables.
- No mezclar refactors.
- Rebase nocturno y conflicto como issue.
- Si se rechaza, fork con releases; documentar el delta.

### R13 — Scope creep

Votos, chat, QR, web, Jam nativa, múltiples fuentes, playback sync fino. Todo es tentador y nada es necesario para el MVP.

**Mitigación:**

- `docs/01-vision-producto.md` tiene no-goals explícitos.
- Cada feature nueva entra por P1/P2, no por el MVP.
- Gate G3 exige 30 min de jam real; si no se ha probado, no se añade nada.

### R14 — Pruebas reales

CI no puede autenticar Spotify ni reproducir audio. Los tests simulados no detectan un bug de librespot o de la sesión nativa.

**Mitigación:**

- Checklist manual por release (15 casos).
- Cuentas de prueba dedicadas.
- Smoke test de login/playback en cada release.
- Presupuesto de 1 día por release para pruebas manuales.

---

## 4. Matriz de aceptación de riesgos

| Riesgo | ¿Aceptable para MVP? | Condición |
|---|---|---|
| R1 sin API Jam | Sí | No prometer interoperabilidad |
| R2 política | Sí, con límites | Uso personal, sin servicio público; host-only recomendado |
| R3 sesión nativa | Sí | Es un riesgo heredado de spotatui |
| R5 divergencia | No | Debe estar resuelto y testeado |
| R6 4 KB | No | Chunking probado |
| R7 drift | Sí | Best-effort documentado |
| R8 host único | Sí | Documentado; P1 |
| R9/R10 abuso | No | Rate limits y validación antes de release |
| R12 compatibilidad | No | Matriz old/new verde |
| R13 scope | No | Backlog congelado en G3 |

---

## 5. Plan de contingencia

Si en Fase 0 el hook de `native_queue` no es viable:

1. **Plan B:** cola virtual solo para display; el host añade al final con `add_to_queue` y no se permite reordenar. Pierde el 30 % del valor, pero es entregable en 3 semanas.
2. **Plan C:** plugin Lua + sidecar para una cola virtual sin tocar el core; la reproducción se controla con `play_uri`. Más frágil, pero demuestra el concepto.
3. **Plan D:** cancelar la integración y escribir un documento de “por qué no” con los hallazgos; el trabajo de investigación sigue siendo útil.

Si el upstream rechaza el PR:

1. Fork mantenido con rebase mensual.
2. Releases propias con el mismo pipeline.
3. Documentar el parche para que otros lo apliquen.

Si la política se vuelve un bloqueo:

1. Reorientar a host-only/jukebox.
2. Mantener la cola como feature local sin sync multi-dispositivo.
3. No publicar como servicio; solo código self-hosted.

---

## 6. Riesgos residuales

Incluso con todo mitigado:

- La Jam oficial puede cambiar y hacer que la nuestra parezca obsoleta.
- La sesión nativa de Spotify puede romperse sin aviso.
- El sync de audio seguirá siendo best-effort.
- El relay público puede sufrir abuso.
- El fork puede quedar desactualizado si el upstream cambia mucho.

Estos riesgos se aceptan y se documentan; no bloquean el MVP.
