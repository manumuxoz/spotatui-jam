use std::{collections::VecDeque, net::SocketAddr, time::Duration};

use futures_util::{SinkExt, StreamExt};
use party_relay::{router, AppState, Config};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{timeout, Instant},
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn spawn_server(config: Config) -> SocketAddr {
    let state = AppState::new(config);
    state.spawn_reaper();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    addr
}

struct TestClient {
    ws: Ws,
    inbox: VecDeque<Value>,
}

impl TestClient {
    async fn connect(addr: SocketAddr, params: &[(&str, &str)]) -> Self {
        let query = params
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("ws://{addr}/ws?{query}");
        let (ws, _) = connect_async(url).await.expect("websocket handshake");
        Self {
            ws,
            inbox: VecDeque::new(),
        }
    }

    async fn send(&mut self, value: Value) {
        self.send_raw(value.to_string()).await;
    }

    async fn send_raw(&mut self, raw: String) {
        self.ws
            .send(Message::Text(raw.into()))
            .await
            .expect("websocket send");
    }

    async fn recv(&mut self, wait: Duration) -> Option<Value> {
        if let Some(value) = self.inbox.pop_front() {
            return Some(value);
        }
        let deadline = Instant::now() + wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, self.ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    return serde_json::from_str(text.as_str()).ok();
                }
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => {
                    return None;
                }
                Ok(Some(Ok(_))) => {}
            }
        }
    }

    async fn expect_type(&mut self, kind: &str, wait: Duration) -> Value {
        let deadline = Instant::now() + wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Some(message) = self.recv(remaining).await else {
                panic!("timed out waiting for {kind}");
            };
            if message.get("type").and_then(Value::as_str) == Some(kind) {
                return message;
            }
        }
    }

    async fn expect(&mut self, kind: &str) -> Value {
        self.expect_type(kind, Duration::from_secs(2)).await
    }

    async fn assert_no_type(&mut self, kind: &str, wait: Duration) {
        let deadline = Instant::now() + wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            let Some(message) = self.recv(remaining).await else {
                return;
            };
            assert_ne!(
                message.get("type").and_then(Value::as_str),
                Some(kind),
                "unexpected {kind}: {message}"
            );
        }
    }

    async fn expect_close(&mut self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, self.ws.next()).await {
                Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) => return true,
                Ok(Some(Ok(_))) => {}
                Err(_) => return false,
            }
        }
    }

    async fn disconnect(mut self) {
        let _ = self.ws.close(None).await;
    }
}

async fn create_host(addr: SocketAddr, name: &str) -> (TestClient, Value) {
    let mut client = TestClient::connect(
        addr,
        &[
            ("action", "create"),
            ("v", "2"),
            ("features", "queue"),
            ("name", name),
        ],
    )
    .await;
    let created = client.expect("room_created").await;
    (client, created)
}

async fn join_guest(addr: SocketAddr, code: &str, name: &str) -> (TestClient, Value) {
    let mut client = TestClient::connect(
        addr,
        &[
            ("action", "join"),
            ("code", code),
            ("v", "2"),
            ("features", "queue"),
            ("name", name),
        ],
    )
    .await;
    let joined = client.expect("joined_room").await;
    (client, joined)
}

fn queue_command(op_id: &str) -> Value {
    json!({
      "type": "queue_command",
      "op_id": op_id,
      "base_rev": 0,
      "action": {
        "op": "add",
        "uri": "spotify:track:7o2AeQZzfCERsRmOM86EcB",
        "title": "Archangel",
        "artists": ["Burial"],
        "duration_ms": 299000,
      },
    })
}

async fn http_get(addr: SocketAddr, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or((response.as_str(), ""));
    let status = head.lines().next().unwrap_or_default().to_owned();
    (status, body.to_owned())
}

