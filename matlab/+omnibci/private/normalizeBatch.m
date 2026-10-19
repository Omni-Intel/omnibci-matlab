function batch = normalizeBatch(batch)
%NORMALIZEBATCH Give empty and nonempty batches consistent MATLAB shapes/types.
%   The native bridge reports channel_count and status_bytes for the selected
%   backend. ESP32 uses 8 channels with 3 ADS1299 status bytes; STM32 uses 32
%   channels with the 12 genuine ADS1299 status bytes (four status words)
%   carried by each V3 frame.
%
%   A nonempty batch must contain complete, correctly shaped sample data.
%   Missing or malformed data raises omnibci:InvalidData instead of silently
%   manufacturing zero samples. Legitimate empty batches keep their declared
%   channel and status widths.
if ~isstruct(batch) || ~isscalar(batch)
    error('omnibci:InvalidData', 'Expected a scalar sample batch struct from the SDK.');
end
channel_count = validateDimension(batch, 'channel_count', 1);
status_bytes = validateDimension(batch, 'status_bytes', 0);
if ~isfield(batch, 'sequence')
    error('omnibci:InvalidData', 'Sample batch is missing sequence.');
end
sample_count = numel(batch.sequence);
batch.sequence = normalizeVector(batch, 'sequence', sample_count, 'uint32');
batch.valid = normalizeVector(batch, 'valid', sample_count, 'logical');
batch.mode = normalizeVector(batch, 'mode', sample_count, 'uint8');
batch.eeg_uv = normalizeMatrix(batch, 'eeg_uv', sample_count, channel_count, 'single');
batch.raw_counts = normalizeMatrix(batch, 'raw_counts', sample_count, channel_count, 'int32');
batch.status = normalizeMatrix(batch, 'status', sample_count, status_bytes, 'uint8');
% The JSON bridge replaces non-finite STM32 microvolt values with numbers so
% jsondecode stays numeric. Restore NaN for invalid frames here. STM32 is the
% only backend with a 32-channel layout and NaN values for invalid frames;
% ESP32 eight-channel frames are always finite. Do not infer this from the
% number of status bytes: STM32 exposes 12 genuine ADS1299 status bytes.
if channel_count == 32 && ~isempty(batch.eeg_uv)
    batch.eeg_uv(~batch.valid, :) = NaN;
end
if isfield(batch, 'sample_indices')
    if ~isfield(batch, 'generation') || ~isfield(batch, 'sample_rate_hz')
        error('omnibci:InvalidData', ...
            'Sample batch with sample_indices is missing generation or sample_rate_hz.');
    end
    if sample_count == 0
        if ~isempty(batch.sample_indices)
            error('omnibci:InvalidData', ...
                'sample_indices is nonempty in an empty sample batch.');
        end
    elseif numel(batch.sample_indices) ~= sample_count || ~isvector(batch.sample_indices)
        error('omnibci:InvalidData', ...
            'sample_indices must be a vector matching the sample count.');
    end
    indices = parseUint64Decimal(batch.sample_indices);
    batch.sample_indices = reshape(indices, numel(indices), 1);
    generation = parseUint64Decimal(batch.generation);
    if ~isscalar(generation)
        error('omnibci:InvalidData', 'generation must be a scalar uint64 value.');
    end
    batch.generation = generation;
    rate = batch.sample_rate_hz;
    if ~isnumeric(rate) || ~isreal(rate) || ~isscalar(rate) || ...
            ~isfinite(rate) || rate <= 0
        error('omnibci:InvalidData', 'sample_rate_hz must be a positive finite scalar.');
    end
    batch.sample_time_s = double(batch.sample_indices) ./ double(rate);
end
end

function value = validateDimension(batch, name, minimum)
%VALIDATEDIMENSION Return a validated nonnegative integer scalar field.
if ~isfield(batch, name)
    error('omnibci:InvalidData', 'Sample batch is missing %s.', name);
end
value = batch.(name);
if ~isnumeric(value) || ~isreal(value) || ~isscalar(value) || ...
        ~isfinite(value) || value < minimum || value ~= floor(value)
    error('omnibci:InvalidData', ...
        '%s must be an integer scalar greater than or equal to %d.', name, minimum);
end
value = double(value);
end

function value = normalizeVector(batch, name, sample_count, output_type)
%NORMALIZEVECTOR Validate and return a sample_count-by-1 metadata vector.
if ~isfield(batch, name)
    error('omnibci:InvalidData', 'Sample batch is missing %s.', name);
