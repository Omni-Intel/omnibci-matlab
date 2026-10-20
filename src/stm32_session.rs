//! Synchronous STM32/E73 owner. The SDK alone owns wire transactions and radio framing.
//!
//! This is the MATLAB-binding counterpart of the Python `stm32_session` module. It
//! wraps the SDK's low-level `stm32::spawn_into` / `e73_ble::spawn_into` transports
//! with blocking control methods and a bounded sample queue, without reimplementing
//! any V3 protocol or E73 BLE framing.
use crossbeam_channel::{SendTimeoutError, Sender, bounded};
use omnibci_sdk::{
    session::{Error, Samples, State},
    stm32::{Configuration, Parser},
    transport::{Command, DeliveryError, DeviceInfo, DeviceMetadata, Event, EventSink},
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Error>;
const SAMPLE_CAPACITY: usize = 128;
const TRIGGER_CAPACITY: usize = 16_384;

#[derive(Clone)]
enum Action {
    Start,
    Stop,
    Configure(Configuration),
}

struct Core {
    state: State,
    config: Option<Configuration>,
    metadata: Option<DeviceMetadata>,
    error: Option<Error>,
    cleanup_stop_error: Option<Error>,
    pending: Option<Action>,
    control_deadline: Option<Instant>,
    connected: bool,
    generation: u64,
    samples: u64,
    missing: u64,
    crc: u64,
    parser: Parser,
    previous: Option<u32>,
    index: u64,
    queue: VecDeque<Samples>,
    triggers: VecDeque<omnibci_sdk::protocol::EegTrigger>,
    disconnected: bool,
    closing: bool,
    loff_may_be_on: bool,
}

impl Default for Core {
    fn default() -> Self {
        Self {
            state: State::Connecting,
            config: None,
            metadata: None,
            error: None,
            cleanup_stop_error: None,
            pending: None,
            control_deadline: None,
            connected: false,
            generation: 0,
            samples: 0,
            missing: 0,
            crc: 0,
            parser: Parser::default(),
            previous: None,
            index: 0,
            queue: VecDeque::new(),
            triggers: VecDeque::new(),
            disconnected: false,
            closing: false,
            loff_may_be_on: false,
        }
    }
}

impl Core {
    fn fault(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
        self.state = State::Faulted;
        self.pending = None;
        self.control_deadline = None;
    }
    /// Keep the first distinct cleanup report for close diagnostics. The
    /// primary fault remains in `error`; an identical report is not repeated.
    fn record_cleanup(&mut self, error: Error) {
        if self.cleanup_stop_error.is_none() && self.error.as_ref() != Some(&error) {
            self.cleanup_stop_error = Some(error);
        }
    }
    fn begin(&mut self, action: Action) -> Result<Command> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.pending.is_some() {
            return Err(Error::InvalidState);
        }
        let command = match &action {
            Action::Start if self.state == State::Ready && self.config.is_some() => {
                self.state = State::Starting;
                Command::Write(b"b".to_vec())
            }
            Action::Stop if matches!(self.state, State::Ready | State::Streaming) => {
                self.state = State::Stopping;
                Command::StopAndDrain
            }
            Action::Configure(config) if self.state == State::Ready => {
                config.validate().map_err(Error::InvalidConfig)?;
                self.loff_may_be_on |= config.loff != 0;
                Command::ConfigureStm32(config.clone())
            }
            _ => return Err(Error::InvalidState),
        };
        self.pending = Some(action);
        Ok(command)
    }
    fn event(&mut self, event: Event) -> Result<()> {
        if self
            .control_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
            && (self.pending.is_some() || self.state == State::Connecting)
        {
            self.fault(Error::Timeout);
        }
        // A timeout invalidates the control generation. No late ACK can revive it.
        if self.error.is_some() {
            match event {
                Event::Disconnected => self.disconnected = true,
                // Cleanup diagnostics survive an earlier fault without changing
                // state or masking the primary error.
                Event::Stm32Stopped(Err(error)) if self.closing => {
                    self.record_cleanup(Error::Transport(error));
                }
                Event::Error(message) if self.closing => {
                    self.record_cleanup(Error::Transport(message));
                }
                _ => {}
            }
            return Ok(());
        }
        match event {
            Event::Connected(_) => {
                self.connected = true;
            }
            Event::DeviceMetadata(metadata) => self.metadata = Some(metadata),
            Event::Stm32Configured(result) => {
                let config = result.map_err(Error::InvalidConfig)?;
                config.validate().map_err(Error::InvalidConfig)?;
                match &self.pending {
                    Some(Action::Configure(expected)) if expected == &config => self.pending = None,
                    Some(_) => {
                        return Err(Error::InvalidConfig(
                            "STM32 configuration ACK does not match request".into(),
                        ));
                    }
                    None if self.state == State::Connecting
                        && config == Configuration::default() => {}
                    _ => {
                        return Err(Error::InvalidConfig(
                            "unexpected STM32 configuration ACK".into(),
                        ));
                    }
                }
                self.loff_may_be_on = config.loff != 0;
                self.config = Some(config);
            }
            Event::Stm32Started(result) if matches!(self.pending, Some(Action::Start)) => {
                result.map_err(Error::Transport)?;
                self.queue.clear();
                self.triggers.clear();
                self.parser = Parser::default();
                self.previous = None;
                self.index = 0;
                self.samples = 0;
                self.missing = 0;
                self.generation += 1;
                self.state = State::Streaming;
                self.pending = None;
            }
            Event::Stm32Stopped(result) if matches!(self.pending, Some(Action::Stop)) => {
                result.map_err(Error::Transport)?;
                self.state = State::Ready;
                self.pending = None;
            }
            // A failed Disconnect stop leaves the latest SDK driver running until
            // Shutdown. Its best-effort cleanup report must not vanish during close.
            Event::Stm32Stopped(Err(error)) if self.pending.is_none() && self.closing => {
                self.record_cleanup(Error::Transport(error));
            }
            Event::Bytes(bytes)
                if matches!(
                    self.state,
                    State::Streaming | State::Stopping | State::Starting
                ) =>
            {
                let config = self.config.as_ref().ok_or(Error::InvalidState)?;
                let frames = self.parser.feed(&bytes, &config.gains);
                self.parser.data_packets.clear();
                let events = std::mem::take(&mut self.parser.events);
                let trigger_overflow = self.triggers.len() + events.len() > TRIGGER_CAPACITY;
                let remaining = TRIGGER_CAPACITY.saturating_sub(self.triggers.len());
                self.triggers.extend(events.into_iter().take(remaining));
                if !frames.is_empty() {
                    if self.queue.len() == SAMPLE_CAPACITY {
                        return Err(Error::Overflow);
                    }
                    let mut indices = Vec::with_capacity(frames.len());
                    for frame in &frames {
                        let gap =
                            omnibci_sdk::protocol::sequence_gap_size(self.previous, frame.sequence)
                                as u64;
                        self.missing += gap;
                        self.index += gap;
                        indices.push(self.index);
                        self.index += 1;
                        self.previous = Some(frame.sequence);
                    }
                    self.samples += frames.len() as u64;
                    self.queue.push_back(Samples {
                        generation: self.generation,
                        frames,
                        sample_indices: indices,
                        received_at: Instant::now(),
                        sample_rate_hz: 250,
                    });
                }
                if trigger_overflow {
                    return Err(Error::Overflow);
                }
            }
            Event::Stm32WireMetrics { crc_bad, .. } => self.crc = crc_bad,
            Event::Error(message) => return Err(Error::Transport(message)),
            Event::Disconnected => {
                self.disconnected = true;
                if self.closing {
                    self.state = State::Closed;
                } else {
                    self.fault(Error::Disconnected);
                }
            }
            _ => {}
        }
        if self.state == State::Connecting && self.connected && self.config.is_some() {
            self.state = State::Ready;
        }
        if self.pending.is_none() && self.state != State::Connecting {
            self.control_deadline = None;
        }
        Ok(())
    }
}

