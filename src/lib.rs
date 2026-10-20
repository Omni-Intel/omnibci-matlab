//! Small JSON C ABI for MATLAB. Device ownership and protocol logic stay in omnibci-sdk.
//!
//! ESP32 sessions use the SDK's synchronous `session::Device`; STM32/E73 sessions
//! use the synchronous owner in [`stm32_session`], which wraps the SDK's low-level
//! transport spawn APIs. Neither path reimplements a wire protocol here.
mod stm32_session;
mod trigger;

use omnibci_sdk::{
    ADS_CHANNELS, SDK_VERSION,
    acquisition::SampleDecoder,
    device_control::frontend,
    protocol::FrontendConfig,
    session::{Device, Samples, discover_ble, serial_ports},
    stm32::Configuration as Stm32Configuration,
    transport::{ComponentFirmware, DeviceMetadata},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, OnceLock},
    time::Duration,
};

#[cfg(feature = "mex")]
mod mex;

/// SDK baseline compiled into the binding. Update together with the SDK submodule.
const SDK_REVISION: &str = "aa9ab512ecf6243646fbbb23e9432e3c79eac1ec";

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    code: Option<u16>,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    handle: u64,
    #[serde(default)]
    timeout: Option<f64>,
    #[serde(default)]
    device_type: Option<String>,
    #[serde(default)]
    config: Option<Value>,
    #[serde(default)]
    bytes: Vec<u8>,
    #[serde(default)]
    gains: Option<[u8; ADS_CHANNELS]>,
}

/// ESP32 ADS1299 frontend configuration. `deny_unknown_fields` prevents
/// STM32-only fields (for example `loff_mask`) from being silently accepted
/// for an ESP32 device at the native boundary. `verified` is accepted and
/// ignored because `getConfig` reports it and callers may pass that struct
/// back unchanged.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigInput {
    reference: u8,
    mode: u8,
    enabled_mask: u8,
    bias_mask: u8,
    srb2_mask: u8,
    gains: [u8; ADS_CHANNELS],
    /// Accepted and ignored, whatever its JSON representation.
    #[serde(default, rename = "verified")]
    _verified: Option<Value>,
}

impl From<ConfigInput> for FrontendConfig {
    fn from(value: ConfigInput) -> Self {
        let ConfigInput {
            reference,
            mode,
            enabled_mask,
            bias_mask,
            srb2_mask,
            gains,
            _verified: _,
        } = value;
        Self {
            reference,
            mode,
            enabled_mask,
            bias_mask,
            srb2_mask,
            gains,
            verified: false,
        }
    }
}

/// STM32 V3 configuration carried as JSON numbers. `enabled_mask`, `bias_mask`
/// and `loff_mask` must fit `u32`; MATLAB jsonencode emits exact integers for
/// uint32-compatible doubles. `verified` is accepted and ignored so the
/// `getConfig` struct can be passed back unchanged.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stm32ConfigInput {
    mode: u8,
    enabled_mask: u32,
    bias_mask: u32,
    loff_mask: u32,
    gains: [u8; 32],
    /// Accepted and ignored, whatever its JSON representation.
    #[serde(default, rename = "verified")]
    _verified: Option<Value>,
}

impl From<Stm32ConfigInput> for Stm32Configuration {
    fn from(value: Stm32ConfigInput) -> Self {
        let Stm32ConfigInput {
            mode,
            enabled_mask,
            bias_mask,
            loff_mask,
            gains,
            _verified: _,
        } = value;
        Self {
            mode,
            enabled: enabled_mask,
            bias: bias_mask,
            loff: loff_mask,
            gains,
        }
    }
}

enum Backend {
    Esp32(Device),
    Stm32(stm32_session::Device),
}