#[tokio::test]
async fn create_and_join_handshake() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;

    let code = created["code"].as_str().expect("code").to_owned();
    assert_eq!(code.len(), 6);
    assert!(code
        .chars()
        .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit()));
    assert_eq!(created["protocol_version"], 2);
    assert_eq!(created["features"], json!(["queue"]));
    assert_eq!(created["queue_revision"], 0);
    assert_eq!(created["control_mode"], "host_only");
    assert_eq!(created["playback_mode"], "follow_host");
    assert_eq!(created["you"]["id"], "host");
    assert_eq!(created["you"]["name"], "manu");
    assert_eq!(created["you"]["role"], "host");

    let (mut guest, joined) = join_guest(addr, &code, "ana").await;
    let guest_id = joined["you"]["id"].as_str().expect("guest id").to_owned();
    assert_ne!(guest_id, "host");
    assert_eq!(joined["host_name"], "manu");
    assert_eq!(joined["protocol_version"], 2);
    assert_eq!(joined["features"], json!(["queue"]));
    assert_eq!(joined["queue_revision"], 0);
    assert_eq!(joined["control_mode"], "host_only");
    assert_eq!(joined["you"]["name"], "ana");
    assert_eq!(joined["you"]["role"], "guest");

    let notice = host.expect("guest_joined").await;
    assert_eq!(notice["id"], guest_id);
    assert_eq!(notice["name"], "ana");

    guest.send(queue_command("op-1")).await;
    let command = host.expect("queue_command").await;
    assert_eq!(command["from"], "ana");
    assert_eq!(command["from_id"], guest_id);
}

#[tokio::test]
async fn guest_messages_reach_host_with_relay_identity() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut guest, joined) = join_guest(addr, code, "ana").await;
    let guest_id = joined["you"]["id"].as_str().unwrap().to_owned();
    let _ = host.expect("guest_joined").await;

    guest
        .send(json!({
          "type": "queue_command",
          "op_id": "op-1",
          "from": "spoofed-name",
          "from_id": "spoofed-id",
          "base_rev": 0,
          "action": { "op": "add" },
        }))
        .await;
    let command = host.expect("queue_command").await;
    assert_eq!(command["from"], "ana");
    assert_eq!(command["from_id"], guest_id);
    assert_eq!(command["op_id"], "op-1");

    guest
        .send(json!({
          "type": "playback_command",
          "action": "next_track",
          "from": "spoofed-name",
          "from_id": "spoofed-id",
        }))
        .await;
    let playback = host.expect("playback_command").await;
    assert_eq!(playback["from"], "ana");
    assert_eq!(playback["from_id"], guest_id);

    guest
        .send(json!({
          "type": "queue_resync",
          "have_revision": 41,
          "from_id": "spoofed-id",
        }))
        .await;
    let resync = host.expect("queue_resync").await;
    assert_eq!(resync["from"], "ana");
    assert_eq!(resync["from_id"], guest_id);
    assert_eq!(resync["have_revision"], 41);
}

#[tokio::test]
async fn guests_cannot_send_host_only_messages() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut guest, _joined) = join_guest(addr, code, "ana").await;
    let _ = host.expect("guest_joined").await;

    guest
        .send(json!({
          "type": "queue_patch",
          "revision": 1,
          "ops": [{ "op": "clear" }],
        }))
        .await;
    guest
        .send(json!({
          "type": "party_config",
          "config": { "queue_mode": "guests_manage" },
        }))
        .await;
    guest
        .send(json!({
          "type": "not_a_real_type",
          "payload": { "anything": true },
        }))
        .await;

    host.assert_no_type("queue_patch", Duration::from_millis(300))
        .await;
    host.assert_no_type("party_config", Duration::from_millis(100))
        .await;

    guest.send(queue_command("still-open")).await;
    let command = host.expect("queue_command").await;
    assert_eq!(command["op_id"], "still-open");
    assert_ne!(command["type"], "queue_patch");
}

