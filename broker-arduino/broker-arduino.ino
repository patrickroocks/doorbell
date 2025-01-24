#include "mqtt-handler.h"

// =============================================
//              main config
// =============================================

#define SAMPLE_TIME 100

// =============================================
//              SETUP
// =============================================

void setup()
{
    // setup serial
    Serial.begin(115200);

    // LEDs are already needed for WIFI setup (blinking!)
    setupLeds();
    setupWifiAndMqtt();
}

// =============================================
//              LOOP
// =============================================

void loop()
{
    loopClients();

    if (startupCycleCompleted) {
        delay(SAMPLE_TIME);
        loopState();
    } else {
        waitCycle();
    }
}
