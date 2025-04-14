#pragma once


constexpr int MainLoopSampleTimeMs = 100;
constexpr int StartupCycleTimeMs = 500;

constexpr int DoorOpenCycles = 50;   // 5 sec
constexpr int ExtBellCycles = 10;    // 1 sec
constexpr int AckLedCycles = 10;     // 1 sec
constexpr int ErrorLedCycles = 10;   // 1 sec
constexpr int RebootWaitCycles = 20; // 2 sec
constexpr int BellBlinkCycles = 600; // 60 sec

constexpr int WifiTimeoutCycles = 10; // 5sec, as a startup cycle is 500ms
