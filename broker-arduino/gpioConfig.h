#pragma once

constexpr int LedOn = 2;
constexpr int LedAckAutobuzz = 3;
constexpr int Led3Unused = 4;
constexpr int LedError = 5;
constexpr int LedDoorbell = 6;

constexpr int FirstLed = LedOn;
constexpr int LastLed = LedDoorbell;

constexpr int SwitchBuzzmode = 7;
constexpr int SwitchAckBuzz = 8;
constexpr int SwitchAck = 9;

constexpr int InputRing = 10;
constexpr int RelayBuzzer = 11;
constexpr int RelayExtBell = 12;

constexpr int SwitchDebounceCycles = 2; // 200ms
constexpr int InputDebounceCycles = 5;  // 500ms