impl Backend {
    fn channel_count(&self) -> usize {
        match self {
            Self::Esp32(_) => ADS_CHANNELS,
            Self::Stm32(_) => 32,
        }
    }
    /// ESP32 ADS1299 frames carry three status bytes. STM32 V3 frames carry
    /// twelve bytes: the four genuine ADS1299 status words at frame bytes
    /// 12..24, exactly as exposed by the SDK parser.
    fn status_bytes(&self) -> usize {
        match self {
            Self::Esp32(_) => 3,
            Self::Stm32(_) => 12,
        }
    }
    fn start(&mut self, timeout: Duration) -> Result<(), String> {
        match self {
            Self::Esp32(device) => device.start(timeout).map_err(|e| e.to_string()),
            Self::Stm32(device) => device.start(timeout).map_err(|e| e.to_string()),
        }
    }
    fn stop(&mut self, timeout: Duration) -> Result<(), String> {
        match self {
            Self::Esp32(device) => device.stop(timeout).map_err(|e| e.to_string()),
            Self::Stm32(device) => device.stop(timeout).map_err(|e| e.to_string()),
        }
    }
    fn close(&mut self, timeout: Duration) -> Result<(), String> {
        match self {
            Self::Esp32(device) => device.close(timeout).map_err(|e| e.to_string()),
            Self::Stm32(device) => device.close(timeout).map_err(|e| e.to_string()),
        }
    }
    fn read(&mut self, timeout: Duration) -> Result<Samples, String> {
        match self {
            Self::Esp32(device) => device.read(timeout).map_err(|e| e.to_string()),
            Self::Stm32(device) => device.read(timeout).map_err(|e| e.to_string()),
        }
    }
    fn configure(&mut self, config: Value, timeout: Duration) -> Result<(), String> {
        match self {
            Self::Esp32(device) => {
                let config: ConfigInput = serde_json::from_value(config)
                    .map_err(|e| format!("invalid ESP32 configuration: {e}"))?;
                device
                    .configure(config.into(), timeout)
                    .map_err(|e| e.to_string())
            }
            Self::Stm32(device) => {
                let config: Stm32ConfigInput = serde_json::from_value(config)
                    .map_err(|e| format!("invalid STM32 configuration: {e}"))?;
                let config: Stm32Configuration = config.into();
                // Reject wrong-pair/untrusted values before any device transaction.
                config
                    .validate()
                    .map_err(|e| format!("invalid STM32 configuration: {e}"))?;
                device.configure(config, timeout).map_err(|e| e.to_string())
            }
        }
    }
    fn snapshot(&self) -> Value {
        match self {
            Self::Esp32(device) => {
                let snapshot = device.snapshot();
                json!({
                    "state": format!("{:?}", snapshot.state),
                    "metadata": snapshot.metadata.map(metadata_json),
                    "config": snapshot.config.map(config_json),
                    "generation": snapshot.generation.to_string(),
                    "samples": snapshot.samples.to_string(),
                    "missing_samples": snapshot.missing_samples.to_string(),
                    "crc_errors": snapshot.crc_errors.to_string(),
                    "ble": {"received": snapshot.ble.received, "delivered": snapshot.ble.delivered,
                        "crc_bad": snapshot.ble.crc_bad, "duplicates": snapshot.ble.duplicates,
                        "out_of_order": snapshot.ble.out_of_order, "gap_markers": snapshot.ble.gap_markers},
                    "error": snapshot.error.map(|e| e.to_string())
                })
            }
            Self::Stm32(device) => {
                let snapshot = device.snapshot();
                json!({
                    "state": format!("{:?}", snapshot.state),
                    "metadata": snapshot.metadata.map(metadata_json),
                    "config": snapshot.config.map(stm32_config_json),
                    "generation": snapshot.generation.to_string(),
                    "samples": snapshot.samples.to_string(),
                    "missing_samples": snapshot.missing_samples.to_string(),
                    "crc_errors": snapshot.crc_errors.to_string(),
                    // BLE/transport metrics are ESP32-specific and unavailable
                    // for STM32. Return an empty object rather than fabricated
                    // zero counters that would look like measured values.
                    "ble": {},
                    "error": snapshot.error.map(|e| e.to_string())
                })
            }
        }
    }
}