end
value = batch.(name);
if sample_count == 0
    if ~isempty(value)
        error('omnibci:InvalidData', '%s is nonempty in an empty sample batch.', name);
    end
    value = zeros(0, 1, output_type);
    return
end
if isempty(value)
    error('omnibci:InvalidData', '%s is empty for a nonempty sample batch.', name);
end
if numel(value) ~= sample_count || ~isvector(value)
    error('omnibci:InvalidData', ...
        '%s must be a vector matching the sample count.', name);
end
if strcmp(output_type, 'logical')
    if ~(islogical(value) || isnumeric(value)) || ~isreal(value) || ...
            ~all(isfinite(value(:))) || any(value(:) ~= 0 & value(:) ~= 1)
        error('omnibci:InvalidData', '%s must contain logical values.', name);
    end
    value = reshape(logical(value), sample_count, 1);
    return
end
value = validateIntegerValues(value, name, output_type);
value = reshape(value, sample_count, 1);
end

function value = normalizeMatrix(batch, name, sample_count, columns, output_type)
%NORMALIZEMATRIX Return sample_count-by-columns for empty, scalar and batched data.
if ~isfield(batch, name)
    error('omnibci:InvalidData', 'Sample batch is missing %s.', name);
end
value = batch.(name);
if sample_count == 0
    if ~isempty(value)
        error('omnibci:InvalidData', '%s is nonempty in an empty sample batch.', name);
    end
    value = zeros(0, columns, output_type);
    return
end
if columns == 0
    % A backend may legitimately expose no status/metadata columns, but it
    % still must not attach payload to those columns.
    if ~isempty(value)
        error('omnibci:InvalidData', ...
            '%s must be empty when zero columns are expected.', name);
    end
    value = zeros(sample_count, 0, output_type);
    return
end
if isempty(value)
    error('omnibci:InvalidData', '%s is empty for a nonempty sample batch.', name);
end
if iscell(value)
    try
        value = cell2mat(value);
    catch
        error('omnibci:InvalidData', '%s has ragged or nonnumeric rows.', name);
    end
end
if ~(isnumeric(value) || islogical(value)) || ~isreal(value)
    error('omnibci:InvalidData', '%s must be a real numeric matrix.', name);
end
% Native JSON emits nested row arrays, so rows must stay in JSON order. Do
% not reshape or transpose a differently sized payload.
if ~isequal(size(value), [sample_count columns])
    error('omnibci:InvalidData', ...
        '%s must be %d-by-%d to match the sample count and expected columns.', ...
        name, sample_count, columns);
end
if strcmp(output_type, 'single')
    if ~all(isfinite(value(:)))
        error('omnibci:InvalidData', '%s contains non-finite values.', name);
    end
else
    value = validateIntegerValues(value, name, output_type);
end
value = cast(value, output_type);
end

function value = validateIntegerValues(value, name, output_type)
%VALIDATEINTEGERVALUES Require finite integers representable by output_type.
if ~isnumeric(value) || ~isreal(value)
    error('omnibci:InvalidData', '%s must be numeric.', name);
end
if ~all(isfinite(value(:))) || any(value(:) ~= floor(value(:)))
    error('omnibci:InvalidData', '%s must contain finite integers.', name);
end
limits = [double(intmin(output_type)) double(intmax(output_type))];
if any(value(:) < limits(1)) || any(value(:) > limits(2))
    error('omnibci:InvalidData', ...
        '%s values are outside the %s range.', name, output_type);
end
value = cast(value, output_type);
end

function numbers = parseUint64Decimal(values)
%PARSEUINT64DECIMAL Decode exact decimal strings without double rounding.
%   MATLAB R2023a does not cast decimal strings directly to uint64.
values = string(values);
numbers = zeros(size(values), 'uint64');
for k = 1:numel(values)
    digits = char(values(k));
    if isempty(digits) || any(digits < '0' | digits > '9')
        error('omnibci:InvalidData', 'Invalid uint64 value returned by the SDK.');
    end
    number = uint64(0);
    for digit = digits
        digitValue = uint64(digit - '0');
        if number > idivide(intmax('uint64') - digitValue, uint64(10))
            error('omnibci:InvalidData', ...
                'uint64 value returned by the SDK is out of range.');
        end
        number = number * uint64(10) + digitValue;
    end
    numbers(k) = number;
end
end
