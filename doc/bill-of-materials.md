# Bill of Materials

All parts needed to build the doorbell device. For the wiring and the GPIO assignment, see [`system-sketch.md`](system-sketch.md); for the connection to the intercom, see [`connection-to-intercom.md`](connection-to-intercom.md).

All components without a specific link are assumed to be commonly available electronic parts.

I also recommend a SMD soldering kit, together with a "third hand" tool for holding components while soldering.

## Controllers

| Qty | Part                       | Specification                                                                                                                                       | Purpose                                        |
| --- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| 1   | Arduino Nano ESP32         | ESP32-S3, 3.3 V logic, with pin headers, e.g. [this one](https://www.amazon.de/Arduino-ESP32-Headers-ABX00083-MicroPython-kompatibel/dp/B0C947BHK5) | Runs the device firmware, WiFi and MQTT        |
| 1   | USB-C power supply + cable | 5 V, ≥ 1 A                                                                                                                                          | Powers the Arduino and the relays              |
| 1   | Raspberry Pi (optional)    | Any model, e.g. Raspberry Pi 4                                                                                                                      | For the doorbell HTTP server and the Mosquitto |


## Intercom

| Qty | Part                     | Specification                                                                                                                                                                                                                                                                                                                 | Purpose                   |
| --- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| 1   | Connection module "AM40" | [Anschlussmodul 40](https://www.elt24.de/shop/STR-33354-AM-40-Anschlussmodul-Anschlussmodul-fuer-HT40-zB-zur-Ansteuerung-von-Zusatzgongs) for the house intercom [STR HT-40 ("Haustelefon 40")](https://www.str-elektronik.de/produkte/innenstationen/audio-innenstationen-qwikbus/ht40-qwikbus-audio-haustelefon-mit-hoerer) | Provides the bell contact |


## Bell input filter

See [`connection-to-intercom.md`](connection-to-intercom.md#bell-signal-input) for the circuit.

| Qty | Part                 | Specification                     | Purpose                                                                                     |
| --- | -------------------- | --------------------------------- | ------------------------------------------------------------------------------------------- |
| 1   | Resistor R1          | 2.2 kΩ, ¼ W                       | Pull-up to 3.3 V                                                                            |
| 1   | Resistor R2          | 220 Ω, ¼ W                        | Limits the discharge current of C1, part of the low-pass                                    |
| 1   | Capacitor C1         | 1 µF, ceramic                     | Low-pass filter against spikes on the cable                                                 |
| 1   | Perfboard            | 2.54 mm pitch, small piece        | Carries R1, R2 and C1 close to the Arduino                                                  |
| 1   | 2-wire cable         | length as needed (here ≈ 3 m)     | From the intercom to the perfboard                                                          |
| 2   | Clip-on ferrite core | inner diameter matching the cable | One at each end of the cable; suppresses high-frequency interference picked up by the cable |


## Relays

| Qty | Part         | Specification                                                                                                                                                                                                                             | Purpose                                                        |
| --- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| 2   | Relay module | 1-channel 3.3 V relay module with high-level trigger and optocoupler input, e.g. [this one](https://www.amazon.de/1-Kanal-Relais-Netzschaltermodul-High-Level-Trigger-Optokoppler-Optoisolierung-ESP8266-Entwicklungsboard/dp/B0CN1C1S7Y) | Relay 1 (`D11`): door buzzer<br>Relay 2 (`D12`): external bell |
| 1   | 2-wire cable | length as needed                                                                                                                                                                                                                          | From relay 1 to the door buzzer button of the intercom         |
| 1   | 2-wire cable | length as needed                                                                                                                                                                                                                          | From relay 2 to the button of the external bell's remote       |


## Front panel

| Qty | Part                | Specification                                                                                                                                                                 | Purpose                                      |
| --- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| 5   | LED, 5 mm           | blue, green, orange, red, white, e.g. from [this assortment](https://www.amazon.de/Leuchtdiode-Leuchtdioden-Sortiment-Wissenschaftsprojekte-Heimwerkerprojekte/dp/B0C3VDB65D) | Status LEDs 1 – 5 (`D2` – `D6`)              |
| 3   | LED series resistor | 15 Ω, ¼ W                                                                                                                                                                     | For the blue, green and white LED, see below |
| 2   | LED series resistor | 56 Ω, ¼ W                                                                                                                                                                     | For the orange and red LED, see below        |
| 3   | Push button         | momentary, normally open                                                                                                                                                      | Switches 1 – 3 (`D7` – `D9`)                 |


### LED series resistors

The LEDs are driven directly by the GPIOs at 3.3 V. The series resistor is R = (3.3 V − U<sub>F</sub>) / I<sub>F</sub>, rounded up to the next E12 value so that the current does not exceed 20 mA:

| LED color          | U<sub>F</sub> | Calculated           | E12 value | Resulting current |
| ------------------ | ------------- | -------------------- | --------- | ----------------- |
| blue, green, white | 3.0 V         | 0.3 V / 20 mA = 15 Ω | **15 Ω**  | 20 mA             |
| orange, red        | 2.2 V         | 1.1 V / 20 mA = 55 Ω | **56 Ω**  | ≈ 19.6 mA         |


## External bell

| Qty | Part                          | Specification                               | Purpose                       |
| --- | ----------------------------- | ------------------------------------------- | ----------------------------- |
| 1   | Wireless doorbell with remote | battery-powered remote and plug-in receiver | External rings in other rooms |


## Wiring

| Qty   | Part          | Purpose                                                           |
| ----- | ------------- | ----------------------------------------------------------------- |
| 1 set | Jumper cables | Connects the Arduino with the LEDs, buttons, relays and perfboard |


## Enclosure

| Qty | Part             | Specification                                  |
| --- | ---------------- | ---------------------------------------------- |
| 1   | Custom enclosure | wood (as built here), alternatively 3D-printed |