#[derive(Default)]
struct Registry {
    next: u64,
    devices: HashMap<u64, Backend>,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

fn duration(seconds: Option<f64>, default: f64) -> Result<Duration, String> {
    let value = seconds.unwrap_or(default);
    if !value.is_finite() || !(0.0 < value && value <= 60.0) {
        return Err("timeout must be in (0, 60] seconds".into());
    }
    Ok(Duration::from_secs_f64(value))
}

fn selected_device_type(value: Option<&str>) -> Result<&str, String> {
    let value = value.unwrap_or("esp32");
    match value {
        "esp32" | "stm32" => Ok(value),
        _ => Err("device_type must be esp32 or stm32".into()),
    }
}

fn config_json(config: FrontendConfig) -> Value {
    json!({
        "reference": config.reference, "mode": config.mode,
        "enabled_mask": config.enabled_mask, "bias_mask": config.bias_mask,
        "srb2_mask": config.srb2_mask, "gains": config.gains,
        "verified": config.verified
    })
}

fn stm32_config_json(config: Stm32Configuration) -> Value {
    json!({
        "mode": config.mode,
        "enabled_mask": config.enabled,
        "bias_mask": config.bias,
        "loff_mask": config.loff,
        "gains": config.gains,
        "verified": true
    })
}

fn component_json(firmware: ComponentFirmware) -> Value {
    json!({
        "version": firmware.version,
        "revision": firmware.revision,
        "dirty": firmware.dirty,
        "state": firmware.state,
        "protocol": firmware.protocol,
    })
}

/// Legacy metadata fields stay present; component provenance is independently
/// reported and serialized as null when that component is unavailable.
fn metadata_json(metadata: DeviceMetadata) -> Value {
    json!({
        "firmware": metadata.firmware,
        "hardware": metadata.hardware,
        "protocol": metadata.protocol,
        "stm32": metadata.stm32.map(component_json),
        "e73": metadata.e73.map(component_json),
    })
}

/// MATLAB receives integer counters as decimal strings so jsondecode cannot
/// round values above 2^53. Non-finite STM32 microvolt values become 0 here and
/// are restored to NaN by normalizeBatch for rows marked invalid.
fn samples_json(
    samples: &Samples,
    channel_count: usize,
    status_bytes: usize,
) -> Result<Value, String> {
    if samples.sample_indices.len() != samples.frames.len() {
        return Err("unexpected device sample index shape".into());
    }
    for frame in &samples.frames {
        if frame.values_uv.len() != channel_count {
            return Err("unexpected device sample channel count".into());
        }
        if frame
            .raw_counts
            .as_ref()
            .is_none_or(|counts| counts.len() != channel_count)
        {
            return Err("unexpected device raw sample shape".into());
        }
        if frame.status.len() != status_bytes {
            return Err("unexpected device sample status shape".into());
        }
    }
    let eeg_uv: Vec<Vec<f32>> = samples
        .frames
        .iter()
        .map(|frame| {
            frame
                .values_uv
                .iter()
                .map(|value| if value.is_finite() { *value } else { 0.0 })
                .collect()
        })
        .collect();
    let raw_counts: Vec<Vec<i32>> = samples
        .frames
        .iter()
        .map(|frame| frame.raw_counts.clone().unwrap_or_default())
        .collect();
    // Serialize every status byte for both backends: three ADS1299 bytes for
    // ESP32 and the four genuine ADS1299 status words (12 bytes) for STM32.
    let status: Vec<&[u8]> = samples
        .frames
        .iter()
        .map(|frame| frame.status.as_slice())
        .collect();
    Ok(json!({
        "eeg_uv": eeg_uv,
        "raw_counts": raw_counts,
        "sequence": samples.frames.iter().map(|frame| frame.sequence).collect::<Vec<_>>(),
        "valid": samples.frames.iter().map(|frame| frame.valid).collect::<Vec<_>>(),
        "mode": samples.frames.iter().map(|frame| frame.mode).collect::<Vec<_>>(),
        "status": status,
        "sample_indices": samples.sample_indices.iter().map(u64::to_string).collect::<Vec<_>>(),
        "generation": samples.generation.to_string(),
        "sample_rate_hz": samples.sample_rate_hz,
        "received_age_s": samples.received_at.elapsed().as_secs_f64(),
        "channel_count": channel_count,
        "status_bytes": status_bytes
    }))
}

fn execute(req: Request) -> Result<Value, String> {
    if req.command.starts_with("trigger_") {
        return trigger::execute(req);
    }
    match req.command.as_str() {
        "dispose" => {
            // Destruction must release the registry entry even when a session
            // retains a permanent fault. Explicit close still preserves it for
            // inspection/retry. Drop the registry lock before bounded cleanup.
            let timeout = duration(req.timeout, 6.0)?;
            let device = registry()
                .lock()
                .map_err(|e| e.to_string())?
                .devices
                .remove(&req.handle);
            if let Some(mut device) = device {
                device.close(timeout)?;
            }
            Ok(json!(true))
        }
        "version" => Ok(json!({
            "binding": env!("CARGO_PKG_VERSION"),
            "sdk": SDK_VERSION,
            "sdk_revision": SDK_REVISION
        })),
        "serial_ports" => Ok(json!(serial_ports().map_err(|e| e.to_string())?)),
        "discover_ble" => {
            let device_type = selected_device_type(req.device_type.as_deref())?;
            let timeout = duration(req.timeout, 10.0)?;
            let devices = match device_type {
                "esp32" => discover_ble(timeout),
                "stm32" => stm32_session::discover(timeout),
                _ => unreachable!(),
            }
            .map_err(|e| e.to_string())?;
            Ok(json!(
                devices
                    .into_iter()
                    .map(|d| json!({
                        "endpoint": format!("ble://{}", d.key), "name": d.name,
                        "address": d.address, "rssi_dbm": d.rssi_dbm
                    }))
                    .collect::<Vec<_>>()
            ))
        }
        "connect" => {
            let device_type = selected_device_type(req.device_type.as_deref())?;
            let timeout = duration(req.timeout, 15.0)?;
            let (kind, address) = req
                .endpoint
                .split_once("://")
                .ok_or("endpoint must start with serial:// or ble://")?;
            if address.is_empty() {
                return Err("device address is empty".into());
            }
            let device = match (device_type, kind) {
                ("esp32", "serial") => Device::connect_serial(address, timeout).map(Backend::Esp32),
                ("esp32", "ble") => Device::connect_ble(address, timeout).map(Backend::Esp32),
                ("stm32", "serial") | ("stm32", "ble") => {
                    stm32_session::Device::connect(kind, address, timeout).map(Backend::Stm32)
                }
                _ => return Err("endpoint must start with serial:// or ble://".into()),
            }
            .map_err(|e| e.to_string())?;
            let mut state = registry().lock().map_err(|e| e.to_string())?;
            state.next = state
                .next
                .checked_add(1)
                .ok_or("handle counter exhausted")?;
            let handle = state.next;
            state.devices.insert(handle, device);
            Ok(json!({"handle": handle}))
        }
        "decode_frames" => {
            // Offline decoding remains the eight-channel ESP32 ADS1299 path.
            let gains = req.gains.unwrap_or([24; ADS_CHANNELS]);
            frontend::validate_gains(&gains).map_err(str::to_owned)?;
            let mut decoder = SampleDecoder {
                scale: gains.map(|gain| omnibci_sdk::lsb_uv(f32::from(gain))),
                ..Default::default()
            };
            let frames = decoder.feed(&req.bytes, false).frames;
            Ok(json!({
                "eeg_uv": frames.iter().map(|f| &f.values_uv).collect::<Vec<_>>(),
                "raw_counts": frames.iter().map(|f| &f.raw_counts).collect::<Vec<_>>(),
                "sequence": frames.iter().map(|f| f.sequence).collect::<Vec<_>>(),
                "valid": frames.iter().map(|f| f.valid).collect::<Vec<_>>(),
                "mode": frames.iter().map(|f| f.mode).collect::<Vec<_>>(),
                "status": frames.iter().map(|f| &f.status).collect::<Vec<_>>(),
                "crc_errors": decoder.parser.crc_bad,
                "sync_drop": decoder.parser.sync_drop,
                "channel_count": ADS_CHANNELS,
                "status_bytes": 3
            }))
        }
        "snapshot" | "start" | "stop" | "configure" | "read" | "close" => {
            let mut state = registry().lock().map_err(|e| e.to_string())?;
            let device = state
                .devices
                .get_mut(&req.handle)
                .ok_or("invalid or closed handle")?;
            match req.command.as_str() {
                "snapshot" => Ok(device.snapshot()),
                "start" => {
                    device.start(duration(req.timeout, 5.0)?)?;
                    Ok(json!(true))
                }
                "stop" => {
                    device.stop(duration(req.timeout, 5.0)?)?;
                    Ok(json!(true))
                }
                "configure" => {
                    let config = req.config.ok_or("config is required")?;
                    device.configure(config, duration(req.timeout, 5.0)?)?;
                    Ok(json!(true))
                }
                "read" => {
                    let batch = device.read(duration(req.timeout, 2.0)?)?;
                    let channel_count = device.channel_count();
                    let status_bytes = device.status_bytes();
                    samples_json(&batch, channel_count, status_bytes)
                }
                "close" => {
                    // Remove only after successful close; a timed-out session can be retried.
                    device.close(duration(req.timeout, 6.0)?)?;
                    state.devices.remove(&req.handle);
                    Ok(json!(true))
                }
                _ => unreachable!(),
            }
        }
        _ => Err(format!("unknown command: {}", req.command)),
    }
}

/// Returned memory is owned by Rust and must be released with `omnibci_free`.
#[unsafe(no_mangle)]
/// # Safety
/// `request` must point to a valid NUL-terminated C string.
pub unsafe extern "C" fn omnibci_call(request: *const c_char) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if request.is_null() {
            return Err("null request".into());
        }
        // SAFETY: the caller promises a NUL-terminated UTF-8 string.
        let input = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let req: Request = serde_json::from_str(input).map_err(|e| e.to_string())?;
        execute(req)
    }));
    let response = match result {
        Ok(Ok(value)) => json!({"ok": true, "value": value}),
        Ok(Err(message)) => json!({"ok": false, "error": message}),
        Err(_) => json!({"ok": false, "error": "native bridge panicked"}),
    };
    CString::new(response.to_string()).unwrap().into_raw()
}

