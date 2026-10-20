function trigger_stm32(endpoint, triggerPort)
%TRIGGER_STM32 Send one hardware trigger and read its original source timing.
% trigger_stm32("serial://COM18", "COM23")
board = omnibci.Board.connect(endpoint,25,"stm32");
cleanupBoard = onCleanup(@() delete(board));
trigger = omnibci.TriggerController.connect(triggerPort,1);
cleanupTrigger = onCleanup(@() delete(trigger));
board.start();
try
    start = tic;
    while toc(start)<.5, board.read(1); end
    trigger.send(7);
    start = tic;
    while toc(start)<2
        batch = board.read(1); %#ok<NASGU>
        % Retain batch.sequence/source_timestamp_us for later event alignment.
        events = board.readTriggers();
        if ~isempty(events), disp(events); end
    end
    board.stop();
    disp(board.readTriggers());
catch exception
    board.stop();
    rethrow(exception);
end
end
