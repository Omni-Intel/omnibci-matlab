classdef Board < handle
    %BOARD Acquire ESP32/ADS1299 or STM32 32-channel data over USB or BLE.
    %   B = omnibci.Board.connect(ENDPOINT) opens an ESP32 device and returns
    %   a handle object. Use connect rather than calling the constructor.
    %   Methods block until completion or timeout; timeouts are in seconds.
    %   Keep reading during acquisition to avoid overflowing the SDK queue.
    %   Only one controlling process may own the device at a time.
    %
    %   Device types:
    %     "esp32" - Eight-channel ADS1299, USB CDC or reliable BLE. Default.
    %     "stm32" - 32-channel STM32 V3 over board USB CDC or E73 BLE.
    %               Fixed 250 Hz and SRB1; no SRB2 or sample-rate control.
    %               Current firmware provides USB CDC and E73 BLE only; the
    %               ESB dongle and dual-mode switching were removed. E73 BLE
    %               requires 2M TX/RX and MTU >= 153. No automatic reconnect.
    %               See TRIGGER.md for tested scope and remaining platform limits.
    %
    %   Board Properties:
    %     Endpoint   - Read-only endpoint used to open the connection.
    %     DeviceType - Read-only "esp32" or "stm32" selected at connect.
    %
    %   Board Methods:
    %     connect   - Open a device and wait for configuration readback.
    %     discover  - List serial ports or scan for BLE devices.
    %     version   - Query binding, SDK version and SDK revision.
    %     snapshot  - Read cached session state and diagnostic counters.
    %     getConfig - Read the cached frontend or STM32 configuration.
    %     configure - Apply and verify device configuration while Ready.
    %     start     - Start acquisition; STM32 waits for the firmware START
    %                 acknowledgement, ESP32 for streaming readiness.
    %     read      - Receive one variable-size sample batch.
    %     readTriggers - Drain available STM32 hardware events independently.
    %     stop      - Stop acquisition while preserving queued samples.
    %     close     - Close the connection and release native resources.
    %     delete    - Perform best-effort cleanup of the handle object.
    %
    %   Example:
    %     ports = omnibci.Board.discover("serial");
    %     % Select the port belonging to your OmniBCI board.
    %     b = omnibci.Board.connect(ports(1), 15, "esp32");
    %     cleanup = onCleanup(@() delete(b));
    %     b.start();
    %     batch = b.read(2);
    %     b.stop();
    %     b.close();
    %
    %   Device failures raise omnibci:Device. Calls requiring an open
    %   connection raise omnibci:Closed after close succeeds. MEX loading
    %   errors are described in the installed package README.
    %
    %   See also omnibci.decodeFrames, omnibci.selftest.
    properties (SetAccess = private)
        %ENDPOINT Connection endpoint as a read-only scalar string.
        %   Uses serial://PORT or the ble:// endpoint returned by discover.
        %   The value is retained after close; it is not a connection status.
        Endpoint (1,1) string
        %DEVICETYPE Selected backend as a read-only scalar string.
        %   "esp32" or "stm32", fixed when the connection is opened.
        DeviceType (1,1) string
    end
    properties (Access = private)
        NativeHandle (1,1) uint64 = uint64(0)
    end
    methods (Access = private)
        function obj = Board(endpoint, handle, deviceType)
            obj.Endpoint = string(endpoint);
            obj.DeviceType = string(deviceType);
            obj.NativeHandle = uint64(handle);
        end
        function handle = requireOpen(obj)
            if obj.NativeHandle == 0
                error('omnibci:Closed', 'Board is closed.');
            end
            handle = obj.NativeHandle;
        end
    end
    methods (Static)
        function obj = connect(endpoint, timeout, deviceType)
            %CONNECT Open an OmniBCI device and wait for configuration readback.
            %   B = omnibci.Board.connect(ENDPOINT) returns a Board handle
            %   for an ESP32 device after it becomes Ready, using a 15-second
            %   timeout.
            %   B = omnibci.Board.connect(ENDPOINT, TIMEOUT) sets a finite
            %   timeout in seconds, with 0 < TIMEOUT <= 60.
            %   B = omnibci.Board.connect(ENDPOINT, TIMEOUT, DEVICETYPE)
            %   selects "esp32" (default) or "stm32".
            %
            %   ENDPOINT is a scalar string such as "serial://COM8" or
            %   "serial:///dev/ttyACM0". For BLE, use the endpoint field
            %   returned by discover(..., DEVICETYPE); do not construct a key
            %   from a MAC address. ESP32 and STM32 use different BLE
            %   services, so the device type must match the discovered board.
            %   STM32 serial endpoints use the board USB CDC port. Close
            %   other applications using the device first.
            %   Invalid endpoints, device types, connection failures and
            %   timeouts raise omnibci:Device.
            %
            %   Example:
            %     b = omnibci.Board.connect("serial://COM8", 15, "esp32");
            %     cleanup = onCleanup(@() delete(b));
            %     disp(b.getConfig());
            %     b.close();
            %
            %   See also omnibci.Board.discover, omnibci.Board.close.
            arguments
                endpoint (1,1) string
                timeout (1,1) double {mustBePositive} = 15
                deviceType (1,1) string {mustBeMember(deviceType, ["esp32", "stm32"])} = "esp32"
            end
            result = native(struct( ...
                'command', 'connect', ...
                'endpoint', char(endpoint), ...
                'timeout', timeout, ...
                'device_type', char(deviceType)));
            obj = omnibci.Board(endpoint, uint64(result.handle), deviceType);
        end
        function devices = discover(transport, timeout, deviceType)
            %DISCOVER List serial ports or scan for OmniBCI BLE devices.
            %   PORTS = omnibci.Board.discover() or discover("serial")
            %   returns a column string array of serial:// endpoints.
            %   Listing a port does not confirm that an OmniBCI board is
            %   attached. Select the correct port before connecting.
            %
            %   DEVICES = omnibci.Board.discover("ble") scans for 10 seconds
            %   using the ESP32 BLE service. DEVICES =
            %   discover("ble", TIMEOUT) sets a finite scan timeout in
            %   seconds, with 0 < TIMEOUT <= 60. DEVICES =
            %   discover("ble", TIMEOUT, DEVICETYPE) selects "esp32"
            %   (default) or "stm32"; the E73 STM32 board uses a different
            %   BLE service and name. TIMEOUT and DEVICETYPE have no effect
            %   on serial port enumeration.
            %
            %   BLE results are structs with endpoint, name, address and
            %   rssi_dbm fields. Optional unavailable values are empty;
            %   no results returns an empty array. Pass endpoint unchanged
            %   to connect with the same DEVICETYPE. RSSI is in dBm.
            %   Discovery does not open a session. Transport failures raise
            %   omnibci:Device.
            %
            %   Example:
            %     devices = omnibci.Board.discover("ble", 10, "stm32");
            %     if ~isempty(devices)
            %         disp(devices(1).endpoint);
            %     end
            %
            %   See also omnibci.Board.connect.
            arguments
                transport (1,1) string {mustBeMember(transport, ["serial", "ble"])} = "serial"
                timeout (1,1) double {mustBePositive} = 10
                deviceType (1,1) string {mustBeMember(deviceType, ["esp32", "stm32"])} = "esp32"
            end
            if transport == "serial"
                ports = native(struct('command', 'serial_ports'));
                devices = "serial://" + string(ports(:));
            else
                devices = native(struct( ...
                    'command', 'discover_ble', ...
                    'timeout', timeout, ...
                    'device_type', char(deviceType)));
            end
        end
        function versions = version()
            %VERSION Return the loaded binding, SDK version and SDK revision.
            %   V = omnibci.Board.version() returns a scalar struct with
            %   character-vector fields binding, sdk and sdk_revision. This
            %   loads the native MEX but does not scan for or connect to
            %   hardware. The version numbers identify separate software
            %   packages; sdk_revision is the exact pinned omnibci-sdk
            %   commit compiled into this binding.
            %
            %   Example:
            %     v = omnibci.Board.version();
            %     disp(v.sdk_revision);
            %
            %   See also omnibci.selftest.
            versions = native(struct('command', 'version'));
        end
    end
    methods
        function snapshot = snapshot(obj)
            %SNAPSHOT Return cached session state and acquisition diagnostics.
            %   S = B.snapshot() returns a scalar struct without requesting
            %   a new device configuration readback. Fields include:
            %     state - Connecting, Ready, Starting, Streaming, Stopping,
            %             Closed or Faulted, as a character vector.
            %     metadata - Legacy firmware, hardware and protocol plus
            %             independent stm32 and e73 component records. Each
            %             component has version, revision, dirty, state and
            %             protocol; unavailable components are empty. The
            %             component state is available, unsupported, timeout
            %             or read_failed.
            %     config - Cached frontend or STM32 configuration, or empty.
            %     generation, samples, missing_samples, crc_errors - Decimal
            %             character vectors preserving full uint64 precision.
            %     ble - received, delivered, crc_bad, duplicates,
            %             out_of_order and gap_markers ESP32 transport
            %             counters. STM32 does not expose BLE transport
            %             metrics, so this field is empty rather than a set
            %             of fabricated zero counters.
            %     error - Session error text, or empty when absent.
            %
            %   Unlike read(), snapshot does not convert generation or sample
            %   counters to uint64. Converting large counters to double can
            %   lose precision. A successfully closed B raises omnibci:Closed.
            %
            %   Example:
            %     s = b.snapshot();
            %     disp(s.state);
            %
            %   See also omnibci.Board.getConfig, omnibci.Board.read.
            snapshot = native(struct('command', 'snapshot', 'handle', obj.requireOpen()));
        end
        function config = getConfig(obj)
            %GETCONFIG Return the cached device configuration.
            %   For an ESP32 board, C is a scalar struct with reference,
            %   mode, enabled_mask, bias_mask, srb2_mask, gains and verified.
            %   For an STM32 board, C has mode, enabled_mask, bias_mask,
            %   loff_mask, gains and verified, with 32 gains. Numeric fields
            %   are MATLAB doubles; verified is logical. See configure for
            %   field meanings and allowed values.
            %
            %   This reads the session snapshot, not a fresh device query.
            %   If no configuration is available, it raises
            %   omnibci:Configuration. A closed B raises omnibci:Closed.
            %
            %   Example:
            %     c = b.getConfig();
            %     disp(c.gains);
            %
            %   See also omnibci.Board.configure, omnibci.Board.snapshot.
            snapshot = obj.snapshot();
            config = snapshot.config;
            if isempty(config)
                error('omnibci:Configuration', 'The SDK has no verified device configuration.');
            end
        end
        function configure(obj, config, timeout)
            %CONFIGURE Apply device settings and verify hardware acknowledgement.
            %   B.configure(C) applies scalar struct C with a 5-second
            %   timeout. B.configure(C, TIMEOUT) sets a finite timeout in
            %   seconds, with 0 < TIMEOUT <= 60. The board must be Ready.
            %
            %   ESP32 (DeviceType "esp32") required fields:
            %     reference    - 0 for SRB1, 1 for SRB2.
            %     mode         - 0 both_bias, 1 eeg, 2 no_bias, 3 shorted,
            %                    or 4 test.
            %     enabled_mask - Integer 0..255 selecting enabled channels.
            %     bias_mask    - Integer 0..255 selecting bias channels.
            %     srb2_mask    - Integer 0..255 selecting SRB2 channels.
            %     gains        - Eight values from [1 2 4 6 8 12 24].
            %   Mask bit 0 selects channel 1, through bit 7 for channel 8.
            %   bias_mask and srb2_mask must be subsets of enabled_mask.
            %
            %   STM32 (DeviceType "stm32") required fields:
            %     mode         - Numeric 0 both_bias, 1 eeg, 2 no_bias,
            %                    3 shorted, or 4 test.
            %     enabled_mask - Integer 0..4294967295; bit 0 is channel 1.
            %     bias_mask    - Integer 0..4294967295, subset of enabled.
            %     loff_mask    - Integer 0..4294967295, subset of enabled;
            %                    lead-off excitation is allowed only in modes
            %                    0..2. Zero disables excitation.
            %     gains        - 32 values from [1 2 4 6 8 12 24].
            %   The STM32 backend is fixed to 250 Hz and SRB1 and has no
            %   SRB2 or sample-rate field. Mode and masks are JSON numbers;
            %   the native bridge and SDK validate range and mask subsets.
            %
            %   The configuration shape must match DeviceType; mixing ESP32
            %   and STM32 fields raises omnibci:Configuration. Missing fields
            %   or invalid gain lists also raise omnibci:Configuration; SDK
            %   validation, invalid state, acknowledgement or timeout
            %   failures raise omnibci:Device. The optional verified field
            %   from getConfig is ignored on input; a successful device
            %   acknowledgement verifies the new settings. A closed B raises
            %   omnibci:Closed.
            %
            %   Example (while Ready):
            %     c = b.getConfig();
            %     c.gains = repmat(12, 1, numel(c.gains));
            %     b.configure(c);
            %     c = b.getConfig();
            %     assert(c.verified);
            %
            %   See also omnibci.Board.getConfig, omnibci.Board.stop.
            arguments
                obj (1,1) omnibci.Board
                config (1,1) struct
                timeout (1,1) double {mustBePositive} = 5
            end
            if obj.DeviceType == "stm32"
                config = validateStm32Config(config);
            else
                validateEsp32Config(config);
            end
            config = rmfieldIfPresent(config, 'verified');
            native(struct('command', 'configure', 'handle', obj.requireOpen(), 'config', config, 'timeout', timeout));
        end
        function start(obj, timeout)
            %START Start acquisition and wait for the firmware START acknowledgement.
            %   B.start() uses a 5-second timeout. B.start(TIMEOUT) sets a
            %   finite timeout in seconds, with 0 < TIMEOUT <= 60.
            %   The board must be Ready; success transitions it to Streaming.
            %   For STM32, success means the firmware acknowledged START (the
            %   SDK's Stm32Started result). It does not wait for the first
            %   sample, which may arrive later; call read to receive samples.
            %   ESP32 start instead returns once streaming readiness is
            %   established. Starting clears queued samples from the
            %   previous acquisition and begins a new generation. Read any
            %   required tail samples before restarting.
            %
            %   Invalid state, transport or timeout failures raise
            %   omnibci:Device. A closed B raises omnibci:Closed.
            %
            %   Example:
            %     b.start();
            %     batch = b.read(2);
            %     b.stop();
            %
            %   See also omnibci.Board.read, omnibci.Board.stop.
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 5
            end
            native(struct('command', 'start', 'handle', obj.requireOpen(), 'timeout', timeout));
        end
        function batch = read(obj, timeout)
            %READ Receive one variable-size batch of acquired samples.
            %   DATA = B.read() waits up to 2 seconds for a queued batch.
            %   DATA = B.read(TIMEOUT) sets a finite wait timeout in seconds,
            %   with 0 < TIMEOUT <= 60. TIMEOUT is not an acquisition duration
            %   or a requested sample count. Already queued data returns
            %   immediately. Read repeatedly while the board is streaming.
            %
            %   DATA is a scalar struct. For N samples and the selected
            %   backend's channel count C (8 ESP32, 32 STM32), its fields are:
            %     eeg_uv         - N-by-C single, scaled channel microvolts.
            %                      Invalid STM32 frames contain NaN.
            %     raw_counts     - N-by-C int32, signed ADC counts.
            %     sequence       - N-by-1 uint32, frame sequence numbers.
            %     valid          - N-by-1 logical, SDK frame validity flags.
            %     mode           - N-by-1 uint8, frame acquisition modes.
            %     status         - N-by-S uint8 where S is 3 for ESP32 and
            %                      12 for STM32 (four genuine ADS1299 status
            %                      words from frame bytes 12-23). The bytes
            %                      are passed through, not reinterpreted.
            %     sample_indices - N-by-1 uint64, sequence-derived indices;
            %                      missing samples leave gaps in this axis.
            %     generation     - uint64 scalar identifying the acquisition.
            %     sample_rate_hz - Sampling frequency in Hz (currently 250).
            %     sample_time_s  - N-by-1 double, sample_indices/rate in seconds.
            %     received_age_s - Seconds since SDK host delivery of this
            %                      batch, measured when the request is handled.
            %     source_timestamp_us - N-by-1 uint32 device DRDY microseconds,
            %                      modulo 2^32; STM32 only when valid below.
            %     source_timestamp_valid - N-by-1 logical availability mask.
            %                      Zero with true validity is a wrapped time;
            %                      false denotes unavailable time (e.g. ESP32).
            %     channel_count  - Scalar double, C above.
            %     status_bytes   - Scalar double, S above.
            %   sample_time_s and received_age_s are not hardware timestamps.
            %   source_timestamp_us is device time, not UTC or host time.
            %   N may be zero, one or many; empty batches keep C and S columns.
            %
            %   After stop(), read can drain remaining queued batches. Once
            %   empty, it raises a timeout; it does not return an end marker.
            %   Timeouts, disconnection and queue overflow raise
            %   omnibci:Device. A closed B raises omnibci:Closed.
            %
            %   Example (after start):
            %     batch = b.read(2);
            %     plot(batch.sample_time_s, batch.eeg_uv(:, 1));
            %
            %   See also omnibci.Board.start, omnibci.Board.stop,
            %       omnibci.Board.snapshot, omnibci.decodeFrames.
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 2
            end
            batch = normalizeBatch(native(struct('command', 'read', 'handle', obj.requireOpen(), 'timeout', timeout)));
        end
        function events = readTriggers(obj)
            %READTRIGGERS Drain available STM32 events independently of EEG reads.
            % Nonblocking, 16384-event bounded queue; overflow raises an error.
            % Stop preserves events; successful start clears the previous generation.
            events = normalizeTriggers(native(struct('command','read_triggers','handle',obj.requireOpen())));
        end
        function stop(obj, timeout)
            %STOP Stop acquisition while preserving queued samples.
            %   B.stop() uses a 5-second timeout. B.stop(TIMEOUT) sets a
            %   finite timeout in seconds, with 0 < TIMEOUT <= 60.
            %   Waits for the SDK stop procedure to finish and return to
            %   Ready. Calling stop while already Ready succeeds.
            %   Remaining batches can be retrieved with read; a subsequent
            %   start clears them. The device connection remains open.
            %
            %   Invalid state, transport or timeout failures raise
            %   omnibci:Device. A closed B raises omnibci:Closed.
            %
            %   Example:
            %     b.stop();
            %     disp(b.snapshot());
            %
            %   See also omnibci.Board.read, omnibci.Board.start,
            %       omnibci.Board.close.
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 5
            end
            native(struct('command', 'stop', 'handle', obj.requireOpen(), 'timeout', timeout));
        end
        function close(obj, timeout)
            %CLOSE Close the device connection and release native resources.
            %   B.close() uses a 6-second timeout. B.close(TIMEOUT) sets a
            %   finite timeout in seconds, with 0 < TIMEOUT <= 60.
            %   Stops active acquisition and waits for worker shutdown.
            %   To retain tail data, call stop and read before close.
            %
            %   For STM32, close first stops acquisition. If the current
            %   configuration has nonzero loff_mask, close submits a new
            %   configuration with loff_mask zero and requires the SDK to
            %   confirm it. If that confirmation or the stop cannot be
            %   obtained, close raises omnibci:Device and the caller must
            %   not assume lead-off excitation is off. A failed Disconnect
            %   stop is escalated through a bounded Shutdown cleanup; any
            %   cleanup failure is reported rather than silently ignored.
            %   STM32 does not auto-reconnect after a disconnect.
            %
            %   After success the handle is closed; repeated close calls do
            %   nothing. Other device operations then raise omnibci:Closed.
            %   Failures raise omnibci:Device and retain the native handle
            %   so cleanup can be retried. Use explicit close to observe
            %   cleanup errors; delete suppresses them.
            %
            %   Example:
            %     b.stop();
            %     b.close();
            %
            %   See also omnibci.Board.stop, omnibci.Board.delete.
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 6
            end
            if obj.NativeHandle ~= 0
                native(struct('command', 'close', 'handle', obj.NativeHandle, 'timeout', timeout));
                obj.NativeHandle = uint64(0);
            end
        end
        function delete(obj)
            %DELETE Perform best-effort cleanup when destroying a Board.
            %   delete(B) releases the native handle even after a fault and
            %   suppresses cleanup errors. Call B.close() explicitly when
            %   the application must detect shutdown failures, including a
            %   failed STM32 stop or unconfirmed LOFF excitation shutdown.
            %
            %   Example:
            %     b = omnibci.Board.connect("serial://COM8");
            %     cleanup = onCleanup(@() delete(b));
            %
            %   See also omnibci.Board.close, onCleanup.
            if obj.NativeHandle ~= 0
                handle = obj.NativeHandle;
                obj.NativeHandle = uint64(0);
                try
                    native(struct('command', 'dispose', 'handle', handle, 'timeout', 6));
                catch
                    % Destructors cannot report cleanup errors. Call close explicitly.
                end
            end
        end
    end
end

function config = rmfieldIfPresent(config, field)
if isfield(config, field)
    config = rmfield(config, field);
end
end

function validateEsp32Config(config)
if isfield(config, 'loff_mask')
    error('omnibci:Configuration', 'loff_mask is only valid for STM32 devices.');
end
required = {'reference', 'mode', 'enabled_mask', 'bias_mask', 'srb2_mask', 'gains'};
if ~all(isfield(config, required))
    error('omnibci:Configuration', ...
        'ESP32 configuration must have reference, mode, masks, and gains.');
end
if numel(config.gains) ~= 8 || ~all(ismember(config.gains, [1 2 4 6 8 12 24]))
    error('omnibci:Configuration', 'gains must contain eight valid ADS1299 gains.');
end
end

function config = validateStm32Config(config)
if any(isfield(config, {'reference', 'srb2_mask'}))
    error('omnibci:Configuration', ...
        'STM32 configuration must not contain ESP32 reference or srb2_mask fields.');
end
required = {'mode', 'enabled_mask', 'bias_mask', 'loff_mask', 'gains'};
if ~all(isfield(config, required))
    error('omnibci:Configuration', ...
        'STM32 configuration must have numeric mode, enabled_mask, bias_mask, loff_mask, and 32 gains.');
end
if ~isnumeric(config.gains) || numel(config.gains) ~= 32 || ...
        ~all(ismember(config.gains, [1 2 4 6 8 12 24]))
    error('omnibci:Configuration', 'gains must contain 32 valid STM32 gains.');
end
if ~isscalar(config.mode) || ~isnumeric(config.mode) || ~isreal(config.mode) || ...
        ~isfinite(config.mode) || config.mode < 0 || config.mode > 4 || ...
        config.mode ~= floor(config.mode)
    error('omnibci:Configuration', 'mode must be a numeric integer from 0 to 4.');
end
validateStm32Mask(config.enabled_mask, 'enabled_mask');
validateStm32Mask(config.bias_mask, 'bias_mask');
validateStm32Mask(config.loff_mask, 'loff_mask');
% Force exact JSON integers for the uint32 masks.
config.enabled_mask = uint32(config.enabled_mask);
config.bias_mask = uint32(config.bias_mask);
config.loff_mask = uint32(config.loff_mask);
end

function validateStm32Mask(value, name)
if ~isscalar(value) || ~isnumeric(value) || ~isreal(value) || ~isfinite(value) || ...
        value < 0 || value > 4294967295 || value ~= floor(value)
    error('omnibci:Configuration', ...
        '%s must be an integer from 0 to 4294967295.', name);
end
end
