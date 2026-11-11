# 09 — Operación

## 1. Topología

### 1.1 Relay público (Cloudflare)

```
spotatui host ──WSS──► party.example.com/ws ──► Worker ──► Durable Object (sala)
spotatui guest ─WSS──►                          │
                                                └─ RateLimiter (DO por IP)
```

- Un Durable Object por sala (`idFromName(room_code)`).
- No hay base de datos: el estado vive en memoria del DO y muere con la sala.
- El Worker solo enruta; no guarda la cola.
- El código está en `worker-relay/` de spotatui; el fork lo extiende.

### 1.2 Relay self-hosted (P1)

```
spotatui ──WSS──► Caddy/Nginx (TLS) ──► jam-relay (axum) ──► rooms in-memory
```

- Binario Rust o contenedor Docker.
- Sin persistencia; al reiniciar se pierden las salas.
- Ideal para LAN, eventos y usuarios que no quieren depender de Cloudflare.
- Mismo protocolo v2; tests de conformidad contra el Worker.

---

## 2. Configuración del cliente

```yaml
behavior:
  relay_server_url: "wss://party.example.com/ws"
  party_queue_enabled: true
  party_queue_mode: guests_add
  party_queue_max: 100
  party_queue_per_guest: 5
  party_queue_dedupe: true
  party_queue_allow_explicit: true
  party_queue_stop_when_empty: false
  party_sync_threshold_ms: 2000
```

- `relay_server_url`: URL del relay; debe ser `wss://` en producción.
- `party_queue_enabled`: kill switch local; si es `false`, se comporta como Listening Party clásico.
- `party_queue_mode`: `host_only`, `guests_add`, `guests_manage`.
- Los límites se negocian con el host; el invitado no puede subirlos.

---

## 3. Despliegue del Worker

### 3.1 Requisitos

- Cuenta de Cloudflare con Workers y Durable Objects.
- `wrangler` (versión fijada en `package-lock.json`).
- Token con permisos `Workers Scripts:Edit`, `Workers KV:Edit` (si se usa), `Durable Objects:Edit`.

### 3.2 Comandos

```bash
cd worker-relay
npm ci
npm test
npx wrangler deploy --env staging
# smoke test contra staging
npx wrangler deploy --env production
npx wrangler versions list
npx wrangler rollback <version-id>
```

### 3.3 `wrangler.toml` (referencia)

```toml
name = "spotatui-party"
main = "src/index.ts"
compatibility_date = "2026-11-01"

[[durable_objects.bindings]]
name = "PARTY_ROOM"
class_name = "PartyRoom"

[[durable_objects.bindings]]
name = "RATE_LIMITER"
class_name = "RateLimiter"

[[migrations]]
tag = "v2"
new_sqlite_classes = ["PartyRoom", "RateLimiter"]

[vars]
QUEUE_ENABLED = "1"
MAX_PARTICIPANTS = "8"
MAX_MESSAGE_BYTES = "4096"

[env.staging]
name = "spotatui-party-staging"

[env.production]
name = "spotatui-party"
```

No commitear secretos. El token va en GitHub Environment `relay-prod` con aprobación manual.

### 3.4 Smoke test

```bash
# Crear sala
websocat "wss://party-staging.example.com/ws?action=create&v=2&features=queue"
# Unir guest
websocat "wss://party-staging.example.com/ws?action=join&code=ABC123&name=test&v=2"
# Enviar queue_command y verificar queue_ack/patch
```

Automatizado en `pipeline/release.yml` con un script Node/TS que use `ws`.

---

## 4. Self-hosting del relay Rust (P1)

### 4.1 Docker

```bash
docker run -d \
  --name jam-relay \
  -p 8080:8080 \
  -e RUST_LOG=info \
  -e MAX_PARTICIPANTS=8 \
  -e MAX_MESSAGE_BYTES=4096 \
  ghcr.io/<owner>/spotatui-jam-relay:latest
```

### 4.2 TLS con Caddy