#[tokio::test]
async fn directed_snapshot_and_ack_reach_only_the_target() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut first, first_joined) = join_guest(addr, code, "ana").await;
    let first_id = first_joined["you"]["id"].as_str().unwrap().to_owned();
    let (mut second, second_joined) = join_guest(addr, code, "luis").await;
    let second_id = second_joined["you"]["id"].as_str().unwrap().to_owned();
    let _ = host.expect("guest_joined").await;
    let _ = host.expect("guest_joined").await;

    host.send(json!({
      "type": "queue_snapshot",
      "revision": 1,
      "snapshot_id": "s-private",
      "chunk": 0,
      "chunks": 1,
      "to": second_id,
      "current": null,
      "items": [],
    }))
    .await;
    let snapshot = second.expect("queue_snapshot").await;
    assert_eq!(snapshot["snapshot_id"], "s-private");
    first
        .assert_no_type("queue_snapshot", Duration::from_millis(300))
        .await;

    host.send(json!({
      "type": "queue_ack",
      "op_id": "op-9",
      "ok": true,
      "revision": 1,
      "to": first_id,
    }))
    .await;
    let ack = first.expect("queue_ack").await;
    assert_eq!(ack["op_id"], "op-9");
    second
        .assert_no_type("queue_ack", Duration::from_millis(300))
        .await;

    host.send(json!({
      "type": "queue_ack",
      "op_id": "op-by-name",
      "ok": true,
      "revision": 1,
      "to": "ana",
    }))
    .await;
    let ack = first.expect("queue_ack").await;
    assert_eq!(ack["op_id"], "op-by-name");

    host.send(json!({
      "type": "queue_snapshot",
      "revision": 2,
      "snapshot_id": "s-broadcast",
      "chunk": 0,
      "chunks": 1,
      "current": null,
      "items": [],
    }))
    .await;
    let first_copy = first.expect("queue_snapshot").await;
    let second_copy = second.expect("queue_snapshot").await;
    assert_eq!(first_copy["snapshot_id"], "s-broadcast");
    assert_eq!(second_copy["snapshot_id"], "s-broadcast");
}

#[tokio::test]
async fn oversized_message_is_rejected_without_closing() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut guest, _joined) = join_guest(addr, code, "ana").await;
    let _ = host.expect("guest_joined").await;

    guest
        .send_raw(
            json!({
              "type": "queue_command",
              "op_id": "too-big",
              "padding": "x".repeat(5000),
            })
            .to_string(),
        )
        .await;
    let error = guest.expect("error").await;
    assert_eq!(error["code"], "message_too_large");

    guest.send(queue_command("after-error")).await;
    let command = host.expect("queue_command").await;
    assert_eq!(command["op_id"], "after-error");
}

#[tokio::test]
async fn queue_commands_are_rate_limited_per_session() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut guest, joined) = join_guest(addr, code, "ana").await;
    let guest_id = joined["you"]["id"].as_str().unwrap().to_owned();
    let _ = host.expect("guest_joined").await;

    for index in 0..10 {
        guest.send(queue_command(&format!("op-{index}"))).await;
    }
    for _ in 0..10 {
        let command = host.expect("queue_command").await;
        assert_eq!(command["from_id"], guest_id);
    }

    guest.send(queue_command("op-over-limit")).await;
    let error = guest.expect("error").await;
    assert_eq!(error["code"], "rate_limited");
    host.assert_no_type("queue_command", Duration::from_millis(250))
        .await;

    tokio::time::sleep(Duration::from_millis(600)).await;
    guest.send(queue_command("op-refilled")).await;
    let command = host.expect("queue_command").await;
    assert_eq!(command["op_id"], "op-refilled");
}

#[tokio::test]
async fn control_mode_updates_and_guest_left_is_broadcast() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap().to_owned();
    let (mut first, _first_joined) = join_guest(addr, &code, "ana").await;
    let (second, second_joined) = join_guest(addr, &code, "luis").await;
    let second_id = second_joined["you"]["id"].as_str().unwrap().to_owned();
    let _ = host.expect("guest_joined").await;
    let _ = host.expect("guest_joined").await;

    host.send(json!({
      "type": "set_control_mode",
      "control_mode": "shared_control",
    }))
    .await;
    assert_eq!(
        host.expect("set_control_mode").await["control_mode"],
        "shared_control"
    );
    assert_eq!(
        first.expect("set_control_mode").await["control_mode"],
        "shared_control"
    );

    second.disconnect().await;
    let left = host.expect("guest_left").await;
    assert_eq!(left["id"], second_id);
    assert_eq!(left["name"], "luis");
    let left_first = first.expect("guest_left").await;
    assert_eq!(left_first["id"], second_id);

    let (late, late_joined) = join_guest(addr, &code, "eva").await;
    assert_eq!(late_joined["control_mode"], "shared_control");
    let _ = host.expect("guest_joined").await;
    late.disconnect().await;
}

