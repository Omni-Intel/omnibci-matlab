classdef Board < handle
    %BOARD Synchronous ESP32/ADS1299 acquisition through omnibci-sdk.
    properties (SetAccess = private)
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
            arguments
                endpoint (1,1) string
                timeout (1,1) double {mustBePositive} = 15
            end
            result = native(struct('command', 'connect', 'endpoint', char(endpoint), 'timeout', timeout));
            obj = omnibci.Board(endpoint, uint64(result.handle));
        end
        function devices = discover(transport, timeout)
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
            versions = native(struct('command', 'version'));
        end
    end
    methods
        function snapshot = snapshot(obj)
            snapshot = native(struct('command', 'snapshot', 'handle', obj.requireOpen()));
        end
        function config = getConfig(obj)
            snapshot = obj.snapshot();
            config = snapshot.config;
            if isempty(config)
                error('omnibci:Configuration', 'The SDK has no verified device configuration.');
            end
        end
        function configure(obj, config, timeout)
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
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 5
            end
            native(struct('command', 'start', 'handle', obj.requireOpen(), 'timeout', timeout));
        end
        function batch = read(obj, timeout)
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 2
            end
            batch = normalizeBatch(native(struct('command', 'read', 'handle', obj.requireOpen(), 'timeout', timeout)));
        end
        function stop(obj, timeout)
            arguments
                obj (1,1) omnibci.Board
                timeout (1,1) double {mustBePositive} = 5
            end
            native(struct('command', 'stop', 'handle', obj.requireOpen(), 'timeout', timeout));
        end
        function close(obj, timeout)
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
