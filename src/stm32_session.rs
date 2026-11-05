//! Language-neutral ownership lives in the SDK; conversion stays in this binding.
pub use omnibci_sdk::session::{Stm32Device as Device, discover_stm32_ble as discover};

#[cfg(test)]
mod tests {
    use super::*;
    use omnibci_sdk::{
        stm32::Configuration,
        transport::{Command, Event},
    };
    use std::time::Duration;

    #[test]
    fn dispose_releases_registry_entry_after_permanent_fault() {
        let device = Device::connect_with_driver(
            Duration::from_secs(1),
            |commands, events| {
                std::thread::spawn(move || {
                    events.send(Event::Connected("simulated".into())).unwrap();
                    events
                        .send(Event::Stm32Configured(Ok(Configuration::default())))
                        .unwrap();
                    while let Ok(command) = commands.recv() {
                        match command {
                            Command::StopAndDrain => {
                                let _ = events.send(Event::Stm32Stopped(Err(
                                    "permanent device fault".into(),
                                )));
                            }
                            Command::Disconnect | Command::Shutdown => {
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
        let handle = {
            let mut registry = crate::registry().lock().unwrap();
            registry.next += 1;
            let handle = registry.next;
            registry
                .devices
                .insert(handle, crate::Backend::Stm32(device));
            handle
        };
        let request =
            serde_json::from_value(serde_json::json!({"command":"dispose", "handle":handle}))
                .unwrap();
        let error = crate::execute(request).unwrap_err();
        assert!(error.contains("permanent device fault"), "{error}");
        assert!(
            !crate::registry()
                .lock()
                .unwrap()
                .devices
                .contains_key(&handle)
        );
    }
}
