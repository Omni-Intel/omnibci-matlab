function build_omnibci()
%BUILD_OMNIBCI Build the Rust SDK bridge and MATLAB MEX gateway.
root = fileparts(mfilename('fullpath'));
if ~isfile(fullfile(root, 'sdk', 'Cargo.toml'))
    error('omnibci:MissingSDK', 'Initialize the SDK submodule: git submodule update --init');
end
previous = pwd;
cleanup = onCleanup(@() cd(previous));
cd(root);
setenv('MATLAB_ROOT', matlabroot);
[status, output] = system('cargo build --offline --locked --release --features mex');
if status ~= 0
    error('omnibci:CargoBuild', 'Rust build failed:\n%s', output);
end
destination = fullfile(root, 'matlab', '+omnibci', 'private');
if ~isfolder(destination)
    mkdir(destination);
end
if ispc
    library = 'omnibci_matlab_native.dll';
elseif ismac
    library = 'libomnibci_matlab_native.dylib';
else
    library = 'libomnibci_matlab_native.so';
end
copyfile(fullfile(root, 'target', 'release', library), fullfile(destination, ['omnibci_mex.' mexext]));
fprintf('OmniBCI MATLAB SDK built in %s\n', destination);
end
