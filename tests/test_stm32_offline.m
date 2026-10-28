function test_stm32_offline()
%TEST_STM32_OFFLINE Offline checks for the STM32 backend adaptation.
%   No board, serial port or BLE adapter is opened. Hardware acquisition is
%   intentionally not covered by this file; run examples/acquire_stm32.m
%   manually after the binding has been built and an actual device is
%   available. STM32 Board.read batches report 32 channels and the 12
%   genuine ADS1299 status bytes from each V3 frame; that path cannot be
%   exercised without hardware.
versions = omnibci.Board.version();
assert(isfield(versions, 'sdk_revision'));
assert(strcmp(versions.sdk_revision, '54fbad405c05bfef813f7acb323e5f0c73153092'));

% decodeFrames must remain the backwards-compatible eight-channel path.
batch = omnibci.decodeFrames(uint8([]));
assert(isequal(size(batch.eeg_uv), [0 8]));
assert(isequal(size(batch.raw_counts), [0 8]));
assert(isequal(size(batch.status), [0 3]));
assert(batch.channel_count == 8 && batch.status_bytes == 3);

% Unknown device types are rejected before any device is opened.
failed = false;
try
    omnibci.Board.connect("serial://COM1", 1, "invalid");
catch exception
    failed = ~strcmp(exception.identifier, 'test:ExpectedFailure');
end
assert(failed, 'An unknown device type was accepted');

% A malformed endpoint is rejected before the STM32 transport is started.
failed = false;
try
    omnibci.Board.connect("invalid", 1, "stm32");
catch exception
    failed = strcmp(exception.identifier, 'omnibci:Device');
end
assert(failed, 'A malformed STM32 endpoint was accepted');

fprintf('OmniBCI MATLAB STM32 offline checks passed.\n');
end
