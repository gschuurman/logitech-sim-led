// Foundation stub: reads newline-delimited JSON telemetry frames from
// sld-service over USB serial (see docs/aux-display-protocol.md for the
// wire schema) and parses them. Wire up an actual display library
// (TFT_eSPI, U8g2, LVGL, ...) in place of renderFrame()'s Serial.printf()
// calls once you've picked hardware.

#include <Arduino.h>
#include <ArduinoJson.h>

static const uint32_t SERIAL_BAUD = 115200;
static const size_t LINE_BUFFER_SIZE = 512;

char lineBuffer[LINE_BUFFER_SIZE];
size_t lineLength = 0;

struct AuxFrame {
    float rpm = 0;
    float rpmMax = 0;
    float speedKph = 0;
    int8_t gear = 0;
    bool hasFuel = false;
    float fuel = 0;
    bool hasLap = false;
    uint32_t lap = 0;
    bool hasPosition = false;
    uint32_t position = 0;
};

bool parseLine(const char *line, AuxFrame &out) {
    JsonDocument doc;
    DeserializationError err = deserializeJson(doc, line);
    if (err) {
        return false;
    }

    out.rpm = doc["rpm"] | 0.0f;
    out.rpmMax = doc["rpm_max"] | 0.0f;
    out.speedKph = doc["speed_kph"] | 0.0f;
    out.gear = doc["gear"] | 0;

    out.hasFuel = doc["fuel"].is<float>();
    out.fuel = out.hasFuel ? doc["fuel"].as<float>() : 0.0f;

    out.hasLap = doc["lap"].is<uint32_t>();
    out.lap = out.hasLap ? doc["lap"].as<uint32_t>() : 0;

    out.hasPosition = doc["position"].is<uint32_t>();
    out.position = out.hasPosition ? doc["position"].as<uint32_t>() : 0;

    return true;
}

void renderFrame(const AuxFrame &frame) {
    // TODO: replace with real display output. Left as Serial output so
    // this sketch is immediately useful for verifying the link before a
    // screen is wired up.
    Serial.printf(
        "rpm=%.0f/%.0f speed=%.1fkph gear=%d\n",
        frame.rpm, frame.rpmMax, frame.speedKph, frame.gear
    );
}

void setup() {
    Serial.begin(SERIAL_BAUD);
    lineLength = 0;
}

void loop() {
    while (Serial.available() > 0) {
        char c = (char)Serial.read();

        if (c == '\n') {
            lineBuffer[lineLength] = '\0';
            AuxFrame frame;
            if (lineLength > 0 && parseLine(lineBuffer, frame)) {
                renderFrame(frame);
            }
            lineLength = 0;
            continue;
        }

        if (lineLength < LINE_BUFFER_SIZE - 1) {
            lineBuffer[lineLength++] = c;
        } else {
            // Line too long (corrupt/overflow) -- reset and resync on the
            // next newline.
            lineLength = 0;
        }
    }
}
