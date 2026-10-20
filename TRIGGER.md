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

Sample and trigger queues are independent. Poll both regularly: the trigger queue
holds 16,384 events. Overflow is an explicit acquisition fault; queued events remain
available before the empty queue reports the fault. STOP retains tail events;
successfully acknowledged START clears the previous generation. Drain before close.
ESP32 event polling is explicitly unsupported; its existing sample path is unchanged.
`omnibci.decodeTriggers(bytes)` is a fresh-parser offline helper for complete STM32
packets, excluding incomplete/CRC-invalid packets.

Windows MATLAB R2023a acceptance (2026-10-21): MEX build, original offline tests,
32-channel checks, and exact >2^53 uint64 trigger fixtures passed. Real STM32 32.2.3:
USB and BLE each delivered 1,000/1,000 events with zero EEG gaps, CRC errors and
source-time/anchor mismatches. Requested rate was 300/s; MATLAB dispatch actually
took 3.917 s over USB (~255/s) and 3.351 s over BLE (~298/s). This does not establish
hard real-time 300/s scheduling in MATLAB. Linux/macOS and ESB Dongle hardware were
not tested. Local evidence: `D:/workspace/trigger-validation/cc-bindings`.
