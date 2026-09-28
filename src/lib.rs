//! Small JSON C ABI for MATLAB. Device ownership and protocol logic stay in omnibci-sdk.
use omnibci_sdk::{
    ADS_CHANNELS, SDK_VERSION,
    acquisition::SampleDecoder,
    device_control::frontend,
    protocol::FrontendConfig,
    session::{Device, discover_ble, serial_ports},
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

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    handle: u64,
    #[serde(default)]
    timeout: Option<f64>,
    #[serde(default)]
    config: Option<ConfigInput>,
    #[serde(default)]
    bytes: Vec<u8>,
    #[serde(default)]
    gains: Option<[u8; ADS_CHANNELS]>,
}

#[derive(Deserialize)]
struct ConfigInput {
    reference: u8,
    mode: u8,
    enabled_mask: u8,
    bias_mask: u8,
    srb2_mask: u8,
    gains: [u8; ADS_CHANNELS],
}

impl From<ConfigInput> for FrontendConfig {
    fn from(value: ConfigInput) -> Self {
        Self {
            reference: value.reference,
            mode: value.mode,
            enabled_mask: value.enabled_mask,
            bias_mask: value.bias_mask,
            srb2_mask: value.srb2_mask,
            gains: value.gains,
            verified: false,
        }
    }
}

#[derive(Default)]
struct Registry {
    next: u64,
    devices: HashMap<u64, Device>,
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

fn config_json(config: FrontendConfig) -> Value {
    json!({
        "reference": config.reference, "mode": config.mode,
        "enabled_mask": config.enabled_mask, "bias_mask": config.bias_mask,
        "srb2_mask": config.srb2_mask, "gains": config.gains,
        "verified": config.verified
    })
}

fn execute(req: Request) -> Result<Value, String> {
    match req.command.as_str() {
        "version" => Ok(json!({"binding": env!("CARGO_PKG_VERSION"), "sdk": SDK_VERSION})),
        "serial_ports" => Ok(json!(serial_ports().map_err(|e| e.to_string())?)),
        "discover_ble" => {
            let devices = discover_ble(duration(req.timeout, 10.0)?).map_err(|e| e.to_string())?;
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
            let timeout = duration(req.timeout, 15.0)?;
            let device = if let Some(port) = req.endpoint.strip_prefix("serial://") {
                if port.is_empty() {
                    return Err("serial port is empty".into());
                }
                Device::connect_serial(port, timeout)
            } else if let Some(key) = req.endpoint.strip_prefix("ble://") {
                if key.is_empty() {
                    return Err("BLE key is empty".into());
                }
                Device::connect_ble(key, timeout)
            } else {
                return Err("endpoint must start with serial:// or ble://".into());
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
                "sync_drop": decoder.parser.sync_drop
            }))
        }
        "snapshot" | "start" | "stop" | "configure" | "read" | "close" => {
            let mut state = registry().lock().map_err(|e| e.to_string())?;
            let device = state
                .devices
                .get_mut(&req.handle)
                .ok_or("invalid or closed handle")?;
            match req.command.as_str() {
                "snapshot" => {
                    let s = device.snapshot();
                    Ok(json!({
                        "state": format!("{:?}", s.state),
                        "metadata": s.metadata.map(|m| json!({"firmware": m.firmware, "hardware": m.hardware, "protocol": m.protocol})),
                        "config": s.config.map(config_json),
                        "generation": s.generation.to_string(),
                        "samples": s.samples.to_string(),
                        "missing_samples": s.missing_samples.to_string(),
                        "crc_errors": s.crc_errors.to_string(),
                        "ble": {"received": s.ble.received, "delivered": s.ble.delivered,
                            "crc_bad": s.ble.crc_bad, "duplicates": s.ble.duplicates,
                            "out_of_order": s.ble.out_of_order, "gap_markers": s.ble.gap_markers},
                        "error": s.error.map(|e| e.to_string())
                    }))
                }
                "start" => {
                    device
                        .start(duration(req.timeout, 5.0)?)
                        .map_err(|e| e.to_string())?;
                    Ok(json!(true))
                }
                "stop" => {
                    device
                        .stop(duration(req.timeout, 5.0)?)
                        .map_err(|e| e.to_string())?;
                    Ok(json!(true))
                }
                "configure" => {
                    let config = req.config.ok_or("config is required")?.into();
                    device
                        .configure(config, duration(req.timeout, 5.0)?)
                        .map_err(|e| e.to_string())?;
                    Ok(json!(true))
                }
                "read" => {
                    let batch = device
                        .read(duration(req.timeout, 2.0)?)
                        .map_err(|e| e.to_string())?;
                    Ok(json!({
                        "eeg_uv": batch.frames.iter().map(|f| &f.values_uv).collect::<Vec<_>>(),
                        "raw_counts": batch.frames.iter().map(|f| &f.raw_counts).collect::<Vec<_>>(),
                        "sequence": batch.frames.iter().map(|f| f.sequence).collect::<Vec<_>>(),
                        "valid": batch.frames.iter().map(|f| f.valid).collect::<Vec<_>>(),
                        "mode": batch.frames.iter().map(|f| f.mode).collect::<Vec<_>>(),
                        "status": batch.frames.iter().map(|f| &f.status).collect::<Vec<_>>(),
                        "sample_indices": batch.sample_indices.iter().map(u64::to_string).collect::<Vec<_>>(),
                        "generation": batch.generation.to_string(),
                        "sample_rate_hz": batch.sample_rate_hz,
                        "received_age_s": batch.received_at.elapsed().as_secs_f64()
                    }))
                }
                "close" => {
                    // Remove only after successful close; a timed-out session can be retried.
                    device
                        .close(duration(req.timeout, 6.0)?)
                        .map_err(|e| e.to_string())?;
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
            return Err("null request".to_owned());
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
        assert_eq!(
            call(json!({"command": "version"}))["value"]["sdk"],
            SDK_VERSION
        );
        assert_eq!(
            call(json!({"command": "decode_frames", "bytes": []}))["value"]["eeg_uv"],
            json!([])
        );
        assert_eq!(
            call(json!({"command": "connect", "endpoint": "invalid"}))["ok"],
            false
        );
        assert_eq!(call(json!({"command": "read", "handle": 999}))["ok"], false);
        assert_eq!(
            call(json!({"command": "decode_frames", "gains": [3,24,24,24,24,24,24,24]}))["ok"],
            false
        );
    }
}
