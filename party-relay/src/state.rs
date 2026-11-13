use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use uuid::Uuid;

use crate::{
    config::Config,
    limits::{KeyedLimiter, TokenBucket},
};

const QUEUE_COMMAND_BURST: f64 = 10.0;
const QUEUE_COMMAND_REFILL_PER_SEC: f64 = 2.0;
const CREATE_ATTEMPTS_BURST: f64 = 20.0;
const CREATE_ATTEMPTS_REFILL_PER_SEC: f64 = 0.5;
const JOIN_ATTEMPTS_BURST: f64 = 50.0;
const JOIN_ATTEMPTS_REFILL_PER_SEC: f64 = 5.0;
const ROOM_CODE_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ROOM_CODE_LEN: usize = 6;
const MAX_NAME_CHARS: usize = 32;

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn lock<T>(mutex: &StdMutex<T>) -> StdMutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Host,
    Guest,
}

impl Role {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Guest => "guest",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlMode {
    HostOnly,
    SharedControl,
}

impl ControlMode {
    pub(crate) fn from_query(value: Option<&str>) -> Self {
        value.map(Self::from_wire).unwrap_or(Self::HostOnly)
    }

    pub(crate) fn from_wire(value: &str) -> Self {
        if value == "shared_control" {
            Self::SharedControl
        } else {
            Self::HostOnly
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HostOnly => "host_only",
            Self::SharedControl => "shared_control",
        }
    }
}

#[derive(Debug)]
pub(crate) enum Outgoing {
    Text(String),
    Close { code: u16, reason: String },
}

pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) role: Role,
    name: StdMutex<String>,
    tx: mpsc::UnboundedSender<Outgoing>,
    queue_limiter: StdMutex<TokenBucket>,
    max_message_bytes: usize,
}

impl Session {
    fn new(
        id: String,
        role: Role,
        name: String,
        tx: mpsc::UnboundedSender<Outgoing>,
        max_message_bytes: usize,
    ) -> Self {
        let queue_limiter = if role == Role::Guest {
            TokenBucket::new(QUEUE_COMMAND_BURST, QUEUE_COMMAND_REFILL_PER_SEC)
        } else {
            TokenBucket::new(0.0, 0.0)
        };
        Self {
            id,
            role,
            name: StdMutex::new(name),
            tx,
            queue_limiter: StdMutex::new(queue_limiter),
            max_message_bytes,
        }
    }

    pub(crate) fn name(&self) -> String {
        lock(&self.name).clone()
    }

    /// Replaces the name and returns the previous one.
    pub(crate) fn set_name(&self, name: String) -> String {
        std::mem::replace(&mut *lock(&self.name), name)
    }

    pub(crate) fn send_value(&self, value: &Value) -> bool {
        match serde_json::to_string(value) {
            Ok(text) => self.send_text(text),
            Err(error) => {
                tracing::warn!(%error, "failed to serialize an outgoing message");
                false
            }
        }
    }

    pub(crate) fn send_text(&self, text: String) -> bool {
        if text.len() > self.max_message_bytes {
            tracing::debug!(
                bytes = text.len(),
                limit = self.max_message_bytes,
                "dropping an oversized outbound message"
            );
            return false;
        }
        self.tx.send(Outgoing::Text(text)).is_ok()
    }

    pub(crate) fn send_error(&self, code: &str, message: &str) {
        self.send_value(&json!({
          "type": "error",
          "code": code,
          "message": message,
        }));
    }

    pub(crate) fn close(&self, code: u16, reason: &str) {
        let _ = self.tx.send(Outgoing::Close {
            code,
            reason: reason.to_owned(),
        });
    }

    pub(crate) fn allow_queue_command(&self) -> bool {
        lock(&self.queue_limiter).try_acquire()
    }
}

struct RoomInner {
    host: Option<Arc<Session>>,
    guests: Vec<Arc<Session>>,
    control_mode: ControlMode,
    host_name: String,
    closed: bool,
}

pub(crate) struct Room {
    pub(crate) code: String,
    inner: AsyncMutex<RoomInner>,
    last_activity_ms: AtomicU64,
}

impl Room {
    fn new(code: String, host_name: String, control_mode: ControlMode, host: Arc<Session>) -> Self {
        Self {
            code,
            inner: AsyncMutex::new(RoomInner {
                host: Some(host),
                guests: Vec::new(),
                control_mode,
                host_name,
                closed: false,
            }),
            last_activity_ms: AtomicU64::new(now_ms()),
        }
    }

    pub(crate) fn touch(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub(crate) fn last_activity_ms(&self) -> u64 {
        self.last_activity_ms.load(Ordering::Relaxed)
    }

    pub(crate) async fn set_control_mode(&self, mode: ControlMode) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.closed {
            return false;
        }
        inner.control_mode = mode;
        true
    }

    pub(crate) async fn rename_host(&self, name: &str) {
        let mut inner = self.inner.lock().await;
        if !inner.closed {
            inner.host_name = name.to_owned();
        }
    }

