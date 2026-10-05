#pragma once

// Sample rate of the main loop. App::loop() holds this period, so the switch debouncing and the
// raw data recording - which count samples, not time - get a stable time base.
constexpr uint32_t MainLoopSampleTimeMs = 100;

// Cycle time of the blocking wait loop used during startup (LED sequence, WiFi connect).
constexpr uint32_t StartupCycleTimeMs = 200;

// Durations of the timers in timer.h. They run on millis(), so they are independent of the loop.
constexpr uint32_t DoorOpenTimeMs = 5000;
constexpr uint32_t ExtBellTimeMs = 1000;
constexpr uint32_t AckLedTimeMs = 1000;
constexpr uint32_t ErrorLedTimeMs = 1000;
constexpr uint32_t RebootWaitTimeMs = 2000;
constexpr uint32_t BellBlinkTimeMs = 60000;

// How long setup() waits for one SSID before falling over to the next one.
constexpr uint32_t WifiConnectTimeoutMs = 10000;

// How long a lost WiFi link gets to come back before we reboot, see NetworkHandler::loop().
constexpr uint32_t WifiReconnectWaitTimeMs = 5000;

// How often the NTP time is refreshed. The NTP library throttles the actual requests on top.
constexpr uint32_t NtpUpdateIntervalMs = 10000;
