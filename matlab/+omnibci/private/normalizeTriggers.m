function events = normalizeTriggers(value)
% Preserve all annotations and exact device times; never align by host arrival.
generation = parseUint64Decimal(value.generation);
events = value.events;
if isempty(events)
    events = struct('generation',{},'code',{},'sequence',{},'source_timestamp_ms',{},'timing',{});
    return
end
for k = 1:numel(events)
    events(k).generation = generation;
    events(k).code = uint16(events(k).code);
    events(k).sequence = uint32(events(k).sequence);
    if ~isempty(events(k).source_timestamp_ms)
        events(k).source_timestamp_ms = uint32(events(k).source_timestamp_ms);
    end
    if ~isempty(events(k).timing)
        events(k).timing.trigger_timestamp_us = parseUint64Decimal(events(k).timing.trigger_timestamp_us);
        events(k).timing.anchor_timestamp_us = uint32(events(k).timing.anchor_timestamp_us);
    end
end
events = events(:);
end
