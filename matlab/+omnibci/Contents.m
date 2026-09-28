% OmniBCI MATLAB SDK
%   Control ESP32-C3/ADS1299 acquisition over USB CDC or reliable BLE.
%
% Device interface
%   Board        - Device connection, configuration and sample acquisition.
%
% Offline utilities
%   decodeFrames - Decode complete USB frames into typed MATLAB arrays.
%   selftest     - Check package loading and decoding without hardware.
%
% Getting started
%   Run omnibci.selftest after adding the package's parent matlab folder
%   to the MATLAB path. Use help omnibci.Board.connect for connection
%   syntax and help omnibci.Board.read for sample fields and units.
%
%   See also omnibci.Board, omnibci.decodeFrames, omnibci.selftest.
