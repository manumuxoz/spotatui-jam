# Contributing

## Flujo de trabajo

El código de spotatui no se versiona completo en este repositorio: se publica como
un parche reproducible contra el tag `v0.48.0`. El flujo es:

1. `./scripts/bootstrap.sh` clona spotatui y aplica el parche.
2. Trabaja en `vendor/spotatui` (o en el worktree de desarrollo).
3. Ejecuta las pruebas de [`docs/11-reproducibilidad.md`](docs/11-reproducibilidad.md).
4. Exporta el parche actualizado:

   ```bash
   cd vendor/spotatui
   git diff v0.48.0 > ../../patches/0001-party-queue.patch
   ```

   En un fork con commits, usa `git format-patch v0.48.0..HEAD --stdout`.

5. Actualiza `docs/10-implementation-status.md` y la documentación afectada.

## Reglas

- La cola es **YouTube-only** en v1 (`youtube:<id>`); no añadas `spotify:track:` a la cola.
- El host es el único escritor de la cola; los invitados envían intents.
- Cada mensaje debe caber en 4 KB; usa `queue_snapshot_messages()` para trocear.
- No metas tokens, credenciales ni datos personales en el relay.
- No uses APIs no documentadas de Spotify para la cola.
- Añade tests unitarios y, si toca red, el test de integración con relay.
- Mantén la compatibilidad con clientes v1 y el transporte Spotify legado.

## Pruebas mínimas

```bash
cd party-relay && cargo test
cd ../work/spotatui
cargo test --locked --no-default-features --features telemetry,tui,streaming,youtube --lib
cargo clippy --locked --no-default-features --features telemetry,tui,streaming,youtube --all-targets -- -D warnings
```

Para los tests de integración hace falta un relay:

```bash
cd party-relay && cargo run --release &
cd ../work/spotatui
PARTY_RELAY_URL=ws://127.0.0.1:8080/ws \
  cargo test --locked --no-default-features --features telemetry,tui,streaming,youtube \
  two_apps_exchange -- --ignored
```

## Upstream

El objetivo es que la feature viva en spotatui. No abras un PR sin revisar antes el diff
completo con el operador. Conserva la licencia MIT y los avisos de copyright.
