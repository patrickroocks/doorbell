#pragma once

#include "configStructs.h"

#include <vector>

std::vector<MqttClientConfig> getMqttConfigs();
std::vector<WifiConfig> getWifiConfigs();