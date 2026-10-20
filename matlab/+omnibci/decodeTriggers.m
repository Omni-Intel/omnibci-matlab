function events = decodeTriggers(bytes)
%DECODETRIGGERS Decode complete STM32 hardware event packets for offline checks.
% Each call uses a fresh parser. Partial trailing packets are not retained.
arguments
    bytes (1,:) uint8
end
events = normalizeTriggers(native(struct('command','decode_triggers','bytes',double(bytes))));
end
