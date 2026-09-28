function test_installation()
%TEST_INSTALLATION Exercise missing, wrong-platform and unloadable MEX packages.
root = fileparts(fileparts(mfilename('fullpath')));
fixtureRoot = fullfile(root, 'dist');
if ~isfolder(fixtureRoot)
    mkdir(fixtureRoot);
end
fixture = tempname(fixtureRoot);
package = fullfile(fixture, 'matlab', '+omnibci');
privateFolder = fullfile(package, 'private');
mkdir(privateFolder);
copyfile(fullfile(root, 'matlab', '+omnibci', 'Board.m'), package);
copyfile(fullfile(root, 'matlab', '+omnibci', 'private', 'native.m'), privateFolder);
oldPath = path;
cleanup = onCleanup(@() path(oldPath)); %#ok<NASGU>
addpath(fullfile(fixture, 'matlab'), '-begin');
rehash;
assertError('omnibci:MissingBinary');

otherExtension = 'mexw64';
if strcmp(mexext, otherExtension)
    otherExtension = 'mexa64';
end
wrongBinary = fullfile(privateFolder, ['omnibci_mex.' otherExtension]);
writeInvalidFile(wrongBinary);
rehash;
assertError('omnibci:PlatformMismatch');
delete(wrongBinary);

binary = fullfile(privateFolder, ['omnibci_mex.' mexext]);
writeInvalidFile(binary);
rehash;
assertError('omnibci:NativeLoadFailed');
delete(binary);

writeInvalidFile(fullfile(fixture, 'build_omnibci.m'));
rehash;
assertError('omnibci:NotBuilt');
fprintf('OmniBCI installation diagnostics passed. Fixtures: %s\n', fixture);
end

function assertError(identifier)
try
    omnibci.Board.version();
catch exception
    assert(strcmp(exception.identifier, identifier), ...
        'Expected %s, got %s: %s', identifier, exception.identifier, exception.message);
    if strcmp(identifier, 'omnibci:NativeLoadFailed')
        assert(~isempty(exception.cause), 'Original loader exception was lost.');
    end
    return;
end
error('test:ExpectedFailure', 'Expected %s.', identifier);
end

function writeInvalidFile(filename)
file = fopen(filename, 'w');
assert(file ~= -1);
cleanup = onCleanup(@() fclose(file)); %#ok<NASGU>
fwrite(file, 'This is deliberately not a native library.');
end
