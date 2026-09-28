function versions = selftest()
%SELFTEST Check MEX loading and offline decoding without connecting to hardware.
%   versions = omnibci.selftest() returns the binding and SDK versions on success.
versions = omnibci.Board.version();
assert(isfield(versions, 'binding') && isfield(versions, 'sdk'));
assert(~isempty(versions.binding) && ~isempty(versions.sdk));
empty = omnibci.decodeFrames(uint8([]));
assert(isequal(size(empty.eeg_uv), [0 8]));
assert(isequal(size(empty.raw_counts), [0 8]));

frame = zeros(1, 48, 'uint8');
frame(1:4) = uint8([165 90 1 1]);
frame(5:8) = uint8([17 0 0 0]);
frame(13:15) = uint8([192 0 1]);
frame(16) = uint8(3);
counts = int32([0 1 -1 8388607 -8388608 42 -42 1234]);
for channel = 1:8
    value = bitand(counts(channel), int32(hex2dec('FFFFFF')));
    frame(17 + 3*(channel-1):19 + 3*(channel-1)) = uint8([
        bitand(bitshift(value, -16), 255), ...
        bitand(bitshift(value, -8), 255), ...
        bitand(value, 255)]);
end
frame(44) = uint8(1);
crc = uint16(65535);
for byte = frame(1:46)
    crc = bitxor(crc, bitshift(uint16(byte), 8));
    for bit = 1:8 %#ok<FXSET>
        if bitand(crc, uint16(32768))
            crc = bitxor(bitshift(crc, 1), uint16(4129));
        else
            crc = bitshift(crc, 1);
        end
    end
end
frame(47:48) = uint8([bitand(crc, 255), bitshift(crc, -8)]);
batch = omnibci.decodeFrames(frame);
assert(isequal(batch.raw_counts, counts));
assert(isequal(batch.sequence, uint32(17)));
assert(isequal(batch.status, uint8([192 0 1])));
assert(batch.valid);
expected = double(counts) * (4.5e6 / (8388607 * 24));
assert(max(abs(double(batch.eeg_uv) - expected)) < 0.05);

invalid = frame;
invalid(21) = bitxor(invalid(21), uint8(1));
bad = omnibci.decodeFrames(invalid);
assert(isempty(bad.sequence));
assert(bad.crc_errors == 1);

try
    omnibci.Board.connect("invalid");
    error('test:ExpectedFailure', 'Invalid endpoint was accepted');
catch exception
    assert(strcmp(exception.identifier, 'omnibci:Device'));
end
fprintf('OmniBCI MATLAB offline tests passed.\n');
end