    pub(crate) async fn guests(&self) -> Vec<Arc<Session>> {
        self.inner.lock().await.guests.clone()
    }

    pub(crate) async fn all_sessions(&self) -> Vec<Arc<Session>> {
        let inner = self.inner.lock().await;
        if inner.closed {
            return Vec::new();
        }
        inner
            .host
            .iter()
            .chain(inner.guests.iter())
            .cloned()
            .collect()
    }

    pub(crate) async fn host(&self) -> Option<Arc<Session>> {
        self.inner.lock().await.host.clone()
    }

    pub(crate) async fn others(&self, exclude_id: &str) -> Vec<Arc<Session>> {
        let inner = self.inner.lock().await;
        if inner.closed {
            return Vec::new();
        }
        inner
            .host
            .iter()
            .chain(inner.guests.iter())
            .filter(|session| session.id != exclude_id)
            .cloned()
            .collect()
    }

    /// Resolves a directed `to` target by participant id first, then by name.
    /// Only guests are addressable: `queue_snapshot` and `queue_ack` go from the
    /// host to one guest. The reference smoke client addresses acks with
    /// `from`, which is the display name in this relay, so both work.
    pub(crate) async fn find(&self, id_or_name: &str) -> Option<Arc<Session>> {
        let inner = self.inner.lock().await;
        let by_id = inner.guests.iter().find(|session| session.id == id_or_name);
        if let Some(session) = by_id {
            return Some(Arc::clone(session));
        }
        inner
            .guests
            .iter()
            .find(|session| session.name() == id_or_name)
            .cloned()
    }

    pub(crate) async fn remove_guest(&self, id: &str) -> Option<Arc<Session>> {
        let mut inner = self.inner.lock().await;
        if inner.closed {
            return None;
        }
        let position = inner.guests.iter().position(|session| session.id == id)?;
        Some(inner.guests.remove(position))
    }

    /// Closes the room when `expected_host_id` is still its host and returns the
    /// guests that must receive `room_closed`.
    pub(crate) async fn close_host_left(&self, expected_host_id: &str) -> Vec<Arc<Session>> {
        let mut inner = self.inner.lock().await;
        if inner.closed {
            return Vec::new();
        }
        let is_host = inner
            .host
            .as_ref()
            .is_some_and(|host| host.id == expected_host_id);
        if !is_host {
            return Vec::new();
        }
        inner.closed = true;
        inner.host = None;
        std::mem::take(&mut inner.guests)
    }

