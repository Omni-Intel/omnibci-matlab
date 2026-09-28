function batch = decodeFrames(bytes, gains)
%DECODEFRAMES Decode a complete USB frame buffer using the Rust SDK.
% Incomplete trailing frames are not retained between calls.
arguments
    bytes (1,:) uint8
    gains (1,8) double = repmat(24, 1, 8)
end
request = struct('command', 'decode_frames', 'bytes', double(bytes), 'gains', gains);
batch = normalizeBatch(native(request));
end
