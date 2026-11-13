use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::debug;

use crate::{
    state::{
        close_sessions, sanitize_name, AttemptKind, ControlMode, CreatedRoom, JoinError, Outgoing,
        Role, Room, Session,
    },
    AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct WsParams {
    action: Option<String>,
    code: Option<String>,
    name: Option<String>,
    control_mode: Option<String>,
}

#[derive(Clone, Copy)]
enum Action {
    Create,
    Join,
}

impl Action {
    fn attempt(self) -> AttemptKind {
        match self {
            Self::Create => AttemptKind::Create,
            Self::Join => AttemptKind::Join,
        }
    }
}

pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(params): Query<WsParams>,
) -> Response {
    let action = match params.action.as_deref() {
        Some("create") => Action::Create,
        Some("join") => Action::Join,
        _ => return bad_request("Missing action=create or action=join"),
    };

    let code = match action {
        Action::Create => None,
        Action::Join => {
            let Some(raw_code) = params.code.as_deref() else {
                return bad_request("Missing room code");
            };
            let code = raw_code.to_ascii_uppercase();
            if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
                return bad_request("Invalid room code");
            }
            Some(code)
        }
    };

    if !state.allow_connect(peer.ip(), action.attempt()) {
        return (StatusCode::TOO_MANY_REQUESTS, "Too many attempts").into_response();
    }

    let control_mode = ControlMode::from_query(params.control_mode.as_deref());
    let requested_name = params.name;
    ws.on_upgrade(move |socket| {
        handle_socket(socket, state, action, code, requested_name, control_mode)
    })
}

async fn handle_socket(
    socket: WebSocket,
    state: Arc<AppState>,
    action: Action,
    code: Option<String>,
    requested_name: Option<String>,
    control_mode: ControlMode,
) {
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel::<Outgoing>();
    let writer = tokio::spawn(writer_task(sink, rx));
    let max_message_bytes = state.config.max_message_bytes;

    let (room, session) = match action {
        Action::Create => {
            let host_name = sanitize_name(requested_name.as_deref(), "Host");
            match state.create_room(host_name, control_mode, tx.clone()).await {
                Some(created) => {
                    let hello =
                        room_created_message(&created, control_mode, state.config.queue_enabled);
                    created.session.send_value(&hello);
                    (created.room, created.session)
                }
                None => {
                    reject(
                        &tx,
                        "internal",
                        "Could not allocate a room code",
                        "Internal error",
                    );
                    drop(tx);
                    wait_writer(writer).await;
                    return;
                }
            }
        }
        Action::Join => {
            let guest_name = sanitize_name(requested_name.as_deref(), "Guest");
            let queue_enabled = state.config.queue_enabled;
            let joined = state
                .join_room(
                    code.as_deref().unwrap_or_default(),
                    guest_name,
                    tx.clone(),
                    |session, host_name, mode| {
                        joined_room_message(session, host_name, mode, queue_enabled)
                    },
                )
                .await;
            match joined {
                Ok(info) => {
                    let notice = guest_joined_message(&info.session);
                    for target in info.room.others(&info.session.id).await {
                        target.send_value(&notice);
                    }
                    (info.room, info.session)
                }
                Err(error) => {
                    let (message, reason) = match error {
                        JoinError::NotFound => ("Room not found", "Room not found"),
                        JoinError::Full => ("Room is full", "Room full"),
                    };
                    reject(&tx, "room_unavailable", message, reason);
                    drop(tx);
                    wait_writer(writer).await;
                    return;
                }
            }
        }
    };
    drop(tx);

    while let Some(frame) = stream.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                room.touch();
                let raw = text.as_str();
                if raw.len() > max_message_bytes {
                    session.send_error(
                        "message_too_large",
                        "Message exceeds the configured byte limit",
                    );
                    continue;
                }
                let value: Value = match serde_json::from_str(raw) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let Some(message_type) =
                    value.get("type").and_then(Value::as_str).map(str::to_owned)
                else {
                    continue;
                };
                match session.role {
                    Role::Host => {
                        handle_host_message(
                            &room,
                            &session,
                            &message_type,
                            &value,
                            raw,
                            state.config.queue_enabled,
                        )
                        .await;
                    }
                    Role::Guest => {
                        handle_guest_message(
                            &room,
                            &session,
                            &message_type,
                            value,
                            max_message_bytes,
                            state.config.queue_enabled,
                        )
                        .await;
                    }
                }
            }
            Ok(Message::Binary(bytes)) => {
                room.touch();
                if bytes.len() > max_message_bytes {
                    session.send_error(
                        "message_too_large",
                        "Message exceeds the configured byte limit",
                    );
                }
            }
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(error) => {
                debug!(%error, "websocket receive failed");
                break;
            }
        }
    }

    cleanup(&state, &room, &session).await;
    drop(session);
    wait_writer(writer).await;
}

