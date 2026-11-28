// UniCore OS Native ResonanceFX PipeWire LADSPA 1.1 Plugin Bridge
// Wraps the user's pure C++17 WinDspCore + UPOLA ConvolutionEngine pipeline
// for 0-MB-idle on-demand PipeWire filter-chain stereo processing.

#include "ResonanceWinEngine.h"

#include <algorithm>
#include <atomic>
#include <cctype>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <memory>
#include <sstream>
#include <string>
#include <sys/stat.h>

// Standard LADSPA 1.1 ABI (header-free, zero external package dependency)
extern "C" {

typedef float LADSPA_Data;
typedef int LADSPA_Properties;
typedef int LADSPA_PortDescriptor;
typedef int LADSPA_PortRangeHintDescriptor;
typedef void* LADSPA_Handle;

#define LADSPA_PROPERTY_HARD_RT_CAPABLE 0x4
#define LADSPA_PORT_INPUT   0x1
#define LADSPA_PORT_OUTPUT  0x2
#define LADSPA_PORT_CONTROL 0x4
#define LADSPA_PORT_AUDIO   0x8

#define LADSPA_HINT_BOUNDED_BELOW 0x1
#define LADSPA_HINT_BOUNDED_ABOVE 0x2
#define LADSPA_HINT_TOGGLED       0x4
#define LADSPA_HINT_DEFAULT_1     0x100

typedef struct {
    LADSPA_PortRangeHintDescriptor HintDescriptor;
    LADSPA_Data LowerBound;
    LADSPA_Data UpperBound;
} LADSPA_PortRangeHint;

typedef struct _LADSPA_Descriptor {
    unsigned long UniqueID;
    const char* Label;
    LADSPA_Properties Properties;
    const char* Name;
    const char* Maker;
    const char* Copyright;
    unsigned long PortCount;
    const LADSPA_PortDescriptor* PortDescriptors;
    const char* const* PortNames;
    const LADSPA_PortRangeHint* PortRangeHints;
    void* ImplementationData;

    LADSPA_Handle (*instantiate)(const struct _LADSPA_Descriptor* Descriptor, unsigned long SampleRate);
    void (*connect_port)(LADSPA_Handle Instance, unsigned long Port, LADSPA_Data* DataLocation);
    void (*activate)(LADSPA_Handle Instance);
    void (*run)(LADSPA_Handle Instance, unsigned long SampleCount);
    void (*run_adding)(LADSPA_Handle Instance, unsigned long SampleCount);
    void (*set_run_adding_gain)(LADSPA_Handle Instance, LADSPA_Data Gain);
    void (*deactivate)(LADSPA_Handle Instance);
    void (*cleanup)(LADSPA_Handle Instance);
} LADSPA_Descriptor;

} // extern "C"