struct Shared {
    core: Mutex<Core>,
    changed: Condvar,
}

/// Snapshot of the synchronous owner's cached state.
pub struct Snapshot {
    pub state: State,
    pub config: Option<Configuration>,
    pub metadata: Option<DeviceMetadata>,
    pub generation: u64,
    pub samples: u64,
    pub missing_samples: u64,
    pub crc_errors: u64,
    pub error: Option<Error>,
}

pub struct Device {
    shared: Arc<Shared>,
    commands: Sender<Command>,
    shutdown: Sender<()>,
    worker: Option<JoinHandle<()>>,
    driver: Option<JoinHandle<()>>,
}

impl Device {
    /// `kind` is `serial` or `ble`; BLE addresses are E73 keys from `discover`.
    pub fn connect(kind: &str, address: &str, timeout: Duration) -> Result<Self> {
        Self::connect_with(
            timeout,
            |commands, sink| {
                if kind == "serial" {
                    omnibci_sdk::stm32::spawn_into(
                        address.into(),
                        Configuration::default(),
                        commands,
                        sink,
                    )
                } else {
                    omnibci_sdk::e73_ble::spawn_into(Configuration::default(), commands, sink)
                }
            },
            (kind == "ble").then(|| address.to_string()),
        )
    }

    fn connect_with(
        timeout: Duration,
        spawn: impl FnOnce(crossbeam_channel::Receiver<Command>, EventSink) -> JoinHandle<()>,
        key: Option<String>,
    ) -> Result<Self> {
        let deadline = Instant::now() + timeout;
        let (commands, command_rx) = bounded(32);
        let (events, event_rx) = bounded(1024);
        let overflow = Arc::new(AtomicBool::new(false));
        let flag = overflow.clone();
        let sink = EventSink::new(move |event| {
            events.try_send(event).map_err(|e| match e {
                crossbeam_channel::TrySendError::Full(_) => {
                    flag.store(true, Ordering::Release);
                    DeliveryError::Full
                }
                _ => DeliveryError::Closed,
            })
        });
        let driver = spawn(command_rx, sink);
        let shared = Arc::new(Shared {
            core: Mutex::new(Core {
                control_deadline: Some(deadline),
                ..Default::default()
            }),
            changed: Condvar::new(),
        });
        let state = shared.clone();
        let (shutdown, quit) = bounded(1);
        let control = commands.clone();
        let worker = thread::spawn(move || {
            loop {
                if quit.try_recv().is_ok() {
                    break;
                }
                let event = event_rx.recv_timeout(Duration::from_millis(10));
                let mut core = state.core.lock().unwrap();
                if overflow.load(Ordering::Acquire) {
                    core.fault(Error::Overflow);
                }
                match event {
                    Ok(event) => {
                        if let Err(error) = core.event(event) {
                            core.fault(error);
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        core.disconnected = true;
                        core.fault(Error::Disconnected);
                    }
                    _ => {}
                }
                // Retry fault cleanup without filling the bounded queue: keep at
                // most one request pending and add none while close owns cleanup.
                if core.error.is_some() && !core.closing && control.is_empty() {
                    let _ = control.try_send(Command::Disconnect);
                }
                state.changed.notify_all();
                if core.disconnected {
                    break;
                }
            }
        });
        let mut device = Self {
            shared,
            commands,
            shutdown,
            worker: Some(worker),
            driver: Some(driver),
        };
        if let Some(key) = key {
            device.send(Command::Scan)?;
            device.send(Command::Connect(key))?;
        }
        if let Err(error) = device.wait(deadline, |core| core.state == State::Ready) {
            let _ = device.close(Duration::from_millis(100));
            return Err(error);
        }
        Ok(device)
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands.try_send(command).map_err(|e| match e {
            crossbeam_channel::TrySendError::Full(_) => Error::Overflow,
            _ => Error::Disconnected,
        })
    }

    /// Control commands used by cleanup must not be dropped when the bounded
    /// queue is momentarily full. Always try to enqueue immediately; only
    /// waiting for a full queue consumes the caller's budget.
    fn send_before(&self, command: Command, deadline: Instant) -> Result<()> {
        match self.commands.try_send(command) {
            Ok(()) => Ok(()),
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => Err(Error::Disconnected),
            Err(crossbeam_channel::TrySendError::Full(command)) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Error::Timeout);
                }
                self.commands
                    .send_timeout(command, remaining)
                    .map_err(|error| match error {
                        SendTimeoutError::Timeout(_) => Error::Timeout,
                        SendTimeoutError::Disconnected(_) => Error::Disconnected,
                    })
            }
        }
    }

    fn wait(&self, deadline: Instant, ready: impl Fn(&Core) -> bool) -> Result<()> {
        let mut core = self.shared.core.lock().map_err(|_| Error::WorkerPanicked)?;
        loop {
            if let Some(error) = &core.error {
                return Err(error.clone());
            }
            if ready(&core) {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                core.fault(Error::Timeout);
                self.shared.changed.notify_all();
                let _ = self.send(Command::Disconnect);
                return Err(Error::Timeout);
            }
            core = self
                .shared
                .changed
                .wait_timeout(core, remaining)
                .map_err(|_| Error::WorkerPanicked)?
                .0;
        }
    }

    fn request(&self, action: Action, timeout: Duration) -> Result<()> {
        if timeout.is_zero() {
            return Err(Error::Timeout);
        }
        let deadline = Instant::now() + timeout;
        let command = {
            let mut core = self.shared.core.lock().map_err(|_| Error::WorkerPanicked)?;
            let command = core.begin(action)?;
            core.control_deadline = Some(deadline);
            command
        };
        if let Err(error) = self.send(command) {
            self.shared.core.lock().unwrap().fault(error.clone());
            return Err(error);
        }
        self.wait(deadline, |core| core.pending.is_none())
    }

    pub fn start(&mut self, timeout: Duration) -> Result<()> {
        self.request(Action::Start, timeout)
    }

    pub fn stop(&mut self, timeout: Duration) -> Result<()> {
        self.request(Action::Stop, timeout)
    }

    pub fn configure(&mut self, config: Configuration, timeout: Duration) -> Result<()> {
        config.validate().map_err(Error::InvalidConfig)?;
        self.request(Action::Configure(config), timeout)
    }

    /// Drain available hardware events independently of sample reads. Call regularly.
    /// Retain the received prefix on a fault, then report the fault on the next call.
    pub fn read_triggers(&self) -> Result<(u64, Vec<omnibci_sdk::protocol::EegTrigger>)> {
        let mut core = self.shared.core.lock().map_err(|_| Error::WorkerPanicked)?;
        if core.triggers.is_empty()
            && let Some(error) = &core.error
        {
            return Err(error.clone());
        }
        let generation = core.generation;
        Ok((generation, core.triggers.drain(..).collect()))
    }
    pub fn read(&self, timeout: Duration) -> Result<Samples> {
        let deadline = Instant::now() + timeout;
        let mut core = self.shared.core.lock().map_err(|_| Error::WorkerPanicked)?;
        loop {
            // Deliver the retained prefix before reporting an asynchronous failure.
            if let Some(batch) = core.queue.pop_front() {
                return Ok(batch);
            }
            if let Some(error) = &core.error {
                return Err(error.clone());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Timeout);
            }
            core = self
                .shared
                .changed
                .wait_timeout(core, remaining)
                .map_err(|_| Error::WorkerPanicked)?
                .0;
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let core = self.shared.core.lock().unwrap();
        Snapshot {
            state: core.state,
            config: core.config.clone(),
            metadata: core.metadata.clone(),
            generation: core.generation,
            samples: core.samples,
            missing_samples: core.missing,
            crc_errors: core.crc,
            error: core.error.clone(),
        }
    }

    pub fn close(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut result = Ok(());
        let mut cleanup_errors = Vec::new();
        let (state, config, original) = {
            let core = self.shared.core.lock().map_err(|_| Error::WorkerPanicked)?;
            (core.state, core.config.clone(), core.error.clone())
        };
        if original.is_none() && matches!(state, State::Streaming | State::Ready) {
            result = self.stop(deadline.saturating_duration_since(Instant::now()));
            if result.is_ok()
                && let Some(mut config) = config
                && config.loff != 0
            {
                config.loff = 0;
                result = self.configure(config, deadline.saturating_duration_since(Instant::now()));
            }
        }
        self.shared
            .core
            .lock()
            .map_err(|_| Error::WorkerPanicked)?
            .closing = true;
        match self.send_before(Command::Disconnect, deadline) {
            Ok(()) | Err(Error::Disconnected) => {}
            Err(error) => cleanup_errors.push(error),
        }
        // The latest SDK keeps the driver alive when Disconnect's stop fails.
        // Allow the normal stop-and-exit path a moment, then request Shutdown,
        // whose wrapper performs a best-effort stop before the thread exits.
        // With no remaining budget this waits zero time; send_before still
        // attempts the enqueue immediately when queue capacity is available.
        let escalate = (Instant::now() + Duration::from_millis(100)).min(deadline);
        while self.driver.as_ref().is_some_and(|h| !h.is_finished()) && Instant::now() < escalate {
            thread::sleep(Duration::from_millis(2));
        }
        if self.driver.as_ref().is_some_and(|h| !h.is_finished()) {
            match self.send_before(Command::Shutdown, deadline) {
                Ok(()) | Err(Error::Disconnected) => {}
                Err(error) if !cleanup_errors.contains(&error) => cleanup_errors.push(error),
                Err(_) => {}
            }
        }
        // OS drivers cannot be forcibly cancelled; detach on deadline, never join a live worker.
        while self.driver.as_ref().is_some_and(|h| !h.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        if let Some(driver) = self.driver.take() {
            if driver.is_finished() {
                if driver.join().is_err() {
                    cleanup_errors.push(Error::WorkerPanicked);
                }
            } else {
                cleanup_errors.push(Error::Timeout);
            }
        }
        // The driver finishing does not mean its final error events were consumed.
        while !self
            .shared
            .core
            .lock()
            .map_err(|_| Error::WorkerPanicked)?
            .disconnected
            && self.worker.as_ref().is_some_and(|h| !h.is_finished())
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
        // A full slot means quit was already queued; Disconnected means the
        // worker has exited. Neither is a cleanup failure.
        let _ = self.shutdown.try_send(());
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
            if worker.is_finished() {
                if worker.join().is_err() {
                    cleanup_errors.push(Error::WorkerPanicked);
                }
            } else {
                cleanup_errors.push(Error::Timeout);
            }
        }
        let mut core = self.shared.core.lock().map_err(|_| Error::WorkerPanicked)?;
        if original.is_none()
            && let Some(error) = &core.error
        {
            result = match result {
                Ok(()) => Err(error.clone()),
                Err(cleanup) if &cleanup == error => Err(cleanup),
                Err(cleanup) => Err(Error::Transport(format!("{error}; cleanup: {cleanup}"))),
            };
        }
        for cleanup in cleanup_errors {
            result = match result {
                Ok(()) => Err(cleanup),
                Err(primary) => Err(Error::Transport(format!("{primary}; cleanup: {cleanup}"))),
            };
        }
        let cleanup_stop = core.cleanup_stop_error.take();
        if let Some(error) = cleanup_stop
            && core.error.as_ref() != Some(&error)
        {
            result = match result {
                Ok(()) => Err(error),
                Err(primary) => Err(Error::Transport(format!("{primary}; cleanup: {error}"))),
            };
        }
        if core.loff_may_be_on {
            let detail = "LOFF excitation off could not be confirmed after session failure";
            result = Err(Error::Transport(match result {
                Ok(()) => detail.into(),
                Err(error) => format!("{error}; {detail}"),
            }));
        }
        if let Err(error) = &result {
            core.fault(error.clone());
        }
        if original.is_none() && result.is_ok() {
            core.state = State::Closed;
        }
        if let Some(error) = original {
            return match result {
                Ok(()) => Err(error),
                Err(cleanup) => Err(Error::Transport(format!("{error}; cleanup: {cleanup}"))),
            };
        }
        result
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.close(Duration::from_secs(6));
        }
    }
}