#[unsafe(no_mangle)]
/// # Safety
/// `response` must be a pointer returned by `omnibci_call`, released exactly once.
pub unsafe extern "C" fn omnibci_free(response: *mut c_char) {
    if !response.is_null() {
        // SAFETY: response was returned by omnibci_call and is freed exactly once.
        unsafe {
            drop(CString::from_raw(response));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn call(value: Value) -> Value {
        let request = CString::new(value.to_string()).unwrap();
        let response = unsafe { omnibci_call(request.as_ptr()) };
        let answer = unsafe { CStr::from_ptr(response) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { omnibci_free(response) };
        serde_json::from_str(&answer).unwrap()
    }

    #[test]
    fn offline_commands_and_errors() {
        let version = call(json!({"command": "version"}));
        assert_eq!(version["value"]["sdk"], SDK_VERSION);
        assert_eq!(version["value"]["sdk_revision"], SDK_REVISION);
        let decoded = call(json!({"command": "decode_frames", "bytes": []}));
        assert_eq!(decoded["value"]["eeg_uv"], json!([]));
        assert_eq!(
            decoded["value"]["channel_count"].as_u64(),
            Some(ADS_CHANNELS as u64)
        );
        assert_eq!(decoded["value"]["status_bytes"].as_u64(), Some(3));
        assert_eq!(
            call(json!({"command": "connect", "endpoint": "invalid"}))["ok"],
            false
        );
        assert_eq!(
            call(json!({"command": "connect", "endpoint": "serial://COM1", "device_type": "bad"}))
                ["ok"],
            false
        );
        assert_eq!(call(json!({"command": "read", "handle": 999}))["ok"], false);
        assert_eq!(
            call(json!({"command": "decode_frames", "gains": [3,24,24,24,24,24,24,24]}))["ok"],
            false
        );
    }

    #[test]
    fn stm32_config_rejects_wrong_device_pair_and_untrusted_values() {
        let valid = json!({
            "mode": 0, "enabled_mask": 4294967295u32, "bias_mask": 4294967295u32,
            "loff_mask": 0, "gains": vec![24; 32]
        });
        let parsed: Stm32ConfigInput = serde_json::from_value(valid.clone()).unwrap();
        assert!(Stm32Configuration::from(parsed).validate().is_ok());
        let mut invalid = vec![];
        for (key, value) in [
            ("mode", json!(5)),
            ("enabled_mask", json!(true)),
            ("enabled_mask", json!(4294967296u64)),
            ("enabled_mask", json!(-1)),
            ("gains", json!(vec![24; 8])),
            ("gains", json!(vec![3; 32])),
            ("gains", json!(vec![true; 32])),
            ("enabled_mask", json!(1)),
        ] {
            let mut config = valid.clone();
            config[key] = value;
            invalid.push(config);
        }
        for config in invalid {
            let result = serde_json::from_value::<Stm32ConfigInput>(config)
                .map_err(|e| e.to_string())
                .and_then(|config| Stm32Configuration::from(config).validate());
            assert!(result.is_err());
        }
        let esp32 = json!({
            "reference": 0, "mode": 1, "enabled_mask": 255, "bias_mask": 255,
            "srb2_mask": 0, "gains": vec![24; 8]
        });
        assert!(serde_json::from_value::<Stm32ConfigInput>(esp32.clone()).is_err());
        assert!(serde_json::from_value::<ConfigInput>(valid.clone()).is_err());

        // `verified` is reported by getConfig and is accepted but ignored by
        // both native configuration shapes.
        let mut stm32_verified = valid;
        stm32_verified["verified"] = json!(true);
        let parsed: Stm32ConfigInput = serde_json::from_value(stm32_verified).unwrap();
        assert_eq!(
            Stm32Configuration::from(parsed),
            Stm32Configuration::default()
        );
    }

    #[test]
    fn esp32_config_rejects_stm32_only_fields_and_ignores_verified() {
        let esp32 = json!({
            "reference": 0, "mode": 1, "enabled_mask": 255, "bias_mask": 255,
            "srb2_mask": 0, "gains": vec![24; 8]
        });
        let parsed: ConfigInput = serde_json::from_value(esp32.clone()).unwrap();
        let config = FrontendConfig::from(parsed);
        assert!(!config.verified, "input verified is never trusted");
        assert_eq!(config.enabled_mask, 255);

        let mut with_verified = esp32.clone();
        with_verified["verified"] = json!(true);
        let parsed: ConfigInput = serde_json::from_value(with_verified).unwrap();
        assert!(!FrontendConfig::from(parsed).verified);

        // STM32-only and arbitrary unknown fields must not be silently ignored
        // at the native ESP32 boundary.
        for (field, value) in [
            ("loff_mask", json!(0)),
            ("enabled", json!(255)),
            ("srb1", json!(true)),
        ] {
            let mut wrong = esp32.clone();
            wrong[field] = value;
            assert!(
                serde_json::from_value::<ConfigInput>(wrong).is_err(),
                "ESP32 config accepted unknown field {field}"
            );
        }
    }

    #[test]
    fn metadata_keeps_legacy_fields_and_component_provenance() {
        let metadata = DeviceMetadata {
            stm32: Some(ComponentFirmware {
                version: Some("32.1.0".into()),
                revision: Some("012345abcdef".into()),
                dirty: Some(true),
                state: "available".into(),
                protocol: "EEG V3 · Control V1".into(),
            }),
            e73: None,
            firmware: "STM32 32.1.0".into(),
            hardware: "STM32H563".into(),
            protocol: "EEG V3".into(),
        };
        let value = metadata_json(metadata);
        assert_eq!(value["firmware"], "STM32 32.1.0");
        assert_eq!(value["hardware"], "STM32H563");
        assert_eq!(value["protocol"], "EEG V3");
        assert_eq!(value["stm32"]["version"], "32.1.0");
        assert_eq!(value["stm32"]["revision"], "012345abcdef");
        assert_eq!(value["stm32"]["dirty"].as_bool(), Some(true));
        assert_eq!(value["stm32"]["state"], "available");
        assert_eq!(value["stm32"]["protocol"], "EEG V3 · Control V1");
        assert!(value["e73"].is_null());
    }

    fn stm32_samples() -> Samples {
        Samples {
            generation: u64::MAX,
            frames: vec![omnibci_sdk::protocol::EegFrame {
                sequence: 7,
                values_uv: vec![f32::NAN; 32],
                raw_counts: Some(vec![0; 32]),
                valid: false,
                mode: 0,
                status: vec![0xC0; 12],
                source_timestamp_ms: None,
                source_timestamp_us: None,
                triggers: vec![],
            }],
            sample_indices: vec![u64::MAX],
            received_at: Instant::now(),
            sample_rate_hz: 250,
        }
    }

    #[test]
    fn stm32_samples_expose_32_channels_twelve_status_bytes_and_decimal_counters() {
        let value = samples_json(&stm32_samples(), 32, 12).unwrap();
        assert_eq!(value["channel_count"].as_u64(), Some(32));
        assert_eq!(value["status_bytes"].as_u64(), Some(12));
        assert_eq!(value["status"][0].as_array().unwrap().len(), 12);
        assert_eq!(value["status"][0][0].as_u64(), Some(0xC0));
        assert_eq!(value["eeg_uv"][0].as_array().unwrap().len(), 32);
        assert_eq!(value["eeg_uv"][0][0].as_f64(), Some(0.0));
        assert_eq!(value["generation"], json!(u64::MAX.to_string()));
        assert_eq!(value["sample_indices"][0], json!(u64::MAX.to_string()));
        assert_eq!(value["valid"][0], json!(false));
    }

    #[test]
    fn stm32_status_rows_are_serialized_for_empty_and_multi_sample_batches() {
        let mut samples = stm32_samples();
        samples.frames.push(samples.frames[0].clone());
        samples.sample_indices.push(8);
        samples.frames[0].status[11] = 1;
        samples.frames[1].status[11] = 2;
        let value = samples_json(&samples, 32, 12).unwrap();
        assert_eq!(value["status"].as_array().unwrap().len(), 2);
        assert_eq!(value["status"][0].as_array().unwrap().len(), 12);
        assert_eq!(value["status"][0][11].as_u64(), Some(1));
        assert_eq!(value["status"][1][11].as_u64(), Some(2));

        let empty = Samples {
            generation: 0,
            frames: vec![],
            sample_indices: vec![],
            received_at: Instant::now(),
            sample_rate_hz: 250,
        };
        let value = samples_json(&empty, 32, 12).unwrap();
        assert_eq!(value["channel_count"].as_u64(), Some(32));
        assert_eq!(value["status_bytes"].as_u64(), Some(12));
        assert_eq!(value["status"], json!([]));
        assert_eq!(value["eeg_uv"], json!([]));
    }

    #[test]
    fn samples_json_rejects_incomplete_status_payloads() {
        let mut samples = stm32_samples();
        samples.frames[0].status.truncate(3);
        assert!(samples_json(&samples, 32, 12).is_err());
        let mut samples = stm32_samples();
        samples.frames[0].values_uv.truncate(8);
        assert!(samples_json(&samples, 32, 12).is_err());
        let mut samples = stm32_samples();
        samples.frames[0].raw_counts = None;
        assert!(samples_json(&samples, 32, 12).is_err());
    }
}
