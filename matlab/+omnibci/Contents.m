% OmniBCI MATLAB SDK
%   Control ESP32-C3/ADS1299 eight-channel or STM32H563 32-channel
%   acquisition over board USB CDC or BLE.
%
% Device interface
%   Board        - Device connection, configuration and sample acquisition.
%                  Select "esp32" (default) or "stm32" with the explicit
%                  deviceType argument to connect and discover.
%   Board.readTriggers - Drain STM32 hardware events separately from EEG.
%   TriggerController  - Send external trigger codes through a serial port.
%
% Offline utilities
%   decodeFrames - Decode complete eight-channel ESP32 USB frames.
%   decodeTriggers - Decode complete STM32 hardware trigger packets.
%   selftest     - Check package loading and decoding without hardware.
%
% Getting started
%   Run omnibci.selftest after adding the package's parent matlab folder
%   to the MATLAB path. Use help omnibci.Board.connect for connection
%   syntax and help omnibci.Board.read for sample fields and units.
%   STM32 uses fixed 250 Hz and SRB1, and its status matrix contains the
%   raw 12 ADS1299 status bytes transmitted with each V3 frame. See README
%   and TRIGGER.md for supported paths and validation scope.
%
%   See also omnibci.Board, omnibci.Board.readTriggers,
%       omnibci.TriggerController, omnibci.decodeFrames,
%       omnibci.decodeTriggers, omnibci.selftest.