/// Scan for E73 BLE.1 devices using the SDK's transport command/event contract.
pub fn discover(timeout: Duration) -> Result<Vec<DeviceInfo>> {
    let deadline = Instant::now() + timeout;
    let (command, commands) = bounded::<Command>(8);
    let (events, receiver) = bounded(64);
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = overflow.clone();
    let sink = EventSink::new(move |event| {
        events.try_send(event).map_err(|_| {
            flag.store(true, Ordering::Release);
            DeliveryError::Full
        })
    });
    let worker = omnibci_sdk::e73_ble::spawn_into(Configuration::default(), commands, sink);
    command
        .try_send(Command::Scan)
        .map_err(|_| Error::Disconnected)?;
    let result = loop {
        if overflow.load(Ordering::Acquire) {
            break Err(Error::Overflow);
        }
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Devices(devices)) => break Ok(devices),
            Ok(Event::Error(error)) => break Err(Error::Transport(error)),
            Ok(Event::Disconnected) => break Err(Error::Disconnected),
            Err(_) => break Err(Error::Timeout),
            _ => {}
        }
    };
    let _ = command.try_send(Command::Shutdown);
    // Scan cancellation is owned by the SDK; never exceed the caller's deadline to join it.
    if worker.is_finished() {
        worker.join().map_err(|_| Error::WorkerPanicked)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispose_releases_registry_entry_after_permanent_fault() {
        let (commands, _receiver) = bounded(4);
        let (shutdown, _quit) = bounded(1);
        let mut core = Core::default();
        core.fault(Error::Transport("permanent device fault".into()));
        let device = Device {
            shared: Arc::new(Shared {
                core: Mutex::new(core),
                changed: Condvar::new(),
            }),
            commands,
            shutdown,
            worker: None,
            driver: None,
        };
        let handle = {
            let mut registry = crate::registry().lock().unwrap();
            registry.next += 1;
            let handle = registry.next;
            registry
                .devices
                .insert(handle, crate::Backend::Stm32(device));
            handle
        };
        let request = serde_json::from_value(serde_json::json!({
            "command": "dispose", "handle": handle
        }))
        .unwrap();
        let error = crate::execute(request).unwrap_err();
        assert!(error.contains("permanent device fault"));
        assert!(
            !crate::registry()
                .lock()
                .unwrap()
                .devices
                .contains_key(&handle)
        );
    }
    use std::sync::mpsc;

    #[test]
    fn send_before_attempts_enqueue_before_rejecting_expired_deadline() {
        let shared = Arc::new(Shared {
            core: Mutex::new(Core::default()),
            changed: Condvar::new(),
        });
        let (commands, receiver) = bounded::<Command>(1);
        let (shutdown, _quit) = bounded::<()>(1);
        let device = Device {
            shared,
            commands,
            shutdown,
            worker: None,
            driver: None,
        };
        let expired = Instant::now() - Duration::from_secs(1);
        // Capacity is available: the nonblocking attempt succeeds even though
        // the caller's deadline has already passed.
        assert!(
            device
                .send_before(Command::QueryDiagnostics, expired)
                .is_ok()
        );
        assert!(matches!(receiver.try_recv(), Ok(Command::QueryDiagnostics)));
        // Only a full queue may consume the remaining (zero) budget.
        assert!(
            device
                .send_before(Command::Disconnect, Instant::now() + Duration::from_secs(1))
                .is_ok()
        );
        assert!(matches!(
            device.send_before(Command::Shutdown, expired),
            Err(Error::Timeout)
        ));
    }

    #[test]
    fn cleanup_reports_survive_primary_fault_and_preserve_first_distinct_error() {
        let mut core = Core::default();
        core.fault(Error::Transport("primary fault".into()));
        core.closing = true;
        core.event(Event::Stm32Stopped(Err("disconnect stop failed".into())))
            .unwrap();
        assert_eq!(core.error, Some(Error::Transport("primary fault".into())));
        assert_eq!(
            core.cleanup_stop_error,
            Some(Error::Transport("disconnect stop failed".into()))
        );
        core.event(Event::Error("later cleanup error".into()))
            .unwrap();
        assert_eq!(
            core.cleanup_stop_error,
            Some(Error::Transport("disconnect stop failed".into())),
            "the first distinct cleanup report is retained"
        );
        assert_eq!(core.error, Some(Error::Transport("primary fault".into())));
    }

    #[test]
    fn error_event_is_recorded_as_cleanup_during_close_after_fault() {
        let mut core = Core::default();
        core.fault(Error::Transport("primary fault".into()));
        core.closing = true;
        core.event(Event::Error("disconnect cleanup failed".into()))
            .unwrap();
        assert_eq!(core.error, Some(Error::Transport("primary fault".into())));
        assert_eq!(
            core.cleanup_stop_error,
            Some(Error::Transport("disconnect cleanup failed".into()))
        );
    }

    #[test]
    fn cleanup_report_equal_to_primary_fault_is_not_duplicated() {
        let mut core = Core::default();
        core.fault(Error::Transport("same failure".into()));
        core.closing = true;
        core.event(Event::Stm32Stopped(Err("same failure".into())))
            .unwrap();
        assert_eq!(core.cleanup_stop_error, None);
        core.event(Event::Error("same failure".into())).unwrap();
        assert_eq!(core.cleanup_stop_error, None);
    }

    /// Simulated SDK driver whose Disconnect keeps the driver alive so close
    /// must escalate to Shutdown.
    fn simulated_close(
        explicit_stop: std::result::Result<(), String>,
        disconnect_stop: std::result::Result<(), String>,
        shutdown_stop: std::result::Result<(), String>,
    ) -> (Device, mpsc::Receiver<()>) {
        let (shutdown, seen) = mpsc::channel();
        let device = Device::connect_with(
            Duration::from_secs(1),
            move |commands, events| {
                thread::spawn(move || {
                    events.send(Event::Connected("simulated".into())).unwrap();
                    events
                        .send(Event::Stm32Configured(Ok(Configuration::default())))
                        .unwrap();
                    while let Ok(command) = commands.recv() {
                        match command {
                            Command::StopAndDrain => {
                                let _ = events.send(Event::Stm32Stopped(explicit_stop.clone()));
                            }
                            Command::Disconnect => {
                                let _ = events.send(Event::Stm32Stopped(disconnect_stop.clone()));
                            }
                            Command::Shutdown => {
                                let _ = events.send(Event::Stm32Stopped(shutdown_stop.clone()));
                                let _ = events.send(Event::Disconnected);
                                let _ = shutdown.send(());
                                break;
                            }
                            _ => {}
                        }
                    }
                })
            },
            None,
        )
        .unwrap();
        (device, seen)
    }

    #[test]
    fn failed_disconnect_stop_escalates_to_shutdown() {
        let (mut device, seen) = simulated_close(
            Err("explicit stop failed".into()),
            Err("disconnect stop failed".into()),
            Err("shutdown cleanup failed".into()),
        );
        let error = device
            .close(Duration::from_secs(1))
            .unwrap_err()
            .to_string();
        seen.recv_timeout(Duration::from_secs(1))
            .expect("close must escalate from Disconnect to Shutdown");
        assert!(error.contains("explicit stop failed"), "{error}");
        assert!(error.contains("disconnect stop failed"), "{error}");
    }

    #[test]
    fn failed_disconnect_stop_is_reported_after_clean_stop() {
        let (mut device, seen) = simulated_close(
            Ok(()),
            Err("disconnect stop failed".into()),
            Err("shutdown cleanup failed".into()),
        );
        let error = device
            .close(Duration::from_secs(1))
            .unwrap_err()
            .to_string();
        seen.recv_timeout(Duration::from_secs(1))
            .expect("close must reject a failed driver cleanup");
        assert!(error.contains("disconnect stop failed"), "{error}");
    }

    #[test]
    fn disconnect_error_event_is_reported_after_clean_stop() {
        let mut device = Device::connect_with(
            Duration::from_secs(1),
            |commands, events| {
                thread::spawn(move || {
                    events.send(Event::Connected("simulated".into())).unwrap();
                    events
                        .send(Event::Stm32Configured(Ok(Configuration::default())))
                        .unwrap();
                    while let Ok(command) = commands.recv() {
                        match command {
                            Command::StopAndDrain => {
                                let _ = events.send(Event::Stm32Stopped(Ok(())));
                            }
                            Command::Disconnect => {
                                let _ =
                                    events.send(Event::Error("disconnect cleanup failed".into()));
                                let _ = events.send(Event::Disconnected);
                                break;
                            }
                            _ => {}
                        }
                    }
                })
            },
            None,
        )
        .unwrap();
        let error = device
            .close(Duration::from_secs(1))
            .unwrap_err()
            .to_string();
        assert!(error.contains("disconnect cleanup failed"), "{error}");
    }

    #[test]
    fn shutdown_timeout_retains_failed_stop_reason() {
        let mut device = Device::connect_with(
            Duration::from_secs(1),
            |commands, events| {
                thread::spawn(move || {
                    events.send(Event::Connected("simulated".into())).unwrap();
                    events
                        .send(Event::Stm32Configured(Ok(Configuration::default())))
                        .unwrap();
                    while let Ok(command) = commands.recv() {
                        match command {
                            Command::StopAndDrain => {
                                let _ =
                                    events.send(Event::Stm32Stopped(Err("SD close failed".into())));
                                thread::sleep(Duration::from_millis(100));
                            }
                            Command::Disconnect => {
                                let _ = events.send(Event::Disconnected);
                                break;
                            }
                            _ => {}
                        }
                    }
                })
            },
            None,
        )
        .unwrap();
        let before = Instant::now();
        let error = device
            .close(Duration::from_millis(10))
            .unwrap_err()
            .to_string();
        assert!(before.elapsed() < Duration::from_millis(80), "{error}");
        assert!(error.contains("SD close failed"), "{error}");
        assert!(error.contains("Timeout"), "{error}");
    }

    #[test]
    fn expired_deadline_still_attempts_final_shutdown() {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let mut device = Device::connect_with(
            Duration::from_secs(1),
            move |commands, events| {
                thread::spawn(move || {
                    events.send(Event::Connected("simulated".into())).unwrap();
                    events
                        .send(Event::Stm32Configured(Ok(Configuration::default())))
                        .unwrap();
                    while let Ok(command) = commands.recv() {
                        match command {
                            Command::StopAndDrain => {
                                // Force close's deadline to expire before it
                                // considers the Shutdown escalation.
                                thread::sleep(Duration::from_millis(100));
                                let _ = events.send(Event::Stm32Stopped(Ok(())));
                            }
                            Command::Disconnect => {}
                            Command::Shutdown => {
                                let _ = shutdown_tx.send(());
                                let _ = events.send(Event::Disconnected);
                                break;
                            }
                            _ => {}
                        }
                    }
                })
            },
            None,
        )
        .unwrap();
        let started = Instant::now();
        assert!(device.close(Duration::from_millis(10)).is_err());
        assert!(started.elapsed() < Duration::from_millis(80));
        shutdown_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("close must attempt Shutdown even after the deadline expired");
    }
}
#[cfg(test)]
mod trigger_queue_tests {
    use super::*;
    fn event_packet(code: u8, timestamp: u64) -> Vec<u8> {
        let mut p = vec![0u8; 48];
        p[..4].copy_from_slice(&[0xa5, 0x5a, 1, 2]);
        p[8..16].copy_from_slice(&timestamp.to_le_bytes());
        p[16] = code;
        p[17] = 1;
        p[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        p[22..26].copy_from_slice(&1000u32.to_le_bytes());
        let crc = omnibci_sdk::protocol::crc16_ccitt(&p[..46]);
        p[46..].copy_from_slice(&crc.to_le_bytes());
        p
    }
    fn core() -> Core {
        Core {
            state: State::Streaming,
            config: Some(Configuration::default()),
            generation: 3,
            ..Default::default()
        }
    }
    #[test]
    fn fragmented_event_only_packets_preserve_repeats_zero_and_exact_time() {
        let mut c = core();
        let timestamp = (1u64 << 53) + 123;
        let packets: Vec<u8> = [0, 7, 7, 255]
            .into_iter()
            .flat_map(|v| event_packet(v, timestamp))
            .collect();
        for chunk in packets.chunks(11) {
            c.event(Event::Bytes(chunk.to_vec())).unwrap();
        }
        assert!(c.queue.is_empty());
        assert_eq!(
            c.triggers.iter().map(|e| e.code).collect::<Vec<_>>(),
            [0, 7, 7, 255]
        );
        for e in &c.triggers {
            assert_eq!(e.sequence, u32::MAX);
            assert_eq!(e.timing.as_ref().unwrap().trigger_timestamp_us, timestamp);
            assert_eq!(e.timing.as_ref().unwrap().anchor_timestamp_us, 1000);
        }
    }
    #[test]
    fn stop_preserves_tail_and_start_clears_old_generation() {
        let mut c = core();
        c.begin(Action::Stop).unwrap();
        c.event(Event::Bytes(event_packet(7, 123))).unwrap();
        c.event(Event::Stm32Stopped(Ok(()))).unwrap();
        assert_eq!(c.triggers.len(), 1);
        c.begin(Action::Start).unwrap();
        assert_eq!(c.triggers.len(), 1);
        c.event(Event::Stm32Started(Ok(()))).unwrap();
        assert!(c.triggers.is_empty());
        assert_eq!(c.generation, 4);
    }
    #[test]
    fn overflow_reports_error_and_retains_prefix() {
        let mut c = core();
        let packets = event_packet(1, 123).repeat(TRIGGER_CAPACITY + 1);
        assert_eq!(c.event(Event::Bytes(packets)).unwrap_err(), Error::Overflow);
        assert_eq!(c.triggers.len(), TRIGGER_CAPACITY);
        assert!(c.triggers.iter().all(|e| e.code == 1));
    }
}
