function test_ble_lifetime(endpoint, deviceType)
%TEST_BLE_LIFETIME Explicit hardware regression for MEX unload during BLE work.
%   Run in a separate MATLAB process and require a successful process exit.
%   This starts acquisition on the explicitly supplied test board.
arguments
    endpoint (1,1) string
    deviceType (1,1) string {mustBeMember(deviceType, ["esp32", "stm32"])}
end
assert(ispc, 'This regression checks the Windows MEX lifetime guard.');
assert(startsWith(endpoint, "ble://"));
board = omnibci.Board.connect(endpoint, 30, deviceType);
cleanup = onCleanup(@() delete(board));
board.start();
count = 0;
expectedWidth = 8;
if deviceType == "stm32"
    expectedWidth = 32;
end
while count < 250
    batch = board.read(15);
    assert(size(batch.eeg_uv, 2) == expectedWidth);
    assert(all(batch.source_timestamp_valid));
    count = count + size(batch.eeg_uv, 1);
end
% An active worker and a deferred OS close must never execute unloaded code.
clear mex
board.stop();
board.close();
delete(board);
clear cleanup
clear mex
versions = omnibci.Board.version();
assert(strcmp(versions.sdk, '0.3.1'));
fprintf('%s BLE acquisition and immediate MEX clear passed.\n', deviceType);
end
