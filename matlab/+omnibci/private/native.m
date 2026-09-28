function value = native(request)
%NATIVE Call the Rust-backed MEX bridge with structured errors.
if exist('omnibci_mex', 'file') ~= 3
    error('omnibci:NotBuilt', 'Run build_omnibci from the repository root first.');
end
bytes = uint8(unicode2native(jsonencode(request), 'UTF-8'));
reply = jsondecode(native2unicode(omnibci_mex(bytes), 'UTF-8'));
if ~reply.ok
    error('omnibci:Device', '%s', reply.error);
end
value = reply.value;
end