```caddyfile
party.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

WebSocket funciona sin configuración extra en Caddy/Nginx modernos. Si se usa Nginx:

```nginx
location /ws {
    proxy_pass http://127.0.0.1:8080;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 3600s;
}
```

### 4.3 Variables

| Variable | Default | Descripción |
|---|---|---|
| `BIND_ADDR` | `0.0.0.0:8080` | Escucha |
| `MAX_PARTICIPANTS` | `8` | Por sala |
| `MAX_MESSAGE_BYTES` | `4096` | Límite por mensaje |
| `QUEUE_ENABLED` | `1` | Kill switch |
| `RUST_LOG` | `info` | Nivel de log |
| `ROOM_TTL_SECS` | `300` | Inactividad |
| `TRUST_PROXY` | `0` | Usar `X-Forwarded-For` |

### 4.4 Limitaciones

- Sin persistencia: reiniciar cierra todas las salas.
- Sin autenticación de operador; proteger el puerto con firewall/reverse proxy.
- Sin métricas Prometheus en v1; logs estructurados.
- Escalado vertical; una instancia soporta decenas de salas.

---

## 5. Observabilidad

### 5.1 Métricas del relay

| Métrica | Tipo | Alerta |
|---|---|---|
| `rooms_active` | gauge | > 80 % de capacidad |
| `connections_active` | gauge | > 1.000 |
| `messages_total{type}` | counter | tasa anómala |
| `message_bytes_total` | counter | > 10 MB/s |
| `rate_limited_total` | counter | > 5 % de mensajes |
| `parse_errors_total` | counter | > 0.1 % |
| `room_duration_seconds` | histogram | p95 < 2 h |
| `room_closed_total{reason}` | counter | picos de `host_left` |
| `ws_errors_total` | counter | > 1 % |

### 5.2 Logs

```json
{"ts":"2026-11-11T20:00:00Z","level":"info","event":"room_created","room":"***","participants":1}
```

- No loguear nombres, URIs ni tokens.
- El código de sala se hashea o se omite.
- En self-host, el operador puede activar `RUST_LOG=debug` bajo su responsabilidad.

### 5.3 Alertas

- Relay caído (healthcheck `/healthz`).
- Tasa de error > 1 % durante 5 min.
- p95 de latencia > 200 ms.
- Abuso: `rate_limited_total` > 20 % durante 10 min.
- Sala atascada: sin heartbeat > 6 min (el relay ya la cierra a los 5).

### 5.4 Healthcheck

```
GET /healthz → 200 {"ok":true,"rooms":N}
```

No expone datos de salas.

---

## 6. Runbook de incidentes

### 6.1 Relay caído

1. Verificar status de Cloudflare / proceso.
2. Revisar logs y métricas.
3. Si es un deploy reciente, `wrangler rollback`.
4. Si es el relay Rust, reiniciar el contenedor.
5. Comunicar: las salas activas se pierden; los clientes muestran error.
6. Postmortem: causa, duración, acción.

### 6.2 Abuso / spam

1. Identificar IP/sala en logs (sin PII).
2. Aplicar rate limit más estricto o bloquear IP en Cloudflare.
3. Deshabilitar `QUEUE_ENABLED` si el abuso es de cola.
4. Si es un bug, parche y release.

### 6.3 Sala atascada

1. La sala expira a los 5 min sin actividad; verificar el alarm.
2. Si el host sigue conectado pero no hay mensajes, pedir reconexión.
3. Si el DO está corrupto, cerrar la sala (`room_closed`) y que el host cree otra.

### 6.4 Fuga de datos

1. Determinar qué se filtró (nombres, URIs, tokens).
2. Revocar tokens si aplica (no hay tokens de Spotify en el relay).
3. Rotar secretos de despliegue.
4. Notificar a los usuarios si hay PII.
5. Postmortem público si el proyecto es open source.

### 6.5 Cliente roto tras release

1. Reproducir con la versión anterior.
2. Si es protocolo, desplegar relay compatible o kill switch.
3. Publicar hotfix; no borrar la release rota, marcarla.

---

## 7. Capacidad y costes

### Cloudflare

- Durable Objects: cobro por requests y duración; una sala de 8 personas genera ~4 msg/s, bajo.
- Workers: requests por conexión; el plan gratuito puede no ser suficiente para uso público.
- Estimar con métricas reales; poner límite de salas por IP.

### Relay Rust

- 1 vCPU / 256 MB soporta decenas de salas.
- Ancho de banda: ~4 KB/s por sala en reposo, picos de snapshot.
- Docker en un VPS de 5 €/mes es suficiente para un grupo.

---

## 8. Mantenimiento

| Tarea | Frecuencia |
|---|---|
| Actualizar dependencias | Semanal (Dependabot/renovate) |
| Rebase de upstream | Semanal o ante release |
| Revisar alertas | Diario (si hay servicio) |
| Rotar tokens de deploy | Trimestral |
| Probar rollback | Cada release |
| Revisar logs de abuso | Semanal |
| Actualizar docs | Cada release |
| Backup | No aplica (sin persistencia); export de cola es del usuario |

---

## 9. Privacidad

- El relay ve metadatos de la sala (nombres, URIs, títulos). No ve audio ni tokens.
- No se persiste nada; los Durable Objects son efímeros.
- El operador del relay debe informar de ello en la guía de self-hosting.
- El cliente no envía el historial completo ni la biblioteca.
- Los nombres son opcionales; se puede usar un alias.
- GDPR: no hay cuentas ni PII persistente; aún así, documentar la retención cero.

---

## 10. Compatibilidad de despliegue

- Mantener el relay v1 y v2 en paralelo durante la transición.
- El cliente nuevo usa v2 si el relay lo anuncia; si no, v1.
- El relay v2 debe aceptar clientes v1 (sin `v=2`).
- No romper `relay_server_url` existente.
- Documentar qué versión de relay requiere cada versión de cliente.

---

## 11. Checklist de operación antes de release

- [ ] Relay staging desplegado y smoke test verde.
- [ ] Relay producción con rollback probado.
- [ ] Métricas y alertas configuradas.
- [ ] Healthcheck responde.
- [ ] Rate limits verificados.
- [ ] Kill switch probado.
- [ ] Docs de self-hosting probadas en una máquina limpia.
- [ ] Tokens de deploy en environment con aprobación.
- [ ] Plan de comunicación de incidentes.
