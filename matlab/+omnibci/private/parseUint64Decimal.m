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
