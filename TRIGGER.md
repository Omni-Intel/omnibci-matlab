# Hardware trigger reception and dispatch

`omnibci.TriggerController.connect(port, timeout)` opens the external generator via
the Rust SDK. `send(code)` accepts 0..255 (including zero/repeats), `close()` is
idempotent, and `isOpen()` reports port ownership. Success confirms host dispatch,
not an acknowledgement or a device timestamp. Use `onCleanup` for resource release.

`board.readTriggers()` nonblockingly drains currently available STM32 events.
Every event includes `generation` (uint64), `code` (uint16), anchor `sequence`
(uint32), legacy milliseconds, and optional `timing` containing
`trigger_timestamp_us` (uint64) and `anchor_timestamp_us` (uint32). The JSON bridge
uses decimal strings for 64-bit values and parses them using integer arithmetic;
it never passes them through double. More than one event can refer to one anchor,
and repeated codes remain separate events.

`board.read()` now provides `source_timestamp_us` (uint32) and
`source_timestamp_valid` (logical) for each EEG frame. False validity denotes
unavailable source time; a zero with true validity is a legitimate wrap value.
The original trigger-start time and actual DRDY anchor identify the event,
independently of USB/BLE delivery delay. Compute adjacent u32 clock differences
modulo 2^32. Do not convert trigger time to an integer EEG sample position.

STM32 32.2.4 aligns exported EEG and trigger timestamps to the nominal ADS clock
after a startup settling interval; wire fields and units are unchanged. Before a
timing experiment, acquire uninterrupted for at least one second and verify the
firmware clock diagnostic `ready=1`. This is a monotonic, ADC-referenced timebase;
it makes no UTC or host-clock accuracy claim. See the firmware
[TIMING.md](https://github.com/Omni-Intel/omnibci-firmware-stm32/blob/main/stm32/TIMING.md).

Sample and trigger queues are independent. Poll both regularly: the trigger queue
holds 16,384 events. Overflow is an explicit acquisition fault; queued events remain
available before the empty queue reports the fault. STOP retains tail events;
successfully acknowledged START clears the previous generation. Drain before close.
ESP32 event polling is explicitly unsupported; its existing sample path is unchanged.
`omnibci.decodeTriggers(bytes)` is a fresh-parser offline helper for complete STM32
packets, excluding incomplete/CRC-invalid packets.

Historical hardware acceptance (2026-10-21): with STM32 32.2.3 and E73 0.5.1, USB
and BLE each delivered 1,000/1,000 events with zero EEG gaps, CRC errors and
source-time/anchor mismatches. Requested rate was 300/s; MATLAB dispatch actually
took 3.917 s over USB (~255/s) and 3.351 s over BLE (~298/s). That run used SDK
`aa9ab512ecf6243646fbbb23e9432e3c79eac1ec`, not the current pin; Linux/macOS
hardware was not tested, and ESB Dongle hardware was unavailable then and is retired
from current firmware. This does not establish hard real-time 300/s scheduling in
MATLAB. Local evidence: `D:/workspace/trigger-validation/cc-bindings`.

Current upgrade validation: the bindings were upgraded to `e1f25b3...` and rebuilt
as a Windows R2023a MEX. `test_offline`, `test_stm32_offline`, `test_trigger_controller`
and `test_trigger_timing` passed, along with 21 native Rust tests,
`cargo fmt --check` and `cargo clippy --locked -- -D warnings`. New SDK hardware
tests were not rerun; the historical acceptance above does not validate the current
pin.