namespace {

constexpr unsigned long kPortInL = 0;
constexpr unsigned long kPortInR = 1;
constexpr unsigned long kPortOutL = 2;
constexpr unsigned long kPortOutR = 3;
constexpr unsigned long kPortMasterEnable = 4;
constexpr unsigned long kPortCount = 5;

std::string resolveParamsPath() {
    if (const char* ov = std::getenv("UNICORE_RESONANCE_CONF"); ov && ov[0] != '\0') {
        return std::string(ov);
    }
    if (const char* xdg = std::getenv("XDG_RUNTIME_DIR"); xdg && xdg[0] != '\0') {
        std::string rt = std::string(xdg) + "/unicore-resonance-fx.conf";
        struct stat st {};
        if (stat(rt.c_str(), &st) == 0) {
            return rt;
        }
    }
    if (const char* home = std::getenv("HOME"); home && home[0] != '\0') {
        return std::string(home) + "/.config/unicore/resonance-fx.conf";
    }
    return "/data/home/.config/unicore/resonance-fx.conf";
}

bool parseBoolInt(const std::string& v) {
    if (v == "1" || v == "true" || v == "yes" || v == "on") return true;
    return false;
}

float parseFloatSafe(const std::string& v, float fallback) {
    char* end = nullptr;
    float out = std::strtof(v.c_str(), &end);
    if (end == v.c_str() || !std::isfinite(out)) return fallback;
    return out;
}

int parseIntSafe(const std::string& v, int fallback) {
    char* end = nullptr;
    long out = std::strtol(v.c_str(), &end, 10);
    if (end == v.c_str()) return fallback;
    return static_cast<int>(out);
}

bool loadParamsFromFile(const std::string& path, WinDspParams& out) {
    std::ifstream in(path);
    if (!in.is_open()) return false;
    WinDspParams p;
    std::string line;
    while (std::getline(in, line)) {
        auto pos = line.find('=');
        if (pos == std::string::npos) continue;
        std::string k = line.substr(0, pos);
        std::string v = line.substr(pos + 1);
        while (!v.empty() && (v.back() == '\r' || v.back() == '\n' || v.back() == ' ')) v.pop_back();
        if (k == "master_enabled") p.masterEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "eq_enabled") p.eqEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k.rfind("eq_band_", 0) == 0 && k.size() >= 9) {
            int idx = parseIntSafe(k.substr(8), -1);
            if (idx >= 0 && idx < 10) p.eqBandsDb[idx] = parseFloatSafe(v, 0.0f);
        }
        else if (k == "pure_bass_enabled") p.pureBassEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "pure_bass_mode") p.pureBassMode = std::clamp(parseIntSafe(v, 1), 0, 2);
        else if (k == "pure_bass_gain_db") p.pureBassGainDb = parseFloatSafe(v, 4.2f);
        else if (k == "clarity_enabled") p.clarityEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "clarity_mode") p.clarityMode = std::clamp(parseIntSafe(v, 0), 0, 2);
        else if (k == "clarity_gain_db") p.clarityGainDb = parseFloatSafe(v, 2.5f);
        else if (k == "surround_enabled") p.surroundEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "surround_level") p.surroundLevel = parseFloatSafe(v, 0.35f);
        else if (k == "binaural_enabled") p.binauralEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "binaural_mode") p.binauralMode = std::clamp(parseIntSafe(v, 0), 0, 2);
        else if (k == "crossfeed_level") p.crossfeedLevel = parseFloatSafe(v, 0.30f);
        else if (k == "pan_balance") p.panBalance = parseFloatSafe(v, 0.0f);
        else if (k == "orbit_speed") p.orbitSpeed = parseFloatSafe(v, 0.35f);
        else if (k == "tube_enabled") p.tubeEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "tube_warmth") p.tubeWarmth = parseFloatSafe(v, 0.40f);
        else if (k == "convolver_enabled") p.convolverEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "convolver_preset") p.convolverPreset = std::clamp(parseIntSafe(v, 0), 0, 5);
        else if (k == "reverb_enabled") p.reverbEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "reverb_preset") p.reverbPreset = std::clamp(parseIntSafe(v, 0), 0, 6);
        else if (k == "chorus_enabled") p.chorusEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "chorus_rate_hz") p.chorusRateHz = parseFloatSafe(v, 0.85f);
        else if (k == "chorus_depth") p.chorusDepth = parseFloatSafe(v, 0.60f);
        else if (k == "chorus_mix") p.chorusMix = parseFloatSafe(v, 0.50f);
        else if (k == "delay_enabled") p.delayEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "delay_time_ms") p.delayTimeMs = std::clamp(parseIntSafe(v, 260), 20, 1000);
        else if (k == "delay_feedback") p.delayFeedback = parseFloatSafe(v, 0.42f);
        else if (k == "delay_mix") p.delayMix = parseFloatSafe(v, 0.40f);
        else if (k == "loudness_enabled") p.loudnessEnabled = parseBoolInt(v) ? 1 : 0;
        else if (k == "loudness_gain_db") p.loudnessGainDb = parseFloatSafe(v, 2.0f);
        else if (k == "preamp_db") p.preampDb = parseFloatSafe(v, 0.0f);
        else if (k == "limiter_threshold_db") p.limiterThresholdDb = parseFloatSafe(v, -0.5f);
    }
    out = p;
    return true;
}

struct ResonanceLadspaInstance {
    std::unique_ptr<WinDspCore> core;
    LADSPA_Data* inL = nullptr;
    LADSPA_Data* inR = nullptr;
    LADSPA_Data* outL = nullptr;
    LADSPA_Data* outR = nullptr;
    LADSPA_Data* masterEnable = nullptr;
    std::string paramsPath;
    time_t lastMtime = 0;
    long lastSize = -1;
    unsigned long framesSincePoll = 0;
    std::vector<float> bufL;
    std::vector<float> bufR;
    WinDspParams cachedParams;

    void reloadIfChanged(bool force) {
        if (paramsPath.empty()) paramsPath = resolveParamsPath();
        struct stat st {};
        if (stat(paramsPath.c_str(), &st) != 0) {
            if (force) {
                core->updateParameters(cachedParams, true);
            }
            return;
        }
        if (!force && st.st_mtime == lastMtime && static_cast<long>(st.st_size) == lastSize) {
            return;
        }
        WinDspParams next;
        if (loadParamsFromFile(paramsPath, next)) {
            cachedParams = next;
            lastMtime = st.st_mtime;
            lastSize = static_cast<long>(st.st_size);
            core->updateParameters(cachedParams, true);
        }
    }
};