#[tokio::test]
async fn host_leaving_closes_the_room() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut guest, _joined) = join_guest(addr, code, "ana").await;
    let _ = host.expect("guest_joined").await;

    host.disconnect().await;

    let closed = guest.expect("room_closed").await;
    assert_eq!(closed["type"], "room_closed");
    assert!(guest.expect_close(Duration::from_secs(2)).await);
}

#[tokio::test]
async fn set_name_updates_participant_identity_and_notifies() {
    let addr = spawn_server(Config::default()).await;
    let (mut host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap().to_owned();
    let (mut guest, joined) = join_guest(addr, &code, "ana").await;
    let guest_id = joined["you"]["id"].as_str().unwrap().to_owned();
    let _ = host.expect("guest_joined").await;

    guest
        .send(json!({ "type": "set_name", "name": "  ana   2  " }))
        .await;
    let left = host.expect("guest_left").await;
    assert_eq!(left["id"], guest_id);
    assert_eq!(left["name"], "ana");
    let renamed = host.expect("guest_joined").await;
    assert_eq!(renamed["id"], guest_id);
    assert_eq!(renamed["name"], "ana 2");

    guest.send(queue_command("op-renamed")).await;
    let command = host.expect("queue_command").await;
    assert_eq!(command["from"], "ana 2");
    assert_eq!(command["from_id"], guest_id);

    host.send(json!({ "type": "set_name", "name": "manu 2" }))
        .await;
    let notice = guest.expect("set_name").await;
    assert_eq!(notice["id"], "host");
    assert_eq!(notice["name"], "manu 2");

    let (_late, late_joined) = join_guest(addr, &code, "luis").await;
    assert_eq!(late_joined["host_name"], "manu 2");
}

#[tokio::test]
async fn room_accepts_at_most_eight_participants() {
    let addr = spawn_server(Config::default()).await;
    let (_host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap().to_owned();

    let mut guests = Vec::new();
    for index in 0..7 {
        let (client, _joined) = join_guest(addr, &code, &format!("guest-{index}")).await;
        guests.push(client);
    }

    let mut rejected = TestClient::connect(
        addr,
        &[
            ("action", "join"),
            ("code", &code),
            ("v", "2"),
            ("name", "one-too-many"),
        ],
    )
    .await;
    let error = rejected.expect("error").await;
    assert_eq!(error["code"], "room_unavailable");
    assert!(rejected.expect_close(Duration::from_secs(2)).await);

    drop(guests);
}

#[tokio::test]
async fn room_expires_after_inactivity() {
    let config = Config {
        room_ttl: Duration::from_secs(1),
        ..Config::default()
    };
    let addr = spawn_server(config).await;
    let (_host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (mut guest, _joined) = join_guest(addr, code, "ana").await;

    let closed = guest
        .expect_type("room_closed", Duration::from_secs(5))
        .await;
    assert_eq!(closed["type"], "room_closed");
    assert!(guest.expect_close(Duration::from_secs(2)).await);
}

#[tokio::test]
async fn healthz_reports_active_rooms() {
    let addr = spawn_server(Config::default()).await;
    let (status, body) = http_get(addr, "/healthz").await;
    assert!(status.contains("200"), "unexpected status: {status}");
    let empty: Value = serde_json::from_str(&body).expect("health json");
    assert_eq!(empty, json!({ "ok": true, "rooms": 0 }));

    let (_host, created) = create_host(addr, "manu").await;
    let code = created["code"].as_str().unwrap();
    let (_guest, _joined) = join_guest(addr, code, "ana").await;

    let (status, body) = http_get(addr, "/healthz").await;
    assert!(status.contains("200"), "unexpected status: {status}");
    let health: Value = serde_json::from_str(&body).expect("health json");
    assert_eq!(health, json!({ "ok": true, "rooms": 1 }));
}
