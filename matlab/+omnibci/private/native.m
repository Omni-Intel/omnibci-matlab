function value = native(request)
%NATIVE Call the Rust-backed MEX bridge with structured errors.
privateFolder = fileparts(mfilename('fullpath'));
binary = fullfile(privateFolder, ['omnibci_mex.' mexext]);
if ~isfile(binary)
    otherBinaries = dir(fullfile(privateFolder, 'omnibci_mex.mex*'));
    if ~isempty(otherBinaries)
        error('omnibci:PlatformMismatch', ...
            'This package does not contain a MEX for MATLAB architecture %s (%s). Download the matching platform ZIP. Found: %s', ...
            computer('arch'), mexext, strjoin({otherBinaries.name}, ', '));
    elseif isfile(fullfile(privateFolder, '..', '..', '..', 'build_omnibci.m'))
        error('omnibci:NotBuilt', ...
            'The source checkout has no %s. Run build_omnibci from the repository root, or install a matching precompiled ZIP.', mexext);
    else
        error('omnibci:MissingBinary', ...
            'The installed package is missing %s. Download and fully extract the ZIP matching MATLAB architecture %s.', ...
            binary, computer('arch'));
    end
end
bytes = uint8(unicode2native(jsonencode(request), 'UTF-8'));
try
    response = omnibci_mex(bytes);
catch exception
    if any(strcmp(exception.identifier, {'MATLAB:invalidMEXFile', 'MATLAB:mex:ErrInvalidMEXFile'}))
        failure = MException('omnibci:NativeLoadFailed', ...
            ['MATLAB could not load %s. Check the package architecture, minimum MATLAB version, ' ...
             'and runtime libraries listed in README.md (Linux: libudev1 and libdbus-1-3). ' ...
             'Original loader error: %s'], binary, exception.message);
        throwAsCaller(addCause(failure, exception));
    end
    rethrow(exception);
end
reply = jsondecode(native2unicode(response, 'UTF-8'));
if ~reply.ok
    error('omnibci:Device', '%s', reply.error);
end
value = reply.value;
end
