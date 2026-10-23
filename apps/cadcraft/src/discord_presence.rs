//! Optional local Discord RPC. One worker, nonblocking IPC, bounded frames/queues/timeouts.
//! This module receives preferences only; no drawing data crosses the boundary.

mod transport;

use cadcraft_ui_egui::{
    CadApp,
    discord::{Settings, Status},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_FRAME: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_secs(15);
const HEARTBEAT: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(100);

struct Shared {
    settings: Settings,
    status: Status,
    stop: bool,
}

pub struct Presence {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl Presence {
    pub fn start(app: &mut CadApp, ctx: egui::Context) -> Self {
        app.discord.available = cfg!(any(windows, unix));
        let shared = Arc::new((Mutex::new(Shared { settings: app.discord.settings.clone(), status: Status::Off, stop: false }), Condvar::new()));
        let input = Arc::clone(&shared);
        let worker = match std::thread::Builder::new().name("discord-presence".into()).spawn(move || run(input, ctx)) {
            Ok(worker) => Some(worker),
            Err(error) => {
                log::warn!("Discord presence worker could not start: {error}");
                if let Ok(mut state) = shared.0.lock() {
                    state.status = Status::Error;
                }
                None
            }
        };
        Self { shared, worker }
    }

    pub fn poll(&mut self, app: &mut CadApp) {
        if let Ok(mut shared) = self.shared.0.lock() {
            if shared.settings != app.discord.settings {
                shared.settings.clone_from(&app.discord.settings);
                self.shared.1.notify_one();
            }
            app.discord.status = shared.status;
        }
    }

    pub fn shutdown(&mut self) {
        if let Ok(mut shared) = self.shared.0.lock() {
            shared.stop = true;
            self.shared.1.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn status(shared: &(Mutex<Shared>, Condvar), ctx: &egui::Context, next: Status) {
    if let Ok(mut state) = shared.0.lock()
        && state.status != next
    {
        state.status = next;
        drop(state);
        ctx.request_repaint();
    }
}

fn run(shared: Arc<(Mutex<Shared>, Condvar)>, ctx: egui::Context) {
    let started = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut current = None;
    let mut connection: Option<Connection<transport::Stream>> = None;
    let mut retry_at = Instant::now();
    loop {
        let (settings, stop) = match shared.0.lock() {
            Ok(state) => (state.settings.clone(), state.stop),
            Err(_) => break,
        };
        if stop || current.as_ref() != Some(&settings) {
            if let Some(mut previous) = connection.take() {
                previous.clear();
            }
            retry_at = Instant::now();
            current = Some(settings.clone());
        }
        if stop {
            break;
        }
        if !settings.enabled {
            status(&shared, &ctx, Status::Off);
        } else if settings.validate().is_err() {
            status(&shared, &ctx, Status::Rejected);
        } else if !cfg!(any(windows, unix)) {
            status(&shared, &ctx, Status::Unavailable);
        } else {
            let now = Instant::now();
            if connection.is_none() && now >= retry_at {
                retry_at = now + RETRY;
                match transport::connect().and_then(|stream| Connection::new(stream, &settings.application_id, started, now)) {
                    Ok(client) => {
                        connection = Some(client);
                        status(&shared, &ctx, Status::Connecting);
                    }
                    Err(_) => status(&shared, &ctx, Status::Waiting),
                }
            }
            if let Some(client) = connection.as_mut() {
                match client.tick(now) {
                    Ok(true) => status(&shared, &ctx, Status::Connected),
                    Ok(false) => {}
                    Err(error) => {
                        connection = None;
                        retry_at = now + RETRY;
                        status(&shared, &ctx, if error.kind() == io::ErrorKind::PermissionDenied { Status::Rejected } else { Status::Waiting });
                    }
                }
            }
        }
        let wait = if connection.is_some() {
            TICK
        } else if !settings.enabled || settings.validate().is_err() {
            RETRY
        } else {
            retry_at.saturating_duration_since(Instant::now()).max(TICK).min(RETRY)
        };
        let Ok(state) = shared.0.lock() else { break };
        if state.stop || current.as_ref() != Some(&state.settings) {
            continue;
        }
        if shared.1.wait_timeout(state, wait).is_err() {
            break;
        }
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn frame(opcode: u32, payload: &[u8]) -> io::Result<Vec<u8>> {
    if payload.len() > MAX_FRAME {
        return Err(invalid("Discord frame exceeds limit"));
    }
    let len = u32::try_from(payload.len()).map_err(|_| invalid("Discord frame length overflow"))?;
    let mut bytes = Vec::with_capacity(8 + payload.len());
    bytes.extend_from_slice(&opcode.to_le_bytes());
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn activity(started: u64, clear: bool) -> Value {
    let activity =
        if clear { Value::Null } else { json!({"details":"Designing in CADCraft", "state":"Drafting and modeling", "timestamps":{"start":started}}) };
    json!({"cmd":"SET_ACTIVITY", "args":{"pid":std::process::id(), "activity":activity}, "nonce":if clear {"cadcraft-clear"} else {"cadcraft-activity"}})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Ready,
    Activity,
    Active,
}

struct Connection<S> {
    stream: S,
    incoming: Vec<u8>,
    outgoing: VecDeque<Vec<u8>>,
    written: usize,
    phase: Phase,
    deadline: Option<Instant>,
    next_ping: Instant,
    started: u64,
}

impl<S: Read + Write> Connection<S> {
    fn new(stream: S, application_id: &str, started: u64, now: Instant) -> io::Result<Self> {
        let mut client = Self {
            stream,
            incoming: Vec::new(),
            outgoing: VecDeque::new(),
            written: 0,
            phase: Phase::Ready,
            deadline: Some(now + TIMEOUT),
            next_ping: now + HEARTBEAT,
            started,
        };
        client.queue_json(0, &json!({"v":1,"client_id":application_id}))?;
        Ok(client)
    }

    fn queue_json(&mut self, opcode: u32, value: &Value) -> io::Result<()> {
        let payload = serde_json::to_vec(value).map_err(|_| invalid("Discord JSON encoding failed"))?;
        self.queue(opcode, &payload)
    }

    fn queue(&mut self, opcode: u32, bytes: &[u8]) -> io::Result<()> {
        if self.outgoing.len() >= 4 {
            return Err(invalid("Discord output queue exceeds limit"));
        }
        self.outgoing.push_back(frame(opcode, bytes)?);
        Ok(())
    }

    fn write_pending(&mut self) -> io::Result<()> {
        for _ in 0..4 {
            let Some(bytes) = self.outgoing.front() else { break };
            let pending = bytes.get(self.written..).ok_or_else(|| invalid("Discord write offset"))?;
            match self.stream.write(pending) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) if n <= pending.len() => {
                    self.written += n;
                    if self.written == bytes.len() {
                        self.outgoing.pop_front();
                        self.written = 0;
                    }
                }
                Ok(_) => return Err(invalid("Invalid Discord write result")),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn tick(&mut self, now: Instant) -> io::Result<bool> {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        if self.phase == Phase::Active && self.deadline.is_none() && now >= self.next_ping {
            self.queue(3, b"{}")?;
            self.deadline = Some(now + TIMEOUT);
        }
        self.write_pending()?;
        for _ in 0..8 {
            if !self.receive(now)? {
                break;
            }
        }
        let mut buffer = [0u8; 4096];
        for _ in 0..16 {
            let room = (MAX_FRAME + 8).saturating_sub(self.incoming.len()).min(buffer.len());
            match self.stream.read(buffer.get_mut(..room).ok_or_else(|| invalid("Discord input capacity"))?) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) if n <= room => self.incoming.extend_from_slice(buffer.get(..n).ok_or_else(|| invalid("Invalid Discord read result"))?),
                Ok(_) => return Err(invalid("Invalid Discord read result")),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
            for _ in 0..8 {
                if !self.receive(now)? {
                    break;
                }
            }
            if self.incoming.len() >= MAX_FRAME + 8 {
                return Err(invalid("Discord input exceeds limit"));
            }
        }
        self.write_pending()?;
        Ok(self.phase == Phase::Active)
    }

    fn receive(&mut self, now: Instant) -> io::Result<bool> {
        let Some(header) = self.incoming.get(..8) else { return Ok(false) };
        let opcode = u32::from_le_bytes(header.get(..4).and_then(|b| b.try_into().ok()).ok_or_else(|| invalid("Discord header"))?);
        let len = u32::from_le_bytes(header.get(4..8).and_then(|b| b.try_into().ok()).ok_or_else(|| invalid("Discord length"))?) as usize;
        if len > MAX_FRAME {
            return Err(invalid("Discord frame exceeds limit"));
        }
        let end = 8 + len;
        let Some(payload) = self.incoming.get(8..end) else { return Ok(false) };
        match opcode {
            1 => {
                let value: Value = serde_json::from_slice(payload).map_err(|_| invalid("Malformed Discord JSON"))?;
                if value.get("evt").and_then(Value::as_str) == Some("ERROR") {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                if self.phase == Phase::Ready && value.get("evt").and_then(Value::as_str) == Some("READY") {
                    self.phase = Phase::Activity;
                    self.deadline = Some(now + TIMEOUT);
                    self.queue_json(1, &activity(self.started, false))?;
                } else if self.phase == Phase::Activity
                    && value.get("cmd").and_then(Value::as_str) == Some("SET_ACTIVITY")
                    && value.get("nonce").and_then(Value::as_str) == Some("cadcraft-activity")
                {
                    self.phase = Phase::Active;
                    self.deadline = None;
                    self.next_ping = now + HEARTBEAT;
                }
            }
            2 => {
                // Invalid application IDs may close IPC during the handshake rather than
                // sending an ERROR event. Distinguish those from Discord simply restarting.
                let rejected = serde_json::from_slice::<Value>(payload)
                    .ok()
                    .and_then(|v| v.get("code").and_then(Value::as_u64))
                    .is_some_and(|code| (4000..5000).contains(&code));
                return Err(if rejected { io::ErrorKind::PermissionDenied } else { io::ErrorKind::ConnectionAborted }.into());
            }
            3 => {
                let echo = payload.to_vec();
                self.queue(4, &echo)?;
            }
            4 if self.phase == Phase::Active => {
                self.deadline = None;
                self.next_ping = now + HEARTBEAT;
            }
            _ => return Err(invalid("Unknown Discord opcode")),
        }
        self.incoming.drain(..end);
        Ok(true)
    }

    fn clear(&mut self) {
        // Complete any partial frame first. Closing IPC also removes activity. Never perform
        // a synchronous Windows pipe flush; a hung peer must not hold up the UI or shutdown.
        if self.queue_json(1, &activity(self.started, true)).is_err() {
            return;
        }
        let deadline = Instant::now() + Duration::from_millis(200);
        while !self.outgoing.is_empty() && Instant::now() < deadline {
            if self.write_pending().is_err() {
                break;
            }
            if !self.outgoing.is_empty() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        input: VecDeque<u8>,
        output: Vec<u8>,
        chunk: Option<usize>,
        block_writes: bool,
        eof: bool,
    }
    impl Read for Fake {
        fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
            if self.input.is_empty() {
                return if self.eof { Ok(0) } else { Err(io::ErrorKind::WouldBlock.into()) };
            }
            let count = target.len().min(self.chunk.unwrap_or(usize::MAX)).min(self.input.len());
            for byte in target.iter_mut().take(count) {
                *byte = self.input.pop_front().unwrap();
            }
            Ok(count)
        }
    }
    impl Write for Fake {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.block_writes {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let count = bytes.len().min(self.chunk.unwrap_or(usize::MAX));
            self.output.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("The IPC worker must never synchronously flush");
        }
    }
    impl Fake {
        fn reply(&mut self, value: Value) {
            self.input.extend(frame(1, &serde_json::to_vec(&value).unwrap()).unwrap());
        }
        fn sent(&mut self) -> Vec<(u32, Value)> {
            let bytes = std::mem::take(&mut self.output);
            let mut rest = bytes.as_slice();
            let mut frames = Vec::new();
            while !rest.is_empty() {
                let opcode = u32::from_le_bytes(rest[..4].try_into().unwrap());
                let length = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
                frames.push((opcode, serde_json::from_slice(&rest[8..8 + length]).unwrap()));
                rest = &rest[8 + length..];
            }
            frames
        }
    }

    fn active() -> (Connection<Fake>, Instant) {
        let now = Instant::now();
        let mut connection = Connection::new(Fake::default(), cadcraft_ui_egui::discord::APPLICATION_ID, 1234, now).unwrap();
        connection.tick(now).unwrap();
        let handshake = connection.stream.sent();
        assert_eq!(handshake, vec![(0, json!({"v":1,"client_id":cadcraft_ui_egui::discord::APPLICATION_ID}))]);
        connection.stream.reply(json!({"cmd":"DISPATCH","evt":"READY","data":{"user":{"username":"private"}}}));
        assert!(!connection.tick(now).unwrap());
        let payload = connection.stream.sent();
        assert_eq!(payload, vec![(1, activity(1234, false))]);
        assert_eq!(
            payload[0].1["args"]["activity"],
            json!({"details":"Designing in CADCraft","state":"Drafting and modeling","timestamps":{"start":1234}})
        );
        connection.stream.reply(json!({"cmd":"SET_ACTIVITY","nonce":"cadcraft-activity","data":{}}));
        assert!(connection.tick(now).unwrap());
        (connection, now)
    }

    #[test]
    fn acknowledged_activity_heartbeats_and_clear_use_only_public_app_data() {
        let (mut client, now) = active();
        client.tick(now + HEARTBEAT).unwrap();
        assert_eq!(client.stream.sent(), vec![(3, json!({}))]);
        client.stream.input.extend(frame(4, b"{}").unwrap());
        client.tick(now + HEARTBEAT + Duration::from_secs(1)).unwrap();
        assert!(client.deadline.is_none());
        client.stream.input.extend(frame(3, b"{\"ping\":1}").unwrap());
        client.tick(now + HEARTBEAT + Duration::from_secs(2)).unwrap();
        assert_eq!(client.stream.sent(), vec![(4, json!({"ping":1}))]);
        client.clear();
        assert_eq!(client.stream.sent(), vec![(1, activity(1234, true))]);
    }

    #[test]
    fn fragmented_reads_and_writes_preserve_framing() {
        let now = Instant::now();
        let mut client = Connection::new(Fake { chunk: Some(1), ..Default::default() }, "1562973280875315221", 42, now).unwrap();
        client.stream.reply(json!({"evt":"READY"}));
        for _ in 0..200 {
            client.tick(now).unwrap();
        }
        assert_eq!(client.stream.sent(), vec![(0, json!({"v":1,"client_id":"1562973280875315221"})), (1, activity(42, false))]);
        client.stream.reply(json!({"cmd":"SET_ACTIVITY","nonce":"cadcraft-activity"}));
        for _ in 0..40 {
            client.tick(now).unwrap();
        }
        assert_eq!(client.phase, Phase::Active);
    }

    #[test]
    fn hostile_peers_cannot_allocate_without_limit_or_stall_the_connection() {
        let (mut client, now) = active();
        client.stream.input.extend([1, 0, 0, 0, 255, 255, 255, 255]);
        assert_eq!(client.tick(now).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(client.incoming.len() <= MAX_FRAME + 8);
        for (opcode, payload, kind) in [
            (1, b"not json".as_slice(), io::ErrorKind::InvalidData),
            (99, b"{}", io::ErrorKind::InvalidData),
            (2, b"{}", io::ErrorKind::ConnectionAborted),
            (2, b"{\"code\":4000}", io::ErrorKind::PermissionDenied),
            (1, b"{\"evt\":\"ERROR\"}", io::ErrorKind::PermissionDenied),
        ] {
            let (mut client, now) = active();
            client.stream.input.extend(frame(opcode, payload).unwrap());
            assert_eq!(client.tick(now).unwrap_err().kind(), kind);
        }
        let (mut client, now) = active();
        client.stream.eof = true;
        assert_eq!(client.tick(now).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
        let now = Instant::now();
        let mut silent = Connection::new(Fake { block_writes: true, ..Default::default() }, "1562973280875315221", 0, now).unwrap();
        assert_eq!(silent.tick(now + TIMEOUT).unwrap_err().kind(), io::ErrorKind::TimedOut);
        let before = Instant::now();
        silent.clear();
        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(frame(1, &vec![0; MAX_FRAME + 1]).is_err());
    }

    #[test]
    fn disabling_and_shutdown_wake_a_quiet_worker() {
        let mut app = CadApp::new(cadcraft_engine::Session::new(), cadcraft_ui_egui::Services::default());
        app.discord.settings.enabled = false;
        let mut presence = Presence::start(&mut app, egui::Context::default());
        let before = Instant::now();
        presence.shutdown();
        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(presence.worker.is_none());
        presence.shutdown();
    }
}
