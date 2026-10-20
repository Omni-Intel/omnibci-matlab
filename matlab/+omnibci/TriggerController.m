classdef TriggerController < handle
    %TRIGGERCONTROLLER Serial dispatch through the shared Rust SDK.
    % Success means host dispatch only, not receipt by the EEG device.
    % Read Board.readTriggers for device trigger-start timestamps.
    properties (SetAccess = private)
        Port (1,1) string
    end
    properties (Access = private)
        NativeHandle (1,1) uint64 = uint64(0)
    end
    methods (Access = private)
        function obj = TriggerController(port, handle)
            obj.Port = string(port);
            obj.NativeHandle = uint64(handle);
        end
    end
    methods (Static)
        function obj = connect(port, timeout)
            if nargin < 2, timeout = 1; end
            port = string(port);
            if ~isscalar(port) || ismissing(port) || strlength(strtrim(port)) == 0
                error('omnibci:InvalidPort', 'Port must be a nonempty scalar string.');
            end
            validateattributes(timeout, {'numeric'}, {'scalar','real','finite','positive','<=',60});
            value = native(struct('command','trigger_connect','endpoint',char(port),'timeout',double(timeout)));
            obj = omnibci.TriggerController(port, value.handle);
        end
        function ports = availablePorts()
            % Enumerates candidates, not verified trigger generators.
            value = native(struct('command','trigger_ports'));
            ports = string(value(:));
        end
    end
    methods
        function send(obj, code)
            validateattributes(code, {'numeric'}, {'scalar','real','finite','integer','>=',0,'<=',255});
            if obj.NativeHandle == 0
                error('omnibci:Closed', 'Trigger controller is closed.');
            end
            native(struct('command','trigger_send','handle',obj.NativeHandle,'code',double(code)));
        end
        function value = isOpen(obj)
            value = obj.NativeHandle ~= 0 && native(struct('command','trigger_is_open','handle',obj.NativeHandle));
        end
        function close(obj)
            if obj.NativeHandle ~= 0
                native(struct('command','trigger_close','handle',obj.NativeHandle));
                obj.NativeHandle = uint64(0);
            end
        end
        function delete(obj)
            try
                obj.close();
            catch
                % Explicit close reports cleanup errors; destructors are best effort.
            end
        end
    end
end
