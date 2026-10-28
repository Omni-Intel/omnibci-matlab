function acquire_stm32(endpoint)
%ACQUIRE_STM32 Acquire 32-channel STM32 data over board USB CDC or E73 BLE.
%   ACQUIRE_STM32() scans for an STM32/E73 BLE device and acquires ten
%   batches from the first result.
%   ACQUIRE_STM32(ENDPOINT) connects to ENDPOINT instead, for example
%   "serial://COM5" or a ble:// endpoint returned by omnibci.Board.discover.
%
%   Add <repository>/matlab to the MATLAB path before calling this function.
%   DeviceType is explicit because ESP32 and STM32 use different BLE
%   services. Each batch has 32 channels and 12 genuine ADS1299 status bytes
%   (batch.status is N-by-12). This is a function rather than a script so
%   that the onCleanup object is destroyed when an error unwinds the call
%   stack. The success path still calls close explicitly so cleanup failures
%   are reported.
arguments
    endpoint (1,1) string = ""
end
deviceType = "stm32";

addpath(fullfile(fileparts(fileparts(mfilename('fullpath'))), 'matlab'));

if strlength(endpoint) == 0
    devices = omnibci.Board.discover("ble", 10, deviceType);
    if isempty(devices)
        error('omnibci:Example', 'No STM32/E73 BLE device found; pass a serial endpoint instead.');
    end
    % Select the intended board when more than one is listed.
    endpoint = string(devices(1).endpoint);
    disp(devices);
end

board = omnibci.Board.connect(endpoint, 20, deviceType);
cleanup = onCleanup(@() delete(board)); %#ok<NASGU>
assert(board.DeviceType == "stm32");
disp(board.snapshot());
disp(board.getConfig());

% STM32 is fixed to 250 Hz and SRB1; configure only while Ready.
board.start();
for k = 1:10
    batch = board.read(2);
    count = size(batch.eeg_uv, 1);
    if count == 0
        fprintf('empty batch; channel count %d, status bytes %d\n', ...
            batch.channel_count, batch.status_bytes);
    else
        fprintf('%d samples x %d channels; first index %s\n', ...
            count, batch.channel_count, string(batch.sample_indices(1)));
    end
end
board.stop();
board.close();
end
