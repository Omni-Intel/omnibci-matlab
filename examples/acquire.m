% Add <repository>/matlab to path first, then change the endpoint.
addpath(fullfile(fileparts(fileparts(mfilename('fullpath'))), 'matlab'));
disp(omnibci.Board.discover("serial"));
board = omnibci.Board.connect("serial://COM5");
cleanup = onCleanup(@() delete(board));
disp(board.snapshot());
board.start();
for k = 1:10
    batch = board.read(2);
    fprintf('%d samples; first index %s\n', size(batch.eeg_uv, 1), string(batch.sample_indices(1)));
end
board.stop();
board.close();