LADSPA_Handle rfxInstantiate(const LADSPA_Descriptor*, unsigned long sampleRate) {
    auto* inst = new ResonanceLadspaInstance();
    inst->core = std::make_unique<WinDspCore>();
    inst->core->setSampleRate(static_cast<int>(sampleRate));
    inst->paramsPath = resolveParamsPath();
    inst->bufL.resize(4096, 0.0f);
    inst->bufR.resize(4096, 0.0f);
    inst->reloadIfChanged(true);
    return static_cast<LADSPA_Handle>(inst);
}

void rfxConnectPort(LADSPA_Handle instance, unsigned long port, LADSPA_Data* dataLocation) {
    auto* inst = static_cast<ResonanceLadspaInstance*>(instance);
    if (!inst) return;
    switch (port) {
        case kPortInL: inst->inL = dataLocation; break;
        case kPortInR: inst->inR = dataLocation; break;
        case kPortOutL: inst->outL = dataLocation; break;
        case kPortOutR: inst->outR = dataLocation; break;
        case kPortMasterEnable: inst->masterEnable = dataLocation; break;
        default: break;
    }
}

void rfxActivate(LADSPA_Handle instance) {
    auto* inst = static_cast<ResonanceLadspaInstance*>(instance);
    if (!inst) return;
    inst->core->resetPlaybackTails();
    inst->reloadIfChanged(true);
}

void rfxRun(LADSPA_Handle instance, unsigned long sampleCount) {
    auto* inst = static_cast<ResonanceLadspaInstance*>(instance);
    if (!inst || !inst->inL || !inst->inR || !inst->outL || !inst->outR || sampleCount == 0) return;

    inst->framesSincePoll += sampleCount;
    if (inst->framesSincePoll >= 8192) {
        inst->framesSincePoll = 0;
        inst->reloadIfChanged(false);
    }

    const bool portEnabled = (!inst->masterEnable) || (*inst->masterEnable >= 0.5f);
    if (!portEnabled || !inst->cachedParams.masterEnabled) {
        if (inst->outL != inst->inL) std::memcpy(inst->outL, inst->inL, sampleCount * sizeof(float));
        if (inst->outR != inst->inR) std::memcpy(inst->outR, inst->inR, sampleCount * sizeof(float));
        return;
    }

    if (inst->bufL.size() < sampleCount) {
        inst->bufL.resize(sampleCount);
        inst->bufR.resize(sampleCount);
    }
    std::memcpy(inst->bufL.data(), inst->inL, sampleCount * sizeof(float));
    std::memcpy(inst->bufR.data(), inst->inR, sampleCount * sizeof(float));
    inst->core->processBlock(inst->bufL.data(), inst->bufR.data(), static_cast<int>(sampleCount));
    std::memcpy(inst->outL, inst->bufL.data(), sampleCount * sizeof(float));
    std::memcpy(inst->outR, inst->bufR.data(), sampleCount * sizeof(float));
}

void rfxDeactivate(LADSPA_Handle) {}

void rfxCleanup(LADSPA_Handle instance) {
    auto* inst = static_cast<ResonanceLadspaInstance*>(instance);
    delete inst;
}

const LADSPA_PortDescriptor kPortDescriptors[kPortCount] = {
    LADSPA_PORT_INPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_INPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
};

const char* const kPortNames[kPortCount] = {
    "Input L",
    "Input R",
    "Output L",
    "Output R",
    "Master Enable",
};

const LADSPA_PortRangeHint kPortHints[kPortCount] = {
    {0, 0.0f, 0.0f},
    {0, 0.0f, 0.0f},
    {0, 0.0f, 0.0f},
    {0, 0.0f, 0.0f},
    {LADSPA_HINT_TOGGLED | LADSPA_HINT_BOUNDED_BELOW | LADSPA_HINT_BOUNDED_ABOVE | LADSPA_HINT_DEFAULT_1, 0.0f, 1.0f},
};

const LADSPA_Descriptor kDescriptor = {
    0x52465831UL,
    "unicore_resonance_fx",
    LADSPA_PROPERTY_HARD_RT_CAPABLE,
    "UniCore ResonanceFX 10-Band Equalizer & Spatial DSP",
    "UniCore OS / ResonanceFX",
    "GPLv3 / Sovereign Desktop DSP",
    kPortCount,
    kPortDescriptors,
    kPortNames,
    kPortHints,
    nullptr,
    rfxInstantiate,
    rfxConnectPort,
    rfxActivate,
    rfxRun,
    nullptr,
    nullptr,
    rfxDeactivate,
    rfxCleanup,
};

} // namespace

extern "C" const LADSPA_Descriptor* ladspa_descriptor(unsigned long index) {
    return (index == 0) ? &kDescriptor : nullptr;
}
