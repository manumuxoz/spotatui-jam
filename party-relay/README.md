# party-relay

Relay WebSocket auto-hospedable para Party Queue v2. Enruta mensajes JSON
entre un host y hasta 7 invitados por sala, sin persistencia y sin interpretar
la cola: el host es la autoridad.

- Endpoint: `GET /ws` (WebSocket) y `GET /healthz`.
- Salas: código de 6 caracteres `A-Z0-9`, un host, máximo 8 participantes
  (host incluido), cierre al irse el host y expiración por inactividad.
- Mensajes: texto JSON de hasta 4096 bytes; los mensajes mayores se descartan
  con `{"type":"error", ...}` sin cerrar la conexión.
- Sin base de datos: reiniciar el relay cierra todas las salas.

## Arranque

Requiere Rust 1.85 o superior.

```bash
cargo run --release
# escucha en 0.0.0.0:8080
```

Ejemplo con configuración explícita:

```bash
BIND_ADDR=127.0.0.1:8080 \
MAX_PARTICIPANTS=8 \
MAX_MESSAGE_BYTES=4096 \
ROOM_TTL_SECS=300 \
QUEUE_ENABLED=1 \
RUST_LOG=info \
cargo run --release
```

## Variables de entorno

| Variable | Default | Descripción |
|---|---|---|
| `BIND_ADDR` | `0.0.0.0:8080` | Dirección de escucha. |
| `MAX_PARTICIPANTS` | `8` | Participantes por sala, host incluido. |
| `MAX_MESSAGE_BYTES` | `4096` | Límite por mensaje entrante. |
| `ROOM_TTL_SECS` | `300` | Inactividad tras la que se cierra la sala. |
| `QUEUE_ENABLED` | `1` | `0` desactiva los mensajes `queue_*` y anuncia `features: []`. |
| `RUST_LOG` | `info` | Filtro de `tracing` (`debug`, `info`, `warn`, ...). |

Los valores numéricos inválidos hacen que el proceso termine con error; no se
sustituyen silenciosamente.

## Cliente

Configura el relay en Spotatui:

```yaml
behavior:
  relay_server_url: "ws://127.0.0.1:8080/ws"
```

El relay acepta las query params del protocolo v2:

```text
/ws?action=create&v=2&features=queue&name=manu
/ws?action=join&code=ABC123&v=2&features=queue&name=ana
```

`control_mode=host_only|shared_control` se lee al crear la sala. Los clientes
v1 pueden seguir usando el relay: los campos v2 adicionales se ignoran. Para
los mensajes dirigidos, `to` se resuelve primero por id de participante y
después por nombre, de modo que clientes que usan `from` como destino siguen
funcionando.

## Comprobación

```bash
curl http://127.0.0.1:8080/healthz
# {"ok":true,"rooms":0}

cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Los tests de integración usan `tokio-tungstenite` y cubren handshake,
`guest_joined` con id, enrutado guest→host con `from`/`from_id`, dirección
host-only, mensajes dirigidos con `to`, límite de tamaño, rate limit, cierre
al irse el host, límite de participantes, expiración y `/healthz`.

## Docker (opcional)

```bash
docker build -t party-relay .
docker run --rm -p 8080:8080 -e RUST_LOG=info party-relay
```

## TLS

El relay no termina TLS. En producción, ponlo detrás de Caddy o Nginx y usa
`wss://party.example.com/ws` como `relay_server_url`; el proxy debe reenviar
los headers `Upgrade` y `Connection`. No hay autenticación de operador:
protege el puerto con firewall o el reverse proxy.
