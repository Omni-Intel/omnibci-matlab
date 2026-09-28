function batch = decodeFrames(bytes, gains)
%DECODEFRAMES Decode complete OmniBCI USB frames without connecting to hardware.
%   DATA = omnibci.decodeFrames(BYTES) decodes a uint8 row vector using a
%   gain of 24 for all eight channels. BYTES may contain multiple frames.
%   DATA = omnibci.decodeFrames(BYTES, GAINS) uses a 1-by-8 double gain vector
%   whose entries are selected from [1 2 4 6 8 12 24]. GAINS must match the
%   device settings used when capturing BYTES; it controls voltage scaling.
%
%   DATA is a scalar struct. For N decoded frames its fields are:
%     eeg_uv     - N-by-8 single, channel voltages in microvolts.
%     raw_counts - N-by-8 int32, signed ADC counts.
%     sequence   - N-by-1 uint32, frame sequence numbers.
%     valid      - N-by-1 logical, SDK frame validity flags.
%     mode       - N-by-1 uint8, frame acquisition modes.
%     status     - N-by-3 uint8, ADS status bytes.
%     crc_errors - Number of CRC failures detected in this call.
%     sync_drop  - Number of bytes discarded while finding frame boundaries.
%   Unlike Board.read, this function does not return acquisition generation,
%   sample indices or timing fields. An empty input produces empty sample
%   arrays with the listed column counts.
%
%   Each call creates a fresh parser: incomplete trailing frames are not
%   retained between calls. CRC failures are reported in crc_errors and
%   corrupt frames are excluded from the output. This function accepts USB
%   protocol bytes, not BLE notification transport packets.
%   Unsupported gains raise omnibci:Device; argument type and shape errors
%   are reported by MATLAB argument validation.
%
%   Example (read a previously captured binary USB stream):
%     fid = fopen('capture.bin', 'rb');
%     assert(fid ~= -1, 'Cannot open capture.bin');
%     cleanup = onCleanup(@() fclose(fid));
%     bytes = fread(fid, Inf, '*uint8').';
%     data = omnibci.decodeFrames(bytes, repmat(24, 1, 8));
%     disp(data.crc_errors);
%
%   See also omnibci.Board.read, omnibci.selftest.
arguments
    bytes (1,:) uint8
    gains (1,8) double = repmat(24, 1, 8)
end
request = struct('command', 'decode_frames', 'bytes', double(bytes), 'gains', gains);
batch = normalizeBatch(native(request));
end
