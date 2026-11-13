# 04 — Pipeline de desarrollo y CI/CD

Este documento define **cómo se trabaja** (de la idea al release) y **qué se ejecuta automáticamente** en cada etapa. El objetivo es que ningún cambio de protocolo, cola o relay llegue a `main` sin pruebas de convergencia y compatibilidad.

Los workflows de referencia están en [`pipeline/ci.yml`](../pipeline/ci.yml), [`pipeline/nightly.yml`](../pipeline/nightly.yml) y [`pipeline/release.yml`](../pipeline/release.yml). Son plantillas para copiar a `.github/workflows/` en el fork; no se ejecutan desde este directorio.

---

## 1. Flujo de trabajo

```
Issue template
   │
   ▼
Backlog (docs/08-backlog.md) ──► rama corta ──► PR pequeño
                                                   │
                                                   ├─ CI rápido (fmt, clippy, test)
                                                   ├─ CI protocolo (golden, property, fuzz smoke)
                                                   ├─ CI relay (worker + integración)
                                                   ├─ CI UI (snapshots)
                                                   └─ review (1 aprobación)
                                                          │
                                                          ▼
                                                        main
                                                          │
                                                          ├─ nightly (fuzz, soak, load, upstream)
                                                          └─ tag ──► release + relay staging ──► prod
```

### Reglas de rama

- `main` siempre verde y desplegable.
- Ramas: `feat/party-queue-<tema>`, `fix/party-<tema>`, `relay/<tema>`, `docs/<tema>`.
- Vida máxima de una rama: 5 días laborables. Si no cabe, partir la historia.
- Un PR toca una sola capa: protocolo, host, relay, UI o docs. Excepción: un vertical slice pequeño puede tocar protocolo + host + test.
- Los cambios de protocolo requieren un ADR o una entrada en `docs/protocolo-party-v2.md` y revisión de compatibilidad.

### Commits

Conventional Commits, con scope:

```
feat(party): add host-authoritative queue state
feat(relay): forward queue commands and assign participant ids
fix(party): resync mirror after a revision gap
test(party): property-test host and mirror convergence
docs(party): document queue permissions and limits
```

### Issue templates

- **Bug:** pasos, versión, relay, rol (host/guest), logs sin tokens, captura.
- **Feature:** problema, propuesta, alternativas, impacto en protocolo.
- **Protocol change:** mensajes afectados, compatibilidad old/new, migración, tests.

### Review checklist

- [ ] ¿Respeta los invariantes de `docs/02-arquitectura.md`?
- [ ] ¿Un cliente viejo sigue funcionando?
- [ ] ¿Los mensajes nuevos caben en 4 KB?
- [ ] ¿Hay validación de input no confiable?
- [ ] ¿Los tests cubren el camino feliz y los rechazos?
- [ ] ¿La UI tiene snapshot y estados de error?
- [ ] ¿La documentación y el CHANGELOG están actualizados?

---

## 2. Pipeline local (pre-push)

Ideal: un `justfile` con los mismos comandos que CI.

```bash
# Formato y lints
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features telemetry,tui -- -D warnings

# Tests rápidos
cargo test --locked
cargo test --locked --no-default-features --features telemetry,tui,streaming

# Solo la cola (rápido, sin audio)
cargo test --locked party::

# Relay local (Rust, sin wrangler)
cd party-relay && cargo test && cargo run --release

# Relay local (Worker)
cd worker-relay && npm ci && npm test && npx wrangler dev --local

# Integración real host + guest contra el relay
PARTY_RELAY_URL=ws://127.0.0.1:8080/ws \
  cargo test --locked --no-default-features --features telemetry,tui,streaming \
  -- --ignored

# UI snapshots
cargo test --locked --features tui party_ui
```

`just` propuesto:

```makefile
fmt:
	cargo fmt --all -- --check

lint:
	cargo clippy --locked --all-targets -- -D warnings

test:
	cargo test --locked

test-party:
	cargo test --locked party::

relay-dev:
	cd worker-relay && npx wrangler dev --local

integration:
	PARTY_RELAY_URL=ws://127.0.0.1:8080/ws cargo test --locked -- --ignored

ci: fmt lint test test-party
```

