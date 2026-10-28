# Hardware trigger reception and dispatch

This page describes the `dev/32ch` source API. Installation and the current SDK pin are listed in the [README validation table](README.md#验证状态); old Release ZIPs do not contain these STM32 APIs.

## Dispatch and event fields

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

Read the diagnostic through the separate DFU CDC port (`mcumgr stat clock`), not through `Board`. This binding has no clock-readiness query; elapsed host time alone does not prove `ready=1`.

## Poll both queues

Sample and trigger queues are independent. Poll both regularly: the trigger queue
holds 16,384 events. Overflow is an explicit acquisition fault; queued events remain
available before the empty queue reports the fault. STOP retains tail events;
successfully acknowledged START clears the previous generation. Drain before close.
ESP32 event polling is explicitly unsupported; its existing sample path is unchanged.
`omnibci.decodeTriggers(bytes)` is a fresh-parser offline helper for complete STM32
packets, excluding incomplete/CRC-invalid packets.

Within an already started STM32 session, keep EEG and event histories separate. For example, the following loop uses `board` and `trigger` opened with the connection examples in [README](README.md#连接和采集) and [DEVELOPMENT](DEVELOPMENT.md#serial-trigger-dispatch):

```matlab
% Requires real hardware and a running STM32 acquisition.
% For a timing experiment, first verify clock ready=1 as described above.
batchHistory = {};
eventHistory = {};
sent = false;
started = tic;
while toc(started) < 5
    if ~sent
        trigger.send(7);          % Host dispatch, not reception confirmation
        sent = true;
    end
    batchHistory{end+1} = board.read(1); % Timeout in seconds, not sample count
    eventHistory{end+1} = board.readTriggers(); % May refer to an earlier batch
end
```

The loop retains all returned data, including empty event arrays, and lets device errors propagate. Associate events with EEG by generation and anchor sequence, retaining the original integer times; do not pair them by loop iteration or arrival time. For a long experiment, consume or save both histories incrementally rather than letting them grow without bound. The snippet leaves session shutdown to its caller: stop sending first, call `board.stop()`, then drain pending EEG and events before `board.close()` or another `start()`. An empty post-stop EEG queue raises a timeout; do not suppress other device failures as if they were a normal end marker. Use function-scoped `onCleanup` handles for both opened devices, and call `close()` explicitly when cleanup errors must be observed.

## Validation scope

Historical hardware acceptance (2026-10-21): with STM32 32.2.3 and E73 0.5.1, USB
and BLE each delivered 1,000/1,000 events with zero EEG gaps, CRC errors and
source-time/anchor mismatches. Requested rate was 300/s; MATLAB dispatch actually
took 3.917 s over USB (~255/s) and 3.351 s over BLE (~298/s). That run used SDK
`aa9ab512ecf6243646fbbb23e9432e3c79eac1ec`, not the current pin; Linux/macOS
hardware was not tested, and ESB Dongle hardware was unavailable then and is retired
from current firmware. This does not establish hard real-time 300/s scheduling in
MATLAB. Local evidence: `D:/workspace/trigger-validation/cc-bindings`.

Current SDK 0.3.1 validation is recorded in [DEVELOPMENT](DEVELOPMENT.md#sdk-升级验证). The historical trigger run above does not validate the current pin; physical trigger dispatch was not repeated during this upgrade.

The local raw-evidence path above is not a downloadable archive. Related device-side protocol and trigger test summaries are retained in the firmware [validation index](https://github.com/Omni-Intel/omnibci-firmware-stm32/blob/a20876d6131ecbf51e4eda595f92f866303d99cf/docs/validation/README.md); those records cover their listed images and are not a substitute for the binding-specific acceptance run.
