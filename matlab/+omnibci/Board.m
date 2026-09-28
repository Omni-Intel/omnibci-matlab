classdef Board < handle
    %BOARD Acquire eight-channel ESP32/ADS1299 data over USB CDC or BLE.
    %   B = omnibci.Board.connect(ENDPOINT) opens a device and returns a
    %   handle object. Use connect rather than calling the constructor.
    %   Methods block until completion or timeout; timeouts are in seconds.
    %   Keep reading during acquisition to avoid overflowing the SDK queue.
    %   Only one controlling process may own the device at a time.
    %
    %   Board Properties:
    %     Endpoint - Read-only endpoint used to open the connection.
    %
    %   Board Methods:
    %     connect   - Open a device and wait for configuration readback.
    %     discover  - List serial ports or scan for BLE devices.
    %     version   - Query MATLAB binding and Rust SDK versions.
    %     snapshot  - Read cached session state and diagnostic counters.
    %     getConfig - Read the cached frontend configuration.
    %     configure - Apply and verify frontend configuration while Ready.
    %     start     - Start acquisition and wait for the first samples.
    %     read      - Receive one variable-size sample batch.
    %     stop      - Stop acquisition while preserving queued samples.
    %     close     - Close the connection and release native resources.
    %     delete    - Perform best-effort cleanup of the handle object.
    %
    %   Example:
    %     ports = omnibci.Board.discover("serial");
    %     % Select the port belonging to your OmniBCI board.
    %     b = omnibci.Board.connect(ports(1));
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
    end
    properties (Access = private)
        NativeHandle (1,1) uint64 = uint64(0)
    end
    methods (Access = private)
        function obj = Board(endpoint, handle)
            obj.Endpoint = string(endpoint);
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
        function obj = connect(endpoint, timeout)
            %CONNECT Open an OmniBCI device and wait for configuration readback.
            %   B = omnibci.Board.connect(ENDPOINT) returns a Board handle
            %   after the device becomes Ready, using a 15-second timeout.
            %   B = omnibci.Board.connect(ENDPOINT, TIMEOUT) sets a finite
            %   timeout in seconds, with 0 < TIMEOUT <= 60.
            %
            %   ENDPOINT is a scalar string such as "serial://COM8" or
            %   "serial:///dev/ttyACM0". For BLE, use the endpoint field
            %   returned by discover("ble"); do not construct a key from a
            %   MAC address. Close other applications using the device first.
            %   Invalid endpoints, connection failures and timeouts raise
            %   omnibci:Device.
            %
            %   Example:
            %     b = omnibci.Board.connect("serial://COM8", 15);
            %     cleanup = onCleanup(@() delete(b));
            %     disp(b.getConfig());
            %     b.close();
            %
            %   See also omnibci.Board.discover, omnibci.Board.close.
            arguments
                endpoint (1,1) string
                timeout (1,1) double {mustBePositive} = 15
            end
            result = native(struct('command', 'connect', 'endpoint', char(endpoint), 'timeout', timeout));
            obj = omnibci.Board(endpoint, uint64(result.handle));
        end
        function devices = discover(transport, timeout)
            %DISCOVER List serial ports or scan for OmniBCI BLE devices.
            %   PORTS = omnibci.Board.discover() or discover("serial")
            %   returns a column string array of serial:// endpoints.
            %   Listing a port does not confirm that an OmniBCI board is
            %   attached. Select the correct port before connecting.
            %
            %   DEVICES = omnibci.Board.discover("ble") scans for 10 seconds.
            %   DEVICES = omnibci.Board.discover("ble", TIMEOUT) uses a
            %   finite scan timeout in seconds, with 0 < TIMEOUT <= 60.
            %   TIMEOUT has no effect on serial port enumeration.
            %
            %   BLE results are structs with endpoint, name, address and
            %   rssi_dbm fields. Optional unavailable values are empty;
            %   no results returns an empty array. Pass endpoint unchanged
            %   to connect. RSSI is in dBm. Discovery does not open a session.
            %   Transport failures raise omnibci:Device.
            %
            %   Example:
            %     devices = omnibci.Board.discover("ble", 10);
            %     if ~isempty(devices)
            %         disp(devices(1).endpoint);
            %     end
            %
            %   See also omnibci.Board.connect.
            arguments
                transport (1,1) string {mustBeMember(transport, ["serial", "ble"])} = "serial"
                timeout (1,1) double {mustBePositive} = 10
            end
            if transport == "serial"
                ports = native(struct('command', 'serial_ports'));
                devices = "serial://" + string(ports(:));
            else
                devices = native(struct('command', 'discover_ble', 'timeout', timeout));
            end
        end
        function versions = version()
            %VERSION Return the loaded MATLAB binding and Rust SDK versions.
            %   V = omnibci.Board.version() returns a scalar struct with
            %   character-vector fields binding and sdk. This loads the
            %   native MEX but does not scan for or connect to hardware.
            %   The two version numbers identify separate software packages.
            %
            %   Example:
            %     v = omnibci.Board.version();
            %     disp(v.binding);
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
            %     metadata - firmware, hardware and protocol, or empty.
            %     config - Cached frontend configuration, or empty.
            %     generation, samples, missing_samples, crc_errors - Decimal
            %             character vectors preserving full uint64 precision.
            %     ble - received, delivered, crc_bad, duplicates,
            %             out_of_order and gap_markers transport counters.
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
            %GETCONFIG Return the cached frontend configuration.
            %   C = B.getConfig() returns a scalar struct with reference,
            %   mode, enabled_mask, bias_mask, srb2_mask, gains and verified.
            %   Numeric fields are MATLAB doubles; verified is logical.
            %   See configure for field meanings and allowed values.
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
            %CONFIGURE Apply frontend settings and verify device readback.
            %   B.configure(C) applies scalar struct C with a 5-second
            %   timeout. B.configure(C, TIMEOUT) sets a finite timeout in
            %   seconds, with 0 < TIMEOUT <= 60. The board must be Ready.
            %
            %   Required fields in C:
            %     reference    - 0 for SRB1, 1 for SRB2.
            %     mode         - 0 both_bias, 1 eeg, 2 no_bias, 3 shorted,
            %                    or 4 test.
            %     enabled_mask - Integer 0..255 selecting enabled channels.
            %     bias_mask    - Integer 0..255 selecting bias channels.
            %     srb2_mask    - Integer 0..255 selecting SRB2 channels.
            %     gains        - Eight values from [1 2 4 6 8 12 24].
            %   Mask bit 0 selects channel 1, through bit 7 for channel 8.
            %   bias_mask and srb2_mask must be subsets of enabled_mask.
            %   The optional verified field from getConfig is ignored on
            %   input; successful device readback verifies the new settings.
            %
            %   Missing fields or invalid gain lists raise
            %   omnibci:Configuration; other SDK validation, invalid state,
            %   readback or timeout failures raise omnibci:Device.
            %   A closed B raises omnibci:Closed.
            %
            %   Example (while Ready):
            %     c = b.getConfig();
            %     c.gains = repmat(12, 1, 8);
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
            required = {'reference', 'mode', 'enabled_mask', 'bias_mask', 'srb2_mask', 'gains'};
            if ~all(isfield(config, required))
                error('omnibci:Configuration', 'Configuration must have reference, mode, masks, and gains.');
            end
            if numel(config.gains) ~= 8 || ~all(ismember(config.gains, [1 2 4 6 8 12 24]))
                error('omnibci:Configuration', 'gains must contain eight valid ADS1299 gains.');
            end
            config = rmfieldIfPresent(config, 'verified');
            native(struct('command', 'configure', 'handle', obj.requireOpen(), 'config', config, 'timeout', timeout));
        end
        function start(obj, timeout)
            %START Start acquisition and wait for the first received samples.
            %   B.start() uses a 5-second timeout. B.start(TIMEOUT) sets a
            %   finite timeout in seconds, with 0 < TIMEOUT <= 60.
            %   The board must be Ready; success transitions it to Streaming.
            %   Starting clears queued samples from the previous acquisition
            %   and begins a new generation. Read any required tail samples
            %   before restarting.
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
            %   DATA is a scalar struct. For N samples, its fields are:
            %     eeg_uv         - N-by-8 single, scaled channel microvolts.
            %     raw_counts     - N-by-8 int32, signed ADC counts.
            %     sequence       - N-by-1 uint32, frame sequence numbers.
            %     valid          - N-by-1 logical, SDK frame validity flags.
            %     mode           - N-by-1 uint8, frame acquisition modes.
            %     status         - N-by-3 uint8, ADS status bytes.
            %     sample_indices - N-by-1 uint64, sequence-derived indices;
            %                      missing samples leave gaps in this axis.
            %     generation     - uint64 scalar identifying the acquisition.
            %     sample_rate_hz - Sampling frequency in Hz (currently 250).
            %     sample_time_s  - N-by-1 double, sample_indices/rate in seconds.
            %     received_age_s - Seconds since SDK host delivery of this
            %                      batch, measured when the request is handled.
            %   Neither time field is an absolute hardware timestamp.
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
            %   delete(B) calls close if B still has a native handle and
            %   suppresses cleanup errors. Call B.close() explicitly when
            %   the application must detect shutdown failures.
            %
            %   Example:
            %     b = omnibci.Board.connect("serial://COM8");
            %     cleanup = onCleanup(@() delete(b));
            %
            %   See also omnibci.Board.close, onCleanup.
            if obj.NativeHandle ~= 0
                try
                    obj.close();
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
