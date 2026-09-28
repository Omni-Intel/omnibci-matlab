function results = test_hardware(transports)
%TEST_HARDWARE Exercise USB and BLE against a connected OmniBCI board.
% Requires the board on COM8 and a discoverable BLE device named OmniBCI.
arguments
    transports (1,:) string = ["usb", "ble"]
end
results = struct();
if any(transports == "usb")
    results.usb = acquireFor('serial://COM8');
    disp(results.usb);
end
if any(transports == "ble")
    devices = omnibci.Board.discover("ble", 10);
    names = string({devices.name});
    match = find(names == "OmniBCI", 1);
    assert(~isempty(match), 'No OmniBCI BLE device was discovered.');
    results.ble = acquireFor(devices(match).endpoint);
    disp(results.ble);
end
disp(results);
end

function result = acquireFor(endpoint)
board = omnibci.Board.connect(string(endpoint), 20);
cleanup = onCleanup(@() delete(board));
initial = board.snapshot();
assert(strcmp(initial.state, 'Ready'));
config = board.getConfig();
assert(config.verified);
board.configure(config, 8); % Confirm write/readback of the current settings.
assert(board.getConfig().verified);

board.start(10);
count = 0;
valid = 0;
first = uint64(0);
last = uint64(0);
started = tic;
while toc(started) < 3
    batch = board.read(2);
    assert(size(batch.eeg_uv, 2) == 8);
    assert(size(batch.raw_counts, 2) == 8);
    assert(size(batch.status, 2) == 3);
    assert(numel(batch.sequence) == size(batch.eeg_uv, 1));
    if count == 0
        first = batch.sample_indices(1);
    end
    assert(all(diff(double(batch.sample_indices)) >= 0));
    last = batch.sample_indices(end);
    count = count + size(batch.eeg_uv, 1);
    valid = valid + sum(batch.valid);
end
board.stop(10);
final = board.snapshot();
assert(strcmp(final.state, 'Ready'));
assert(count >= 250, 'Too few samples were received in three seconds.');
result = struct('samples', count, 'valid', valid, 'first_index', first, ...
    'last_index', last, 'missing_samples', final.missing_samples, ...
    'crc_errors', final.crc_errors, 'state', final.state);
board.close(10);
end