No se instala `just` como dependencia obligatoria; es azúcar. Los comandos `cargo` son la fuente de verdad.

---

## 3. CI de pull request

### 3.1 Jobs y matriz

| Job | Qué hace | Bloqueante |
|---|---|---|
| `prepare` | Valida el evento, calcula skip `[skip ci]` | Sí |
| `fmt` | `cargo fmt --all -- --check` | Sí |
| `clippy` | Matriz de features (default, slim, all-sources, headless) con `-D warnings` | Sí |
| `test` | Matriz de features; incluye tests de party | Sí |
| `party-protocol` | Golden JSON, property tests, chunk size | Sí |
| `party-integration` | Host + guest reales contra el relay, add/ack/patch | Sí |
| `compat` | Matriz old/new de cliente y relay | Sí |
| `tui-snapshots` | `insta` snapshots a 80×24/120×40/200×60 | Sí |
| `relay` | `npm ci`, typecheck, tests, `wrangler deploy --dry-run` | Sí |
| `relay-rust` | `fmt`, `clippy`, tests del crate `party-relay`, smoke del protocolo | Sí |
| `security` | `cargo audit`, `cargo deny`, `cargo vet` (si se adopta) | Sí |
| `docs` | Link check, spellcheck, TOC | No bloqueante (warning) |
| `coverage` | `cargo llvm-cov`, subida a Codecov | No bloqueante |
| `msrv` | `cargo check` con Rust 1.90 | Sí |
| `macos` | `check + clippy` en macOS | Sí |
| `windows` | `check` en Windows | Sí (informativo al inicio) |

La matriz de features debe incluir la combinación exacta del release de Linux, como ya hace spotatui en `ci.yml`. No crear una lista paralela que se desincronice.

### 3.2 Tests específicos de la cola

| Test | Qué valida |
|---|---|
| `queue_invariants` | IDs únicos, límites, revisión monotónica |
| `apply_action_matrix` | Permisos por rol y modo; rechazos con error correcto |
| `mirror_convergence` | 10k secuencias aleatorias host↔mirror |
| `snapshot_chunk_size` | Peor caso < 3.800 bytes |
| `patch_gap_resync` | Gap → resync → convergencia |
| `op_id_idempotency` | Reintento no duplica |
| `consume_head` | `AdvanceNativeQueue` → item fuera de la cola |
| `old_client_ignores_queue` | Un parser v1 no se rompe con mensajes v2 |
| `new_client_old_host` | La UI muestra “sin cola” y el playback sigue |
| `malicious_metadata` | ANSI/control chars se limpian |
| `oversized_message` | Relay y host rechazan > 4 KB |
| `command_flood` | Rate limit devuelve error, no panic |

### 3.3 Fuzz

Targets:

1. `parse_sync_message` con bytes arbitrarios.
2. `QueueAction` deserialization con strings raros.
3. `PartyMirror::apply_patch` con ops aleatorias (property test, no fuzz de bytes).
4. Relay: JSON pass-through con tamaños límite.

En PR: 60 s por target. En nightly: 1 h por target, corpus persistido.

### 3.4 Compatibilidad

Matriz mínima:

| Host | Relay | Invitado | Esperado |
|---|---|---|---|
| 0.48 | v1 | 0.48 | Listening Party clásico |
| 0.48 | v2 | 0.48 | Clásico; `queue_*` ignorados |
| nuevo | v1 | nuevo | Sin cola; playback OK |
| nuevo | v2 | 0.48 | Host ve invitado sin cola; playback OK |
| 0.48 | v2 | nuevo | Invitado ve host sin cola |
| nuevo | v2 | nuevo | Jam completa |

Los binarios 0.48 se pueden obtener del release de GitHub en CI; si no, se simulan con fixtures de su protocolo. La matriz con binarios reales es un job nocturno, no de PR (descarga y tamaño).

---

