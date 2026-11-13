#!/usr/bin/env node
/**
 * Smoke test del relay Party Queue v2.
 *
 * Uso:
 *   node scripts/smoke-party.mjs --url wss://party-staging.example.com/ws
 *
 * Simula un host y un invitado, crea la sala, pide un add y verifica
 * queue_command -> queue_ack -> queue_patch. No reproduce audio.
 *
 * Requiere Node 22+ (WebSocket global). Si se usa una versión anterior,
 * instalar `ws` y adaptar el import.
 */

const args = process.argv.slice(2);
const urlArg = args.indexOf("--url");
const relayUrl = urlArg >= 0 ? args[urlArg + 1] : process.env.PARTY_RELAY_URL;

if (!relayUrl) {
  console.error("Falta --url o PARTY_RELAY_URL");
  process.exit(2);
}

const TIMEOUT_MS = 10_000;

let WebSocketCtor = globalThis.WebSocket;

async function resolveWebSocket() {
  if (WebSocketCtor) return WebSocketCtor;
  const { createRequire } = await import("node:module");
  const { dirname, join } = await import("node:path");
  const candidates = [
    process.cwd(),
    join(process.cwd(), "work/spotatui/worker-relay"),
    join(dirname(process.cwd()), "work/spotatui/worker-relay"),
  ];
  for (const dir of candidates) {
    try {
      const require = createRequire(join(dir, "package.json"));
      const mod = require("ws");
      if (mod?.WebSocket) return mod.WebSocket;
    } catch {
      // try the next candidate
    }
  }
  throw new Error("no WebSocket implementation; use Node 22+ or install ws");
}

function withTimeout(promise, label) {
  return Promise.race([
    promise,
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error(`timeout: ${label}`)), TIMEOUT_MS),
    ),
  ]);
}

function connect(params) {
  const url = new URL(relayUrl);
  for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v);
  const ws = new WebSocketCtor(url);
  const inbox = [];
  const waiters = [];

  ws.addEventListener("message", (event) => {
    let msg;
    try {
      msg = JSON.parse(event.data);
    } catch {
      return;
    }
    const waiter = waiters.find((w) => w.type === msg.type);
    if (waiter) {
      waiters.splice(waiters.indexOf(waiter), 1);
      waiter.resolve(msg);
    } else {
      inbox.push(msg);
    }
  });

  const opened = new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  return {
    ws,
    opened,
    send(msg) {
      ws.send(JSON.stringify(msg));
    },
    waitFor(type) {
      const idx = inbox.findIndex((m) => m.type === type);
      if (idx >= 0) return Promise.resolve(inbox.splice(idx, 1)[0]);
      return withTimeout(
        new Promise((resolve, reject) => waiters.push({ type, resolve, reject })),
        type,
      );
    },
    close() {
      try {
        ws.close();
      } catch {
        // ignore
      }
    },
  };
}

async function main() {
  WebSocketCtor = await resolveWebSocket();
  const host = connect({
    action: "create",
    v: "2",
    features: "queue",
    name: "smoke-host",
  });
  await host.opened;
  const created = await host.waitFor("room_created");
  const code = created.code;
  if (!code) throw new Error("room_created sin code");
  console.log(`sala ${code} creada`);

  const guest = connect({
    action: "join",
    code,
    v: "2",
    features: "queue",
    name: "smoke-guest",
  });
  await guest.opened;
  const joined = await guest.waitFor("joined_room");
  console.log(`guest unido como ${joined.you?.id ?? "?"}`);

  // El host publica configuración y snapshot inicial.
  host.send({
    type: "party_config",
    config: {
      queue_mode: "guests_add",
      playback_mode: "follow_host",
      max_items: 100,
      max_per_guest: 5,
      add_cooldown_ms: 0,
      dedupe: true,
      allow_explicit: true,
      stop_when_empty: false,
      votes_enabled: false,
    },
  });
  host.send({
    type: "queue_snapshot",
    revision: 0,
    snapshot_id: "smoke-0",
    chunk: 0,
    chunks: 1,
    current: null,
    items: [],
    history_tail: [],
  });

  const opId = `smoke-${Date.now()}`;
  guest.send({
    type: "queue_command",
    op_id: opId,
    base_rev: 0,
    action: {
      op: "add",
      uri: "spotify:track:7o2AeQZzfCERsRmOM86EcB",
      title: "Archangel",
      artists: ["Burial"],
      duration_ms: 299000,
    },
  });

  const command = await host.waitFor("queue_command");
  if (command.op_id !== opId) throw new Error("op_id no coincide");
  if (!command.from) throw new Error("el relay no asignó from");
  if (command.action?.op !== "add") throw new Error("acción inesperada");
  console.log(`comando recibido de ${command.from}`);

  const item = {
    id: 1,
    uri: command.action.uri,
    title: command.action.title,
    artists: command.action.artists,
    duration_ms: command.action.duration_ms,
    added_by: command.from,
    added_by_name: "smoke-guest",
    votes: 0,
    added_at_ms: Date.now(),
  };

  host.send({
    type: "queue_ack",
    op_id: opId,
    ok: true,
    revision: 1,
    to: command.from,
  });
  host.send({
    type: "queue_patch",
    revision: 1,
    ops: [{ op: "add", item }],
  });

  const ack = await guest.waitFor("queue_ack");
  if (!ack.ok) throw new Error(`ack falló: ${ack.error}`);
  const patch = await guest.waitFor("queue_patch");
  if (patch.revision !== 1 || patch.ops?.[0]?.op !== "add") {
    throw new Error("patch inesperado");
  }

  console.log("smoke OK: add -> ack -> patch");
  host.close();
  guest.close();
}

main().catch((err) => {
  console.error(`smoke FAIL: ${err.message}`);
  process.exit(1);
});
