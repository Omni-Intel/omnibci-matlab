function test_trigger_timing()
root = fileparts(mfilename('fullpath'));
f = fopen(fullfile(root,'fixtures','stm32-trigger-timing.bin'),'rb');
assert(f >= 0); cleanup = onCleanup(@() fclose(f));
bytes = fread(f,Inf,'*uint8').';
events = omnibci.decodeTriggers(bytes);
assert(numel(events) == 4);
assert(isequal([events.code],uint16([0 7 7 255])));
assert(all([events.sequence] == uint32(0)));
offsets = uint64([1234 1000 500 1]);
base = bitshift(uint64(1),53) + bitshift(uint64(1),32) + uint64(2000);
for k = 1:4
    assert(isa(events(k).timing.trigger_timestamp_us,'uint64'));
    assert(events(k).timing.trigger_timestamp_us == base - offsets(k));
    assert(events(k).timing.anchor_timestamp_us == uint32(2000));
end
bytes(end) = bitxor(bytes(end),uint8(1));
assert(numel(omnibci.decodeTriggers(bytes)) == 3);
fprintf('Exact uint64 trigger timestamps, repeated codes and CRC checks passed.\n');
end