## 4. Nightly

| Job | Frecuencia | Qué hace | Alerta |
|---|---|---|---|
| `fuzz` | P1 | 1 h por target, corpus en caché | Issue automático |
| `soak` | diario | 1 h, host + 8 invitados simulados, red con 50–300 ms de latencia | Issue |
| `load-relay` | semanal | 100 salas × 8 clientes, 10 min | Issue |
| `upstream-sync` | diario | Fetch de `LargeModGames/spotatui`, intento de rebase, conflicto → issue | Issue |
| `compat-binaries` | semanal | Descarga 0.48 y prueba matriz real | Issue |
| `audit` | diario | `cargo audit`, `npm audit` | Issue |
| `flaky` | diario | Repite tests de integración 20× | Issue |

El soak debe medir:

- divergencia de cola (host vs 8 espejos) al final;
- latencia p50/p95/p99 de add→patch;
- memoria RSS del host y del relay;
- reconexiones y resyncs;
- errores de parseo.

---

## 5. Release

### 5.1 Versionado

- Fork: `0.49.0-jam.1`, `0.49.0-jam.2`, … (prerelease semver).
- Si se upstream: la versión la decide spotatui; el feature se documenta en el CHANGELOG.
- Protocolo: `protocol_version: 2`; no se cambia dentro de una release menor sin compatibilidad.
- Relay: versión independiente (`relay-v1`, `relay-v2`), desplegable por separado.

### 5.2 Artefactos

| Artefacto | Plataformas | Firma |
|---|---|---|
| `spotatui` TUI | Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64 | `cosign`/minisign + SHA256 |
| Paquetes | `.deb`, `.rpm`, `.tar.gz`, `.zip` | SHA256 |
| Relay Worker | Cloudflare | `wrangler` version |
| Relay Rust (P1) | Linux x86_64/aarch64, Docker | Imagen `ghcr.io/...` + SBOM |
| Docs | Markdown/HTML | — |

Reutilizar el workflow `cd.yml` de spotatui para no duplicar la matriz de builds. Si el fork añade un crate/feature, añadirlo a la matriz existente.

### 5.3 Secuencia de release

```
1. Freeze: rama release/x.y.z, congelar features
2. Bump versión + CHANGELOG (release-plz o manual)
3. CI completo + soak de 1 h + security
4. Tag firmado
5. Build matrix + SBOM + firmas
6. Relay staging (worker nuevo, URL staging)
7. Smoke test automatizado contra staging:
     - crear sala
     - unirse con 2 clientes
     - añadir 3 tracks
     - quitar 1
     - mover 1
     - verificar snapshot/ack/patch
8. Promover relay a producción (wrangler deploy)
9. Publicar GitHub Release + artefactos
10. Smoke test post-release con binario descargado
11. Anuncio + docs
```

### 5.4 Rollback

- **Cliente:** las releases son aditivas; un cliente viejo sigue funcionando contra relay v2. Si una release rompe, se marca como “deprecated” y se publica un fix; no se borra.
- **Relay:** `wrangler rollback` o redeploy de la versión anterior (Durable Objects no persistentes; las salas activas se pierden). Mantener dos versiones desplegadas (v1 y v2) durante la transición.
- **Feature flag:** el relay puede deshabilitar `queue_*` con una variable de entorno `QUEUE_ENABLED=0`, degradando a Listening Party clásico sin rollback de código.
- **Kill switch:** el cliente puede ignorar la cola si `behavior.party_queue_enabled: false`.

### 5.5 Secretos y permisos

| Secreto | Uso | Ámbito |
|---|---|---|
| `CLOUDFLARE_API_TOKEN` | Deploy del Worker | Environment `relay-prod` con aprobación manual |
| `CLOUDFLARE_ACCOUNT_ID` | Deploy | Igual |
| `CODECOV_TOKEN` | Coverage | Repo |
| `GITHUB_TOKEN` | Release | Permisos mínimos: `contents: write` solo en release |

Reglas:

- Actions fijadas por SHA, no por tag.
- `permissions:` mínimo en cada workflow.
- No ejecutar workflows con secretos en PRs de forks.
- Environment `relay-prod` con required reviewers.
- SBOM y `cargo deny` antes de publicar.

---

## 6. Observabilidad

### 6.1 Cliente

- Logs con `tracing`/`log`: sala, rol, revisión, op_id, resultado; **nunca** tokens ni URIs completas en nivel info (las URIs no son secretas, pero pueden ser privadas).
- Métricas locales opcionales: drift, latencia de ack, resyncs.
- `--debug` ya existe; añadir un `party-debug` que no loguee nombres.

### 6.2 Relay

- Métricas: salas activas, conexiones, mensajes/s, bytes/s, rechazos por rate limit, errores de parseo, duración de sala.
- Logs estructurados sin nombres ni URIs; solo códigos de sala hasheados.
- Alertas: tasa de error > 1 %, latencia p95 > 200 ms, CPU > 80 %, sala atascada.
- Dashboard mínimo (Grafana/Cloudflare Analytics) para el operador.

### 6.3 CI

- Duración de jobs y tasa de flakiness.
- Cobertura del crate de party.
- Fuzz corpus y crashes.
- Upstream sync: días desde el último rebase.

---

## 7. Documentación

- `docs/listening-party.md` de spotatui se actualiza con la cola.
- Nueva guía `docs/party-queue.md`: permisos, límites, teclas, troubleshooting.
- Nueva guía `docs/self-hosting-relay.md`: Worker y Rust, TLS, variables, backup.
- `docs/protocolo-party-v2.md`: spec para implementadores.
- CHANGELOG por release.
- Cada PR user-facing actualiza docs; el job `docs` verifica links y TOC.

---

## 8. Pipeline de contribución upstream

```
1. Fork + branch desde main
2. PR draft con design doc (link a docs/02-arquitectura.md)
3. Feedback temprano del maintainer
4. Commits atómicos:
     a. protocol types + tests
     b. host queue + tests
     c. relay v2 + tests
     d. guest mirror + UI
     e. docs
5. CI verde + soak
6. Review
7. Merge upstream o, si se rechaza, release del fork
```

Regla: **no mezclar** el fork y el upstream en la misma rama. El fork puede tener commits de packaging; el PR upstream debe contener solo la feature.

Si el upstream pide un diseño distinto (por ejemplo, cola en el relay en vez de en el host), se evalúa en un ADR; no se rebasa el diseño en silencio.

---

## 9. Matriz de gates

| Gate | Criterio | Bloquea |
|---|---|---|
| G0 | Spikes OK, ToS revisada, maintainer contactado | Escribir código de feature |
| G1 | Protocolo + property tests + fuzz smoke | Integración host |
| G2 | Host + relay + sim 8 invitados | UI invitado |
| G3 | Jam manual 30 min con 2–3 personas | Release candidate |
| G4 | Soak + security + docs + rollback | Release pública |

Ningún gate se salta por presión de calendario. Si un gate falla, se recorta alcance (P1) antes que calidad.

---

## 10. Plantilla de PR

```markdown
## Qué

## Por qué

## Cómo probarlo
1.
2.

## Protocolo
- [ ] Sin cambios
- [ ] Cambio aditivo (v2)
- [ ] Cambio incompatible (requiere ADR)

## Compatibilidad
- [ ] Cliente viejo + relay nuevo
- [ ] Cliente nuevo + relay viejo
- [ ] Host viejo + invitado nuevo
- [ ] Host nuevo + invitado viejo

## Riesgos / rollback

## Capturas (UI)
```

---

## 11. Plantilla de release notes

```markdown
# spotatui-jam vX.Y.Z

## Destacado
- Cola colaborativa en Listening Party.

## Cambios
- ...

## Protocolo
- v2: queue_snapshot, queue_patch, queue_command, queue_ack, queue_resync.

## Compatibilidad
- Funciona con relay v1 en modo solo playback.
- Requiere relay v2 para la cola.

## Auto-hosting
- ...

## Known issues
- ...
```
