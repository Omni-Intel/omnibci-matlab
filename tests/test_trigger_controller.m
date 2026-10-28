function test_trigger_controller()
% Invalid arguments must fail before any port is opened.
for port = {"", " ", ["a" "b"]}
    rejected = false;
    try
        omnibci.TriggerController.connect(port{1});
    catch exception
        rejected = strcmp(exception.identifier, 'omnibci:InvalidPort');
    end
    assert(rejected);
end
fprintf('Trigger controller invalid-port checks passed.\n');
end
