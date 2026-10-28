//! MATLAB dispatch wrapper; wire protocol and port lifecycle belong to the SDK.
use omnibci_sdk::trigger::TriggerController;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[derive(Default)]
struct Controllers {
    next: u32,
    ports: HashMap<u64, TriggerController>,
}
static CONTROLLERS: OnceLock<Mutex<Controllers>> = OnceLock::new();

pub(super) fn execute(req: super::Request) -> Result<Value, String> {
    if req.command == "trigger_ports" {
        return TriggerController::available_ports()
            .map(|p| json!(p))
            .map_err(|e| e.to_string());
    }
    let mut state = CONTROLLERS
        .get_or_init(|| Mutex::new(Controllers::default()))
        .lock()
        .map_err(|e| e.to_string())?;
    match req.command.as_str() {
        "trigger_connect" => {
            let timeout = super::duration(req.timeout, 1.0)?;
            let next = state
                .next
                .checked_add(1)
                .ok_or("trigger handle space exhausted")?;
            let port =
                TriggerController::connect(&req.endpoint, timeout).map_err(|e| e.to_string())?;
            state.next = next;
            state.ports.insert(u64::from(next), port);
            Ok(json!({"handle":next}))
        }
        "trigger_close" => {
            state.ports.remove(&req.handle);
            Ok(json!(true))
        }
        "trigger_is_open" => Ok(json!(
            state.ports.get(&req.handle).is_some_and(|p| p.is_open())
        )),
        "trigger_send" => {
            let code = req.code.ok_or("trigger code is required")?;
            if code > 255 {
                return Err("trigger code must be in [0, 255]".into());
            }
            state
                .ports
                .get_mut(&req.handle)
                .ok_or("trigger controller is closed")?
                .send_code(code)
                .map_err(|e| e.to_string())?;
            Ok(json!(true))
        }
        _ => Err("unknown trigger operation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(command: &str, code: Option<u16>) -> super::super::Request {
        serde_json::from_value(json!({"command":command,"code":code,"handle":0})).unwrap()
    }
    #[test]
    fn rejects_missing_invalid_or_closed_send_and_close_is_idempotent() {
        assert!(
            execute(request("trigger_send", None))
                .unwrap_err()
                .contains("required")
        );
        assert!(
            execute(request("trigger_send", Some(256)))
                .unwrap_err()
                .contains("[0, 255]")
        );
        assert!(
            execute(request("trigger_send", Some(0)))
                .unwrap_err()
                .contains("closed")
        );
        assert_eq!(
            execute(request("trigger_is_open", None)).unwrap(),
            json!(false)
        );
        for _ in 0..2 {
            assert_eq!(
                execute(request("trigger_close", None)).unwrap(),
                json!(true)
            );
        }
    }
}
