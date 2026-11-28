// Automated verification harness for libunicore_resonance_fx.so (PipeWire LADSPA 1.1)
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <fstream>
#include <string>
#include <vector>

extern "C" {
typedef float LADSPA_Data;
typedef void* LADSPA_Handle;
typedef struct _LADSPA_Descriptor {
    unsigned long UniqueID;
    const char* Label;
    int Properties;
    const char* Name;
    const char* Maker;
    const char* Copyright;
    unsigned long PortCount;
    const int* PortDescriptors;
    const char* const* PortNames;
    const void* PortRangeHints;
    void* ImplementationData;
    LADSPA_Handle (*instantiate)(const struct _LADSPA_Descriptor*, unsigned long);
    void (*connect_port)(LADSPA_Handle, unsigned long, LADSPA_Data*);
    void (*activate)(LADSPA_Handle);
    void (*run)(LADSPA_Handle, unsigned long);
    void (*run_adding)(LADSPA_Handle, unsigned long);
    void (*set_run_adding_gain)(LADSPA_Handle, LADSPA_Data);
    void (*deactivate)(LADSPA_Handle);
    void (*cleanup)(LADSPA_Handle);
} LADSPA_Descriptor;
typedef const LADSPA_Descriptor* (*LadspaDescriptorFn)(unsigned long);
}

int main(int argc, char** argv) {
    const char* soPath = (argc > 1) ? argv[1] : "/usr/lib/ladspa/libunicore_resonance_fx.so";
    const std::string tmpConf = "/tmp/unicore_resonance_selftest.conf";
    {
        std::ofstream out(tmpConf);
        out << "master_enabled=1\n"
            << "eq_enabled=1\n"
            << "eq_band_0=6.0\n"
            << "eq_band_1=4.5\n"
            << "eq_band_8=3.0\n"
            << "pure_bass_enabled=1\n"
            << "pure_bass_mode=1\n"
            << "pure_bass_gain_db=5.0\n"
            << "clarity_enabled=1\n"
            << "clarity_gain_db=3.0\n"
            << "surround_enabled=1\n"
            << "surround_level=0.45\n"
            << "binaural_enabled=1\n"
            << "binaural_mode=0\n"
            << "tube_enabled=1\n"
            << "tube_warmth=0.35\n"
            << "convolver_enabled=1\n"
            << "convolver_preset=0\n"
            << "reverb_enabled=1\n"
            << "reverb_preset=1\n"
            << "limiter_threshold_db=-0.5\n";
    }
    setenv("UNICORE_RESONANCE_CONF", tmpConf.c_str(), 1);

    void* dl = dlopen(soPath, RTLD_NOW);
    if (!dl) {
        std::fprintf(stderr, "dlopen failed for %s: %s\n", soPath, dlerror());
        return 1;
    }
    auto fn = reinterpret_cast<LadspaDescriptorFn>(dlsym(dl, "ladspa_descriptor"));
    if (!fn) {
        std::fprintf(stderr, "Missing ladspa_descriptor symbol\n");
        return 1;
    }
    const LADSPA_Descriptor* desc = fn(0);
    if (!desc || std::strcmp(desc->Label, "unicore_resonance_fx") != 0 || desc->PortCount != 5) {
        std::fprintf(stderr, "Invalid LADSPA descriptor\n");
        return 1;
    }

    LADSPA_Handle h = desc->instantiate(desc, 48000);
    if (!h) {
        std::fprintf(stderr, "instantiate failed\n");
        return 1;
    }

    constexpr unsigned long N = 4096;
    std::vector<float> inL(N), inR(N), outL(N, 0.0f), outR(N, 0.0f);
    float master = 1.0f;
    for (unsigned long i = 0; i < N; ++i) {
        float t = static_cast<float>(i) / 48000.0f;
        float s = 0.25f * std::sin(2.0f * 3.14159265f * 80.0f * t)
                + 0.20f * std::sin(2.0f * 3.14159265f * 1000.0f * t)
                + 0.10f * std::sin(2.0f * 3.14159265f * 8000.0f * t);
        inL[i] = s;
        inR[i] = s * 0.9f;
    }
    desc->connect_port(h, 0, inL.data());
    desc->connect_port(h, 1, inR.data());
    desc->connect_port(h, 2, outL.data());
    desc->connect_port(h, 3, outR.data());
    desc->connect_port(h, 4, &master);
    desc->activate(h);
    desc->run(h, N);

    double inEnergy = 0.0, outEnergy = 0.0;
    for (unsigned long i = 0; i < N; ++i) {
        if (!std::isfinite(outL[i]) || !std::isfinite(outR[i])) {
            std::fprintf(stderr, "Non-finite DSP sample at frame %lu\n", i);
            return 1;
        }
        inEnergy += static_cast<double>(inL[i]) * inL[i];
        outEnergy += static_cast<double>(outL[i]) * outL[i];
    }
    double inRms = std::sqrt(inEnergy / N);
    double outRms = std::sqrt(outEnergy / N);

    // Test clean bypass
    master = 0.0f;
    desc->run(h, N);
    if (std::abs(outL[100] - inL[100]) > 1e-6f) {
        std::fprintf(stderr, "Bypass verification failed\n");
        return 1;
    }

    const std::string pluginName = desc->Name ? desc->Name : "";
    const std::string pluginLabel = desc->Label ? desc->Label : "";
    const unsigned long portCount = desc->PortCount;

    desc->deactivate(h);
    desc->cleanup(h);
    dlclose(dl);
    std::remove(tmpConf.c_str());

    if (outRms <= 0.01 || outRms <= inRms * 0.8) {
        std::fprintf(stderr, "Unexpected DSP RMS: in=%.4f out=%.4f\n", inRms, outRms);
        return 1;
    }
    std::printf("{\"plugin\":\"%s\",\"label\":\"%s\",\"ports\":%lu,\"in_rms\":%.4f,\"out_rms\":%.4f,\"all_passed\":true}\n",
        pluginName.c_str(), pluginLabel.c_str(), portCount, inRms, outRms);
    return 0;
}
