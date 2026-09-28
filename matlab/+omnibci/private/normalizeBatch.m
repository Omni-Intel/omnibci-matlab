function batch = normalizeBatch(batch)
%NORMALIZEBATCH Give empty and nonempty batches consistent MATLAB shapes/types.
if isempty(batch.eeg_uv)
    batch.eeg_uv = zeros(0, 8, 'single');
    batch.raw_counts = zeros(0, 8, 'int32');
    batch.status = zeros(0, 3, 'uint8');
else
    batch.eeg_uv = single(batch.eeg_uv);
    batch.raw_counts = int32(batch.raw_counts);
    batch.status = uint8(batch.status);
end
batch.sequence = uint32(batch.sequence(:));
batch.valid = logical(batch.valid(:));
batch.mode = uint8(batch.mode(:));
if isfield(batch, 'sample_indices')
    batch.sample_indices = parseUint64Decimal(batch.sample_indices(:));
    batch.generation = parseUint64Decimal(batch.generation);
    batch.sample_time_s = double(batch.sample_indices) ./ double(batch.sample_rate_hz);
end
end

function numbers = parseUint64Decimal(values)
% MATLAB R2023a does not cast decimal strings directly to uint64.
values = string(values);
numbers = zeros(size(values), 'uint64');
for k = 1:numel(values)
    digits = char(values(k));
    if isempty(digits) || any(digits < '0' | digits > '9')
        error('omnibci:InvalidData', 'Invalid uint64 value returned by the SDK.');
    end
    number = uint64(0);
    for digit = digits
        number = number * uint64(10) + uint64(digit - '0');
    end
    numbers(k) = number;
end
end