    /// Returns `None` while the room is still active. Once expired (or already
    /// closed) it returns every remaining session so the caller can close them.
    pub(crate) async fn close_expired(
        &self,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Option<Vec<Arc<Session>>> {
        let mut inner = self.inner.lock().await;
        if inner.closed {
            return Some(Vec::new());
        }
        if now_ms.saturating_sub(self.last_activity_ms()) < ttl_ms {
            return None;
        }
        inner.closed = true;
        let mut sessions = std::mem::take(&mut inner.guests);
        if let Some(host) = inner.host.take() {
            sessions.push(host);
        }
        Some(sessions)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttemptKind {
    Create,
    Join,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JoinError {
    NotFound,
    Full,
}

pub(crate) struct CreatedRoom {
    pub(crate) code: String,
    pub(crate) room: Arc<Room>,
    pub(crate) session: Arc<Session>,
}

pub(crate) struct JoinInfo {
    pub(crate) room: Arc<Room>,
    pub(crate) session: Arc<Session>,
}

pub struct AppState {
    pub config: Config,
    rooms: AsyncMutex<HashMap<String, Arc<Room>>>,
    create_limiter: KeyedLimiter,
    join_limiter: KeyedLimiter,
}

impl AppState {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            rooms: AsyncMutex::new(HashMap::new()),
            create_limiter: KeyedLimiter::new(
                CREATE_ATTEMPTS_BURST,
                CREATE_ATTEMPTS_REFILL_PER_SEC,
            ),
            join_limiter: KeyedLimiter::new(JOIN_ATTEMPTS_BURST, JOIN_ATTEMPTS_REFILL_PER_SEC),
        })
    }

    pub(crate) fn allow_connect(&self, ip: IpAddr, kind: AttemptKind) -> bool {
        match kind {
            AttemptKind::Create => self.create_limiter.allow(ip),
            AttemptKind::Join => self.join_limiter.allow(ip),
        }
    }

    pub(crate) async fn create_room(
        &self,
        host_name: String,
        control_mode: ControlMode,
        tx: mpsc::UnboundedSender<Outgoing>,
    ) -> Option<CreatedRoom> {
        let mut rooms = self.rooms.lock().await;
        for _ in 0..64 {
            let code = generate_room_code();
            if rooms.contains_key(&code) {
                continue;
            }
            let session = Arc::new(Session::new(
                "host".to_owned(),
                Role::Host,
                host_name.clone(),
                tx,
                self.config.max_message_bytes,
            ));
            let room = Arc::new(Room::new(
                code.clone(),
                host_name,
                control_mode,
                session.clone(),
            ));
            rooms.insert(code.clone(), room.clone());
            return Some(CreatedRoom {
                code,
                room,
                session,
            });
        }
        None
    }

    /// Inserts the guest and queues its handshake while the room lock is held, so
    /// no host broadcast can be delivered before `joined_room`.
    pub(crate) async fn join_room<F>(
        &self,
        code: &str,
        name: String,
        tx: mpsc::UnboundedSender<Outgoing>,
        handshake: F,
    ) -> Result<JoinInfo, JoinError>
    where
        F: FnOnce(&Session, &str, ControlMode) -> Value,
    {
        let room = {
            let rooms = self.rooms.lock().await;
            rooms.get(code).cloned()
        }
        .ok_or(JoinError::NotFound)?;

        let mut inner = room.inner.lock().await;
        if inner.closed || inner.host.is_none() {
            return Err(JoinError::NotFound);
        }
        if inner.guests.len() + 2 > self.config.max_participants {
            return Err(JoinError::Full);
        }

        let id = generate_guest_id(&inner.guests);
        let session = Arc::new(Session::new(
            id,
            Role::Guest,
            name,
            tx,
            self.config.max_message_bytes,
        ));
        let hello = handshake(&session, &inner.host_name, inner.control_mode);
        session.send_value(&hello);
        inner.guests.push(session.clone());
        room.touch();
        drop(inner);

        Ok(JoinInfo { room, session })
    }

    pub(crate) async fn remove_room(&self, code: &str, room: &Arc<Room>) {
        let mut rooms = self.rooms.lock().await;
        if rooms
            .get(code)
            .is_some_and(|current| Arc::ptr_eq(current, room))
        {
            rooms.remove(code);
        }
    }

    pub(crate) async fn room_count(&self) -> usize {
        self.rooms.lock().await.len()
    }

    pub fn spawn_reaper(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let state = Arc::clone(self);
        let interval_ms = (self.config.room_ttl.as_millis() / 2).clamp(250, 1_000) as u64;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                state.reap_expired().await;
            }
        })
    }

    async fn reap_expired(&self) {
        let ttl_ms = self.config.room_ttl.as_millis() as u64;
        let now = now_ms();
        let candidates: Vec<(String, Arc<Room>)> = {
            let rooms = self.rooms.lock().await;
            rooms
                .iter()
                .filter(|(_, room)| now.saturating_sub(room.last_activity_ms()) >= ttl_ms)
                .map(|(code, room)| (code.clone(), Arc::clone(room)))
                .collect()
        };

        for (code, room) in candidates {
            if let Some(sessions) = room.close_expired(now, ttl_ms).await {
                close_sessions(&sessions, "Room expired");
                self.remove_room(&code, &room).await;
            }
        }
    }
}

pub(crate) fn close_sessions(sessions: &[Arc<Session>], reason: &str) {
    let message = json!({ "type": "room_closed" });
    for session in sessions {
        session.send_value(&message);
        session.close(1000, reason);
    }
}

pub(crate) fn sanitize_name(raw: Option<&str>, fallback: &str) -> String {
    let filtered: String = raw
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let mut normalized = String::with_capacity(filtered.len());
    for word in filtered.split_whitespace() {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push_str(word);
    }
    let name: String = normalized.chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        fallback.to_owned()
    } else {
        name
    }
}

fn generate_room_code() -> String {
    let bytes = Uuid::new_v4();
    bytes
        .as_bytes()
        .iter()
        .take(ROOM_CODE_LEN)
        .map(|byte| ROOM_CODE_CHARS[*byte as usize % ROOM_CODE_CHARS.len()] as char)
        .collect()
}

fn generate_guest_id(existing: &[Arc<Session>]) -> String {
    loop {
        let uuid = Uuid::new_v4().simple().to_string();
        let id = format!("p-{}", &uuid[..8]);
        if existing.iter().all(|session| session.id != id) {
            return id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_name, ControlMode};

    #[test]
    fn names_are_normalized_and_bounded() {
        assert_eq!(sanitize_name(None, "Guest"), "Guest");
        assert_eq!(sanitize_name(Some("   "), "Guest"), "Guest");
        assert_eq!(sanitize_name(Some("  ana \t lú  "), "Guest"), "ana lú");
        assert_eq!(
            sanitize_name(Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), "Guest").len(),
            32
        );
        assert!(!sanitize_name(Some("a\u{0}b"), "Guest").contains('\u{0}'));
    }

    #[test]
    fn unknown_control_mode_falls_back_to_host_only() {
        assert_eq!(
            ControlMode::from_wire("shared_control"),
            ControlMode::SharedControl
        );
        assert_eq!(ControlMode::from_wire("anything"), ControlMode::HostOnly);
        assert_eq!(ControlMode::from_query(None), ControlMode::HostOnly);
    }
}