async fn writer_task(
    mut sink: SplitSink<WebSocket, Message>,
    mut rx: mpsc::UnboundedReceiver<Outgoing>,
) {
    while let Some(outgoing) = rx.recv().await {
        match outgoing {
            Outgoing::Text(text) => {
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            Outgoing::Close { code, reason } => {
                let frame = CloseFrame {
                    code,
                    reason: reason.into(),
                };
                let _ = sink.send(Message::Close(Some(frame))).await;
                break;
            }
        }
    }
    let _ = sink.close().await;
}

async fn wait_writer(mut writer: tokio::task::JoinHandle<()>) {
    if tokio::time::timeout(Duration::from_secs(1), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
    }
}

fn reject(tx: &mpsc::UnboundedSender<Outgoing>, code: &str, message: &str, reason: &str) {
    let payload = json!({
      "type": "error",
      "code": code,
      "message": message,
    })
    .to_string();
    let _ = tx.send(Outgoing::Text(payload));
    let _ = tx.send(Outgoing::Close {
        code: 1008,
        reason: reason.to_owned(),
    });
}

async fn cleanup(state: &AppState, room: &Arc<Room>, session: &Arc<Session>) {
    match session.role {
        Role::Host => {
            let guests = room.close_host_left(&session.id).await;
            close_sessions(&guests, "Host left");
            state.remove_room(&room.code, room).await;
        }
        Role::Guest => {
            if let Some(left) = room.remove_guest(&session.id).await {
                let notice = guest_left_message(&left);
                for target in room.others(&left.id).await {
                    target.send_value(&notice);
                }
            }
        }
    }
}

async fn handle_host_message(
    room: &Room,
    session: &Session,
    message_type: &str,
    value: &Value,
    raw: &str,
    queue_enabled: bool,
) {
    match message_type {
        "sync_state" | "heartbeat" | "party_config" | "party_members" => {
            broadcast_to_guests(room, raw).await;
        }
        "queue_patch" if queue_enabled => {
            broadcast_to_guests(room, raw).await;
        }
        "set_control_mode" => {
            let mode = ControlMode::from_wire(
                value
                    .get("control_mode")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            if room.set_control_mode(mode).await {
                let mut message = value.clone();
                if let Some(object) = message.as_object_mut() {
                    object.insert(
                        "control_mode".to_owned(),
                        Value::String(mode.as_str().to_owned()),
                    );
                }
                broadcast_to_all(room, &message.to_string()).await;
            }
        }
        "queue_snapshot" | "queue_ack" if queue_enabled => {
            route_directed(room, value, raw).await;
        }
        "set_name" => {
            let Some(raw_name) = value.get("name").and_then(Value::as_str) else {
                return;
            };
            let new_name = sanitize_name(Some(raw_name), "Host");
            let old_name = session.set_name(new_name.clone());
            room.rename_host(&new_name).await;
            if old_name != new_name {
                let notice = json!({
                  "type": "set_name",
                  "id": "host",
                  "name": new_name,
                });
                broadcast_to_guests(room, &notice.to_string()).await;
            }
        }
        _ => {}
    }
}

async fn handle_guest_message(
    room: &Room,
    session: &Session,
    message_type: &str,
    value: Value,
    max_message_bytes: usize,
    queue_enabled: bool,
) {
    match message_type {
        "set_name" => {
            let Some(raw_name) = value.get("name").and_then(Value::as_str) else {
                return;
            };
            let new_name = sanitize_name(Some(raw_name), "Guest");
            let old_name = session.set_name(new_name.clone());
            if old_name == new_name {
                return;
            }
            let left = json!({
              "type": "guest_left",
              "id": session.id.as_str(),
              "name": old_name,
            });
            let joined = json!({
              "type": "guest_joined",
              "id": session.id.as_str(),
              "name": new_name,
            });
            for target in room.others(&session.id).await {
                target.send_value(&left);
                target.send_value(&joined);
            }
            return;
        }
        "playback_command" => {}
        "queue_command" if queue_enabled => {
            if !session.allow_queue_command() {
                session.send_error("rate_limited", "Too many queue_command messages");
                return;
            }
        }
        "queue_resync" if queue_enabled => {}
        _ => return,
    }

    let rewritten = rewrite_from(value, session);
    if rewritten.len() > max_message_bytes {
        session.send_error(
            "message_too_large",
            "Message exceeds the configured byte limit after relay metadata",
        );
        return;
    }
    if let Some(host) = room.host().await {
        host.send_text(rewritten);
    }
}

fn rewrite_from(mut value: Value, session: &Session) -> String {
    if let Some(object) = value.as_object_mut() {
        object.insert("from".to_owned(), Value::String(session.name()));
        object.insert("from_id".to_owned(), Value::String(session.id.clone()));
    }
    value.to_string()
}

async fn broadcast_to_guests(room: &Room, raw: &str) {
    for guest in room.guests().await {
        guest.send_text(raw.to_owned());
    }
}

async fn broadcast_to_all(room: &Room, raw: &str) {
    for session in room.all_sessions().await {
        session.send_text(raw.to_owned());
    }
}

async fn route_directed(room: &Room, value: &Value, raw: &str) {
    match value.get("to") {
        None | Some(Value::Null) => broadcast_to_guests(room, raw).await,
        Some(Value::String(target_id)) => {
            if let Some(target) = room.find(target_id).await {
                target.send_text(raw.to_owned());
            }
        }
        Some(_) => {}
    }
}

fn features(queue_enabled: bool) -> Value {
    if queue_enabled {
        json!(["queue"])
    } else {
        json!([])
    }
}

fn you_object(session: &Session) -> Value {
    let name = session.name();
    json!({
      "id": session.id.as_str(),
      "name": name,
      "role": session.role.as_str(),
    })
}

fn room_created_message(
    created: &CreatedRoom,
    control_mode: ControlMode,
    queue_enabled: bool,
) -> Value {
    json!({
      "type": "room_created",
      "code": created.code.as_str(),
      "control_mode": control_mode.as_str(),
      "protocol_version": 2,
      "features": features(queue_enabled),
      "playback_mode": "follow_host",
      "you": you_object(&created.session),
      "queue_revision": 0,
    })
}

fn joined_room_message(
    session: &Session,
    host_name: &str,
    control_mode: ControlMode,
    queue_enabled: bool,
) -> Value {
    json!({
      "type": "joined_room",
      "host_name": host_name,
      "control_mode": control_mode.as_str(),
      "protocol_version": 2,
      "features": features(queue_enabled),
      "playback_mode": "follow_host",
      "you": you_object(session),
      "queue_revision": 0,
    })
}

fn guest_joined_message(session: &Session) -> Value {
    let name = session.name();
    json!({
      "type": "guest_joined",
      "id": session.id.as_str(),
      "name": name,
    })
}

fn guest_left_message(session: &Session) -> Value {
    let name = session.name();
    json!({
      "type": "guest_left",
      "id": session.id.as_str(),
      "name": name,
    })
}

fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}
