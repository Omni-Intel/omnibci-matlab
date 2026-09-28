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
    batch.sample_indices = uint64(string(batch.sample_indices(:)));
    batch.generation = uint64(string(batch.generation));
    batch.sample_time_s = double(batch.sample_indices) ./ double(batch.sample_rate_hz);
end
end
