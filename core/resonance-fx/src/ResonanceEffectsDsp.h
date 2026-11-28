#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <vector>
#include "ResonanceBiquadEq.h"

namespace rfx {

inline constexpr float kFxPi = 3.14159265358979323846f;
inline constexpr float kFxTwoPi = 2.0f * kFxPi;

inline float clampFinite(float v, float lo, float hi, float fallback) {
    return std::isfinite(v) ? std::clamp(v, lo, hi) : fallback;
}

inline float readPcm24LeSample(const uint8_t* p) {
    constexpr float inv24 = 1.0f / 8388608.0f;
    int32_t v = static_cast<int32_t>(p[0]) | (static_cast<int32_t>(p[1]) << 8) | (static_cast<int32_t>(p[2]) << 16);
    if (v & 0x800000) v |= ~0xFFFFFF;
    return static_cast<float>(v) * inv24;
}

template <typename ReadSampleFn>
inline void downmixChannelsToStereo(int sourceChannels, ReadSampleFn readCh, float& l, float& r) {
    l = readCh(0);
    if (sourceChannels == 1) {
        r = l;
        return;
    }
    r = readCh(1);
    if (sourceChannels == 2) {
        return;
    }
    float sumL = l;
    float sumR = r;
    if (sourceChannels >= 3) {
        const float center = readCh(2) * 0.70710678f;
        sumL += center;
        sumR += center;
    }
    if (sourceChannels >= 4) {
        const float lfe = readCh(3) * 0.35355339f;
        sumL += lfe;
        sumR += lfe;
    }
    for (int ch = 4; ch < sourceChannels; ++ch) {
        const float s = readCh(ch) * 0.5f;
        if ((ch % 2) == 0) sumL += s;
        else sumR += s;
    }
    const float normScale = 1.0f / std::max(1.0f, std::sqrt(static_cast<float>(sourceChannels) * 0.5f));
    l = std::clamp(sumL * normScale, -1.0f, 1.0f);
    r = std::clamp(sumR * normScale, -1.0f, 1.0f);
}

struct SubBassProtectorState {
    float lpL = 0.0f;
    float lpR = 0.0f;

    void reset() { lpL = lpR = 0.0f; }

    // Soft-limit only the <300 Hz split sub band so heavy Pure Bass boosts stay loud & vocal-clean
    void processSample(
        float& l,
        float& rS,
        StereoBiquad& bassShelf,
        StereoBiquad& bassCharacterFilter,
        StereoBiquad& antiMaskFilter,
        float subAlpha
    ) {
        bassShelf.processStereo(l, rS);
        bassCharacterFilter.processStereo(l, rS);
        antiMaskFilter.processStereo(l, rS);
        lpL = flushDenormal((1.0f - subAlpha) * l + subAlpha * lpL);
        lpR = flushDenormal((1.0f - subAlpha) * rS + subAlpha * lpR);
        const float restL = l - lpL;
        const float restR = rS - lpR;
        auto shapeSub = [](float s) {
            constexpr float knee = 0.36f;
            constexpr float ceilSub = 0.58f;
            const float ax = std::abs(s);
            if (ax <= knee) return s;
            const float shaped = knee + (ceilSub - knee) * std::tanh((ax - knee) / (ceilSub - knee));
            return (s < 0.0f) ? -shaped : shaped;
        };
        l = shapeSub(lpL) + restL;
        rS = shapeSub(lpR) + restR;
    }
};

struct TubeStageState {
    float dcLpL = 0.0f;
    float dcLpR = 0.0f;
    float hfLpL = 0.0f;
    float hfLpR = 0.0f;

    void reset() { dcLpL = dcLpR = hfLpL = hfLpR = 0.0f; }

    void processSample(
        float& l,
        float& rS,
        bool enabled,
        float warmth,
        float dcAlpha,
        float hfAlpha,
        StereoBiquad& bodyShelf
    ) {
        if (!enabled || warmth <= 0.01f) {
            dcLpL = dcLpR = 0.0f;
            hfLpL = l;
            hfLpR = rS;
            bodyShelf.resetState();
            return;
        }
        const float w = std::clamp(warmth, 0.0f, 1.0f);
        const float drive = 1.0f + w * 2.6f;
        const float bias = 0.42f * w;
        const float biasVal = std::tanh(bias);
        const float slope0 = std::max(0.30f, drive * (1.0f - biasVal * biasVal));
        const float rawL = (std::tanh(l * drive + bias) - biasVal) / slope0;
        const float rawR = (std::tanh(rS * drive + bias) - biasVal) / slope0;
        dcLpL = flushDenormal((1.0f - dcAlpha) * rawL + dcAlpha * dcLpL);
        dcLpR = flushDenormal((1.0f - dcAlpha) * rawR + dcAlpha * dcLpR);
        const float triL = rawL - dcLpL;
        const float triR = rawR - dcLpR;
        l = l * (1.0f - w * 0.85f) + triL * (w * 0.85f);
        rS = rS * (1.0f - w * 0.85f) + triR * (w * 0.85f);
        bodyShelf.processStereo(l, rS);
        hfLpL = flushDenormal((1.0f - hfAlpha) * l + hfAlpha * hfLpL);
        hfLpR = flushDenormal((1.0f - hfAlpha) * rS + hfAlpha * hfLpR);
        l = l * (1.0f - 0.25f * w) + hfLpL * (0.25f * w);
        rS = rS * (1.0f - 0.25f * w) + hfLpR * (0.25f * w);
    }
};

inline void configureTubeAndLoudnessFilters(
    bool masterEnabled,
    bool tubeEnabled,
    float tubeWarmth,
    bool loudnessEnabled,
    float loudnessGainDb,
    float sr,
    StereoBiquad& tubeBodyShelf,
    StereoBiquad& loudShelfLow,
    StereoBiquad& loudShelfHigh
) {
    const float tubeBodyDb = (masterEnabled && tubeEnabled && tubeWarmth > 0.01f)
        ? std::clamp(tubeWarmth, 0.0f, 1.0f) * 2.0f
        : 0.0f;
    tubeBodyShelf.setPeaking(220.0f, sr, tubeBodyDb, 0.70f);

    if (masterEnabled && loudnessEnabled && loudnessGainDb > 0.01f) {
        const float fletcherDb = std::clamp(loudnessGainDb, 0.0f, 12.0f);
        loudShelfLow.setLowShelf(78.0f, sr, fletcherDb * 0.34f, 0.72f);
        loudShelfHigh.setHighShelf(10000.0f, sr, fletcherDb * 0.24f, 0.72f);
    } else {
        loudShelfLow.setLowShelf(78.0f, sr, 0.0f, 0.72f);
        loudShelfHigh.setHighShelf(10000.0f, sr, 0.0f, 0.72f);
    }
}

inline void processSurroundStageSample(
    float& l,
    float& rS,
    bool enabled,
    float surroundLevel,
    int haasOffset,
    int ambStep,
    int ambDelta,
    std::vector<float>& haasDelay,
    int& haasIdx
) {
    const int bufSize = static_cast<int>(haasDelay.size());
    if (bufSize <= 16) return;
    const float rawMid = (l + rS) * 0.5f;
    haasDelay[haasIdx] = rawMid;
    if (enabled && surroundLevel > 0.01f) {
        const float s = std::clamp(surroundLevel, 0.0f, 1.0f);
        const int d0 = (haasIdx - haasOffset + bufSize) % bufSize;
        const int d1 = (d0 - ambStep + bufSize) % bufSize;
        const int d2 = (d0 - ambDelta + bufSize) % bufSize;
        const int d3 = (d0 - ambDelta - ambStep + bufSize) % bufSize;
        const float bpAmb = 0.25f * ((haasDelay[d0] + haasDelay[d1]) - (haasDelay[d2] + haasDelay[d3]));
        const float rawSide = (l - rS) * 0.5f;
        const float widSide = rawSide * (1.0f + s * 0.85f) + bpAmb * (s * 0.18f);
        const float midAtten = rawMid * (1.0f - s * 0.05f);
        const float norm = 1.0f / (1.0f + s * 0.22f);
        l = (midAtten + widSide) * norm;
        rS = (midAtten - widSide) * norm;
    }
    haasIdx = (haasIdx + 1) % bufSize;
}

struct SchroederReverbState {
    static constexpr int kNumCombs = 4;
    std::vector<float> combBufL[kNumCombs];
    std::vector<float> combBufR[kNumCombs];
    int combIdxL[kNumCombs] = {0};
    int combIdxR[kNumCombs] = {0};
    float combLpL[kNumCombs] = {0};
    float combLpR[kNumCombs] = {0};
    std::vector<float> allpassL[2];
    std::vector<float> allpassR[2];
    int apIdxL[2] = {0};
    int apIdxR[2] = {0};

    void init(int sampleRate) {
        const float ratio = static_cast<float>(std::max(8000, sampleRate)) / 44100.0f;
        constexpr int baseCombs[kNumCombs] = {1116, 1277, 1422, 1617};
        constexpr int baseAp[2] = {556, 441};
        for (int i = 0; i < kNumCombs; ++i) {
            const int lenL = std::max(64, static_cast<int>(baseCombs[i] * ratio));
            const int lenR = lenL + 23;
            combBufL[i].assign(lenL, 0.0f);
            combBufR[i].assign(lenR, 0.0f);
            combIdxL[i] = combIdxR[i] = 0;
            combLpL[i] = combLpR[i] = 0.0f;
        }
        for (int i = 0; i < 2; ++i) {
            const int lenL = std::max(32, static_cast<int>(baseAp[i] * ratio));
            const int lenR = lenL + 17;
            allpassL[i].assign(lenL, 0.0f);
            allpassR[i].assign(lenR, 0.0f);
            apIdxL[i] = apIdxR[i] = 0;
        }
    }

    void reset() {
        for (int i = 0; i < kNumCombs; ++i) {
            std::fill(combBufL[i].begin(), combBufL[i].end(), 0.0f);
            std::fill(combBufR[i].begin(), combBufR[i].end(), 0.0f);
            combIdxL[i] = combIdxR[i] = 0;
            combLpL[i] = combLpR[i] = 0.0f;
        }
        for (int i = 0; i < 2; ++i) {
            std::fill(allpassL[i].begin(), allpassL[i].end(), 0.0f);
            std::fill(allpassR[i].begin(), allpassR[i].end(), 0.0f);
            apIdxL[i] = apIdxR[i] = 0;
        }
    }

    void clearIdleSample() {
        for (int c = 0; c < kNumCombs; ++c) {
            if (!combBufL[c].empty()) {
                combBufL[c][combIdxL[c]] = 0.0f;
                combIdxL[c] = (combIdxL[c] + 1) % static_cast<int>(combBufL[c].size());
            }
            if (!combBufR[c].empty()) {
                combBufR[c][combIdxR[c]] = 0.0f;
                combIdxR[c] = (combIdxR[c] + 1) % static_cast<int>(combBufR[c].size());
            }
            combLpL[c] = combLpR[c] = 0.0f;
        }
        for (int a = 0; a < 2; ++a) {
            if (!allpassL[a].empty()) {
                allpassL[a][apIdxL[a]] = 0.0f;
                apIdxL[a] = (apIdxL[a] + 1) % static_cast<int>(allpassL[a].size());
            }
            if (!allpassR[a].empty()) {
                allpassR[a][apIdxR[a]] = 0.0f;
                apIdxR[a] = (apIdxR[a] + 1) % static_cast<int>(allpassR[a].size());
            }
        }
    }

    void processSample(float& l, float& rS, int reverbPreset) {
        if (reverbPreset <= 0) {
            clearIdleSample();
            return;
        }
        float feedback = 0.84f;
        float damp = 0.32f;
        float wet = 0.34f;
        float dryScale = 0.94f;
        switch (reverbPreset) {
            case 1:  feedback = 0.78f;  damp = 0.46f; wet = 0.28f; dryScale = 0.96f; break;
            case 2:  feedback = 0.845f; damp = 0.36f; wet = 0.36f; dryScale = 0.93f; break;
            case 3:  feedback = 0.885f; damp = 0.28f; wet = 0.42f; dryScale = 0.91f; break;
            case 4:  feedback = 0.915f; damp = 0.24f; wet = 0.48f; dryScale = 0.89f; break;
            case 5:  feedback = 0.945f; damp = 0.18f; wet = 0.56f; dryScale = 0.86f; break;
            default: feedback = 0.905f; damp = 0.10f; wet = 0.44f; dryScale = 0.91f; break;
        }

        const float revInL = (l * 0.75f + rS * 0.25f) * 0.11f;
        const float revInR = (rS * 0.75f + l * 0.25f) * 0.11f;
        float accL = 0.0f;
        float accR = 0.0f;
        for (int i = 0; i < kNumCombs; ++i) {
            const int idxL = combIdxL[i];
            const float yL = combBufL[i][idxL];
            combLpL[i] = flushDenormal(yL * (1.0f - damp) + combLpL[i] * damp);
            combBufL[i][idxL] = flushDenormal(revInL + combLpL[i] * feedback);
            combIdxL[i] = (idxL + 1) % static_cast<int>(combBufL[i].size());
            accL += yL;

            const int idxR = combIdxR[i];
            const float yR = combBufR[i][idxR];
            combLpR[i] = flushDenormal(yR * (1.0f - damp) + combLpR[i] * damp);
            combBufR[i][idxR] = flushDenormal(revInR + combLpR[i] * feedback);
            combIdxR[i] = (idxR + 1) % static_cast<int>(combBufR[i].size());
            accR += yR;
        }
        for (int i = 0; i < 2; ++i) {
            const int idxL = apIdxL[i];
            const float bufL = allpassL[i][idxL];
            const float wL = accL + 0.5f * bufL;
            allpassL[i][idxL] = flushDenormal(wL);
            accL = flushDenormal(bufL - 0.5f * wL);
            apIdxL[i] = (idxL + 1) % static_cast<int>(allpassL[i].size());

            const int idxR = apIdxR[i];
            const float bufR = allpassR[i][idxR];
            const float wR = accR + 0.5f * bufR;
            allpassR[i][idxR] = flushDenormal(wR);
            accR = flushDenormal(bufR - 0.5f * wR);
            apIdxR[i] = (idxR + 1) % static_cast<int>(allpassR[i].size());
        }
        l = l * dryScale + accL * wet;
        rS = rS * dryScale + accR * wet;
    }
};

inline float readFracDelay(const std::vector<float>& buf, int writeIdx, float delaySamples) {
    const int size = static_cast<int>(buf.size());
    if (size < 4) return 0.0f;
    const float clamped = std::clamp(delaySamples, 1.0f, static_cast<float>(size - 3));
    const int iDelay = static_cast<int>(clamped);
    const float frac = clamped - static_cast<float>(iDelay);
    const int idx1 = (writeIdx - iDelay + size) % size;
    const int idx0 = (idx1 - 1 + size) % size;
    const float s1 = buf[idx1];
    const float s0 = buf[idx0];
    return s1 + frac * (s0 - s1);
}

inline void configurePureBassFilters(
    bool enabled,
    int mode,
    float gainDb,
    float sr,
    StereoBiquad& bassShelf,
    StereoBiquad& bassCharacterFilter,
    StereoBiquad& antiMaskFilter
) {
    if (!enabled || gainDb <= 0.01f) {
        bassShelf.setLowShelf(74.0f, sr, 0.0f, 0.78f);
        bassCharacterFilter.setPeaking(100.0f, sr, 0.0f, 0.90f);
        antiMaskFilter.setPeaking(1450.0f, sr, 0.0f, 0.72f);
        return;
    }

    const float g = std::clamp(gainDb, 0.0f, 24.0f);
    if (mode == 0) {
        // Mode 0: Smooth Lows — warm musical mid-bass body around 115 Hz (less extreme <50Hz sub boom)
        bassShelf.setLowShelf(135.0f, sr, std::clamp(g * 0.22f, 0.0f, 6.0f), 0.62f);
        bassCharacterFilter.setPeaking(115.0f, sr, std::clamp(g * 0.40f, 0.0f, 4.8f), 0.90f);
    } else if (mode == 2) {
        // Mode 2: Subwoofer — deep earth-shaking sub-octave bloom around 42-58 Hz without vocal ducking
        bassShelf.setLowShelf(58.0f, sr, std::clamp(g * 0.82f, 0.0f, 13.6f), 0.90f);
        bassCharacterFilter.setPeaking(42.0f, sr, std::clamp(g * 0.14f, 0.0f, 2.0f), 0.95f);
    } else {
        // Mode 1: Deep Impact — punchy 68-78 Hz club slam with 195 Hz mud cleanup
        bassShelf.setLowShelf(78.0f, sr, std::clamp(g * 0.56f, 0.0f, 11.0f), 0.78f);
        bassCharacterFilter.setPeaking(195.0f, sr, -std::clamp(g * 0.18f, 0.0f, 2.6f), 1.05f);
    }

    const float bassExcess = (gainDb > 3.5f) ? std::min(14.5f, gainDb - 3.5f) : 0.0f;
    antiMaskFilter.setPeaking(1450.0f, sr, std::min(2.4f, bassExcess * 0.17f), 0.72f);
}

inline void configureClarityFilters(
    bool enabled,
    int mode,
    float gainDb,
    float sr,
    StereoBiquad& clarityShelf,
    StereoBiquad& clarityVoicingFilter
) {
    if (!enabled || gainDb <= 0.01f) {
        clarityShelf.setHighShelf(8800.0f, sr, 0.0f, 0.72f);
        clarityVoicingFilter.setPeaking(3200.0f, sr, 0.0f, 0.85f);
        return;
    }

    const float g = std::clamp(gainDb, 0.0f, 24.0f);
    if (mode == 1) {
        // Mode 1: Air Dynamic — relaxed mid glare @ 2.4 kHz + soaring open air @ 7.8 kHz+
        clarityVoicingFilter.setPeaking(2400.0f, sr, -std::clamp(g * 0.22f, 0.0f, 3.2f), 0.85f);
        clarityShelf.setHighShelf(7800.0f, sr, std::clamp(g * 0.72f, 0.0f, 10.0f), 0.85f);
    } else if (mode == 2) {
        // Mode 2: Ultra Hi-Fi — vivid 4.6 kHz transient bite + strong 6.8 kHz+ mastering shelf
        clarityVoicingFilter.setPeaking(4600.0f, sr, std::clamp(g * 0.46f, 0.0f, 6.2f), 0.78f);
        clarityShelf.setHighShelf(6800.0f, sr, std::clamp(g * 0.88f, 0.0f, 12.5f), 0.78f);
    } else {
        // Mode 0: Studio Natural — forward 3.0 kHz vocal silk + soft velvety 9.2 kHz top without hiss
        clarityVoicingFilter.setPeaking(3000.0f, sr, std::clamp(g * 0.36f, 0.0f, 4.8f), 0.90f);
        clarityShelf.setHighShelf(9200.0f, sr, std::clamp(g * 0.14f, 0.0f, 3.2f), 0.68f);
    }
}

struct ClarityExciterSpec {
    float hpfAlpha;
    float amount;
    float h2Weight;
    float h3Weight;
};

inline ClarityExciterSpec prepareClarityExciter(float sr, bool enabled, int mode, float gainDb) {
    if (!enabled || gainDb <= 0.05f) {
        return {std::exp(-kFxTwoPi * 2600.0f / sr), 0.0f, 0.0f, 0.0f};
    }
    const float norm = std::clamp(gainDb, 0.0f, 14.0f) / 14.0f;
    if (mode == 1) {
        return {std::exp(-kFxTwoPi * 2950.0f / sr), norm * 0.46f, 1.38f, 0.45f};
    }
    if (mode == 2) {
        return {std::exp(-kFxTwoPi * 1800.0f / sr), norm * 0.82f, 2.20f, 1.05f};
    }
    return {std::exp(-kFxTwoPi * 2100.0f / sr), norm * 0.28f, 1.15f, 0.0f};
}

inline void processClarityExciterSample(
    float& l,
    float& rS,
    const ClarityExciterSpec& spec,
    float& inLpL,
    float& inLpR,
    float& dcLpL,
    float& dcLpR
) {
    inLpL = flushDenormal((1.0f - spec.hpfAlpha) * l + spec.hpfAlpha * inLpL);
    inLpR = flushDenormal((1.0f - spec.hpfAlpha) * rS + spec.hpfAlpha * inLpR);
    if (spec.amount <= 0.001f) {
        dcLpL = dcLpR = 0.0f;
        return;
    }
    const float hpL = l - inLpL;
    const float hpR = rS - inLpR;
    const float rawHL = std::clamp(
        hpL * hpL * spec.h2Weight + hpL * hpL * hpL * spec.h3Weight,
        -0.38f,
        0.38f
    );
    const float rawHR = std::clamp(
        hpR * hpR * spec.h2Weight + hpR * hpR * hpR * spec.h3Weight,
        -0.38f,
        0.38f
    );
    dcLpL = flushDenormal((1.0f - spec.hpfAlpha) * rawHL + spec.hpfAlpha * dcLpL);
    dcLpR = flushDenormal((1.0f - spec.hpfAlpha) * rawHR + spec.hpfAlpha * dcLpR);
    l += (rawHL - dcLpL) * spec.amount;
    rS += (rawHR - dcLpR) * spec.amount;
}

inline void processBinauralStageSample(
    float& l,
    float& rS,
    bool enabled,
    int mode,
    float crossfeedLevel,
    float panBalance,
    float orbitSpeed,
    float sr,
    std::vector<float>& crossDelayL,
    std::vector<float>& crossDelayR,
    int& crossIdx,
    float& crossLpL,
    float& crossLpR,
    std::vector<float>& panItdL,
    std::vector<float>& panItdR,
    int& panItdIdx,
    float& shadowLpL,
    float& shadowLpR,
    float& orbitPhase
) {
    const int delayLen = static_cast<int>(crossDelayL.size());
    if (delayLen <= 2) return;

    const float crossLpAlpha = std::exp(-kFxTwoPi * 1200.0f / sr);
    const int crossDelaySamples = std::clamp(static_cast<int>(sr * 0.00030f), 1, delayLen - 2);
    crossDelayL[crossIdx] = l;
    crossDelayR[crossIdx] = rS;
    const int cReadIdx = (crossIdx - crossDelaySamples + delayLen) % delayLen;
    crossLpL = flushDenormal((1.0f - crossLpAlpha) * crossDelayL[cReadIdx] + crossLpAlpha * crossLpL);
    crossLpR = flushDenormal((1.0f - crossLpAlpha) * crossDelayR[cReadIdx] + crossLpAlpha * crossLpR);
    crossIdx = (crossIdx + 1) % delayLen;

    const float normCross = std::clamp(crossfeedLevel, 0.0f, 1.0f);
    if (enabled && mode == 0 && normCross > 0.005f) {
        // Mode 0: Natural Stage — true Bauer/AMBEO loudspeaker-in-room crossfeed
        const float crossWet = normCross * 0.48f;
        const float crossDirect = 1.0f - crossWet * 0.42f;
        l = l * crossDirect + crossLpR * crossWet;
        rS = rS * crossDirect + crossLpL * crossWet;
    }

    if (enabled && mode == 1 && std::abs(panBalance) <= 0.02f) {
        // Mode 1: Studio Pan centered — crisp discrete studio separation (no cross-bleed blur)
        const float mid = 0.5f * (l + rS);
        const float side = 0.5f * (l - rS);
        l = (mid + side * 1.24f) * 0.94f;
        rS = (mid - side * 1.24f) * 0.94f;
    }

    float effPan = 0.0f;
    float rearFactor = 0.0f;
    if (enabled) {
        if (mode == 1) {
            effPan = std::clamp(panBalance, -1.0f, 1.0f);
        } else if (mode == 2) {
            const float orbitHz = 0.14f + 0.76f * std::clamp(orbitSpeed, 0.1f, 1.5f);
            orbitPhase += (kFxTwoPi * orbitHz) / sr;
            if (orbitPhase >= kFxTwoPi) orbitPhase -= kFxTwoPi;
            const float orbitDepth = std::clamp(0.76f + 0.22f * normCross, 0.72f, 0.96f);
            effPan = std::sin(orbitPhase) * orbitDepth;
            rearFactor = std::max(0.0f, -std::cos(orbitPhase));

            // Subtle 0.6ms ambient air halo so 8D Orbit stays out-of-head spherical
            const int haloSamples = std::clamp(static_cast<int>(sr * 0.00060f), 1, delayLen - 2);
            const int hIdx = (panItdIdx - haloSamples + delayLen) % delayLen;
            l += panItdR[hIdx] * 0.10f;
            rS += panItdL[hIdx] * 0.10f;
        }
    }

    panItdL[panItdIdx] = l;
    panItdR[panItdIdx] = rS;

    if (std::abs(effPan) > 0.01f) {
        const float angle = (effPan + 1.0f) * (kFxPi * 0.25f);
        const float gainL = std::cos(angle) * 1.4142135f;
        const float gainR = std::sin(angle) * 1.4142135f;
        const float itdF = std::clamp(std::abs(effPan) * 0.00062f * sr, 0.0f, static_cast<float>(delayLen - 3));
        const int i0 = static_cast<int>(itdF);
        const float itdFrac = itdF - static_cast<float>(i0);
        const int idxA = (panItdIdx - i0 + delayLen) % delayLen;
        const int idxB = (idxA - 1 + delayLen) % delayLen;
        const float rearAdjustedCut = std::max(2400.0f, 18000.0f - (14500.0f + 1500.0f * rearFactor) * std::abs(effPan));
        const float shadowAlpha = std::exp(-kFxTwoPi * rearAdjustedCut / sr);
        if (effPan > 0.0f) {
            const float farL = panItdL[idxA] + itdFrac * (panItdL[idxB] - panItdL[idxA]);
            shadowLpL = flushDenormal((1.0f - shadowAlpha) * farL + shadowAlpha * shadowLpL);
            shadowLpR = rS;
            l = shadowLpL * gainL;
            rS *= gainR;
        } else {
            const float farR = panItdR[idxA] + itdFrac * (panItdR[idxB] - panItdR[idxA]);
            shadowLpR = flushDenormal((1.0f - shadowAlpha) * farR + shadowAlpha * shadowLpR);
            shadowLpL = l;
            l *= gainL;
            rS = shadowLpR * gainR;
        }
    } else {
        shadowLpL = l;
        shadowLpR = rS;
    }
    panItdIdx = (panItdIdx + 1) % delayLen;
}

struct ChorusBlockCoeffs {
    int mode = 0;
    float rateRad = 0.0f;
    float depthNorm = 0.0f;
    float targetMix = 0.0f;
    float mixAlpha = 0.01f;
    float hpAlpha = 0.98f;
    float wetHpAlpha = 0.98f;
    float lpAlpha = 0.5f;
    float regen = 0.0f;
    float sr = 48000.0f;

    static ChorusBlockCoeffs prepare(float sampleRate, int chorusMode, float rateHz, float depth, float mix) {
        ChorusBlockCoeffs c;
        c.sr = std::max(8000.0f, sampleRate);
        c.mode = std::clamp(chorusMode, 0, 2);
        c.rateRad = (kFxTwoPi * std::clamp(rateHz, 0.1f, 5.0f)) / c.sr;
        c.depthNorm = std::clamp(depth, 0.0f, 1.0f);
        c.targetMix = std::clamp(mix, 0.0f, 1.0f);
        c.mixAlpha = 1.0f - std::exp(-1.0f / (0.016f * c.sr));
        // High-pass chorus send @ 175 Hz and wet return @ 140 Hz so sub-bass stays tight & never rumbles
        c.hpAlpha = std::exp(-kFxTwoPi * 175.0f / c.sr);
        c.wetHpAlpha = std::exp(-kFxTwoPi * 140.0f / c.sr);
        const float lpCut = (c.mode == 2) ? 16500.0f : ((c.mode == 1) ? 14500.0f : 2400.0f);
        c.lpAlpha = std::exp(-kFxTwoPi * lpCut / c.sr);
        c.regen = (c.mode == 2) ? 0.0f : ((c.mode == 1) ? 0.14f : 0.16f);
        return c;
    }
};

struct StereoChorusState {
    std::vector<float> bufL;
    std::vector<float> bufR;
    int writeIdx = 0;
    float lfoPhase1 = 0.0f;
    float lfoPhase2 = 0.0f;
    float lfoPhase3 = 0.0f;
    float inHpLpL = 0.0f;
    float inHpLpR = 0.0f;
    float lpL = 0.0f;
    float lpR = 0.0f;
    float wetHpLpL = 0.0f;
    float wetHpLpR = 0.0f;
    float fbL = 0.0f;
    float fbR = 0.0f;
    float smoothedMix = 0.0f;

    void init(int sampleRate) {
        const int safeSr = std::max(8000, sampleRate);
        const int chorusLen = std::max(512, static_cast<int>(safeSr * 0.08f));
        bufL.assign(chorusLen, 0.0f);
        bufR.assign(chorusLen, 0.0f);
        reset();
    }

    void reset() {
        std::fill(bufL.begin(), bufL.end(), 0.0f);
        std::fill(bufR.begin(), bufR.end(), 0.0f);
        writeIdx = 0;
        lfoPhase1 = lfoPhase2 = lfoPhase3 = 0.0f;
        inHpLpL = inHpLpR = 0.0f;
        lpL = lpR = 0.0f;
        wetHpLpL = wetHpLpR = 0.0f;
        fbL = fbR = 0.0f;
        smoothedMix = 0.0f;
    }

    bool empty() const { return bufL.empty(); }

    void processSample(float& l, float& rS, bool active, const ChorusBlockCoeffs& c) {
        if (bufL.empty()) return;
        const int cSize = static_cast<int>(bufL.size());
        if (!active) {
            bufL[writeIdx] = 0.0f;
            bufR[writeIdx] = 0.0f;
            inHpLpL = l;
            inHpLpR = rS;
            lpL = lpR = 0.0f;
            wetHpLpL = wetHpLpR = 0.0f;
            fbL = fbR = 0.0f;
            smoothedMix = 0.0f;
            writeIdx = (writeIdx + 1) % cSize;
            return;
        }

        // High-pass send prevents sub-bass (<175 Hz) phase beating / thunder buildup
        inHpLpL = flushDenormal((1.0f - c.hpAlpha) * l + c.hpAlpha * inHpLpL);
        inHpLpR = flushDenormal((1.0f - c.hpAlpha) * rS + c.hpAlpha * inHpLpR);
        const float sendL = l - inHpLpL;
        const float sendR = rS - inHpLpR;

        // Unity-slope soft-clip inside feedback loop (zero runaway gain inflation)
        bufL[writeIdx] = flushDenormal(std::tanh(sendL + fbL * c.regen));
        bufR[writeIdx] = flushDenormal(std::tanh(sendR + fbR * c.regen));

        lfoPhase1 += c.rateRad;
        if (lfoPhase1 >= kFxTwoPi) lfoPhase1 -= kFxTwoPi;
        lfoPhase2 += c.rateRad * 1.37f;
        if (lfoPhase2 >= kFxTwoPi) lfoPhase2 -= kFxTwoPi;
        lfoPhase3 += c.rateRad * 0.79f;
        if (lfoPhase3 >= kFxTwoPi) lfoPhase3 -= kFxTwoPi;

        float wetL = 0.0f;
        float wetR = 0.0f;
        if (c.mode == 1) {
            // Lush Chorus: 3-voice staggered quadrature ensemble normalized to ~unity coherent sum
            const float dL1 = c.sr * (0.0075f + 0.0032f * c.depthNorm * std::sin(lfoPhase1));
            const float dR1 = c.sr * (0.0075f + 0.0032f * c.depthNorm * std::cos(lfoPhase1));
            const float dL2 = c.sr * (0.0155f + 0.0040f * c.depthNorm * std::sin(lfoPhase2 + 2.094f));
            const float dR2 = c.sr * (0.0155f + 0.0040f * c.depthNorm * std::cos(lfoPhase2 + 2.094f));
            const float ph3 = lfoPhase3 + 4.188f;
            const float dL3 = c.sr * (0.0235f + 0.0046f * c.depthNorm * (-std::sin(ph3)));
            const float dR3 = c.sr * (0.0235f + 0.0046f * c.depthNorm * std::cos(ph3));
            wetL = 0.54f * (readFracDelay(bufL, writeIdx, dL1) +
                            readFracDelay(bufL, writeIdx, dL2) +
                            readFracDelay(bufL, writeIdx, dL3));
            wetR = 0.54f * (readFracDelay(bufR, writeIdx, dR1) +
                            readFracDelay(bufR, writeIdx, dR2) +
                            readFracDelay(bufR, writeIdx, dR3));
        } else if (c.mode == 2) {
            // Dimension: Zero-wobble center, wide crystal counter-phase Side wings
            const float mod1 = 0.0024f * c.depthNorm * std::sin(lfoPhase1);
            const float mod2 = 0.0020f * c.depthNorm * std::cos(lfoPhase1);
            const float t1L = readFracDelay(bufL, writeIdx, c.sr * (0.0054f + mod1));
            const float t1R = readFracDelay(bufR, writeIdx, c.sr * (0.0054f - mod1));
            const float t2L = readFracDelay(bufL, writeIdx, c.sr * (0.0118f - mod2));
            const float t2R = readFracDelay(bufR, writeIdx, c.sr * (0.0118f + mod2));
            const float sideWings = 0.80f * (t1L - t1R) + 0.50f * (t2L - t2R);
            wetL = sideWings;
            wetR = -sideWings;
        } else {
            // Analog Warm: Smooth vintage BBD quadrature chorus
            const float dL = c.sr * (0.0125f + 0.0048f * c.depthNorm * std::sin(lfoPhase1));
            const float dR = c.sr * (0.0125f + 0.0048f * c.depthNorm * std::cos(lfoPhase1));
            wetL = readFracDelay(bufL, writeIdx, dL);
            wetR = readFracDelay(bufR, writeIdx, dR);
        }

        lpL = flushDenormal((1.0f - c.lpAlpha) * wetL + c.lpAlpha * lpL);
        lpR = flushDenormal((1.0f - c.lpAlpha) * wetR + c.lpAlpha * lpR);
        wetHpLpL = flushDenormal((1.0f - c.wetHpAlpha) * lpL + c.wetHpAlpha * wetHpLpL);
        wetHpLpR = flushDenormal((1.0f - c.wetHpAlpha) * lpR + c.wetHpAlpha * wetHpLpR);
        fbL = flushDenormal(lpL - wetHpLpL);
        fbR = flushDenormal(lpR - wetHpLpR);

        smoothedMix += c.mixAlpha * (c.targetMix - smoothedMix);
        const float m = smoothedMix;
        float dryScale = 1.0f - m * 0.24f;
        float wetScale = m * 0.85f;
        if (c.mode == 0) {
            dryScale = 1.0f - m * 0.36f;
            wetScale = m * 0.88f;
        } else if (c.mode == 1) {
            dryScale = 1.0f - m * 0.24f;
            wetScale = m * 1.02f;
        } else {
            dryScale = 1.0f - m * 0.14f;
            wetScale = m * 0.90f;
        }

        l = l * dryScale + fbL * wetScale;
        rS = rS * dryScale + fbR * wetScale;
        writeIdx = (writeIdx + 1) % cSize;
    }
};

struct DelayBlockCoeffs {
    int mode = 0;
    float targetSamples = 12480.0f;
    float maxSamples = 50000.0f;
    float feedback = 0.42f;
    float bounceFb = 0.50f;
    float targetMix = 0.40f;
    float smoothCoeff = 0.001f;
    float mixAlpha = 0.01f;
    float hpAlpha = 0.98f;
    float fbLpAlpha = 0.5f;
    float wowInc = 0.0f;
    float sr = 48000.0f;

    static DelayBlockCoeffs prepare(
        float sampleRate,
        int delayMode,
        float delayTimeMs,
        float delayFb,
        float delayMix,
        int bufSize
    ) {
        DelayBlockCoeffs c;
        c.sr = std::max(8000.0f, sampleRate);
        c.mode = std::clamp(delayMode, 0, 2);
        c.maxSamples = static_cast<float>(std::max(5, bufSize) - 4);
        c.targetSamples = std::clamp(delayTimeMs * 0.001f * c.sr, 1.0f, c.maxSamples);
        // Strictly <= 0.80 so feedback loops always decay cleanly with zero self-oscillation
        c.feedback = std::clamp(delayFb, 0.0f, 0.80f);
        // Ping-pong bounces every half-beat; scaled sqrt keeps L<->R bounce crisp without bass buildup
        c.bounceFb = (c.feedback < 0.01f) ? 0.0f : std::clamp(0.78f * std::sqrt(c.feedback), 0.12f, 0.76f);
        c.targetMix = std::clamp(delayMix, 0.0f, 1.0f);
        // 50ms delay-time slew avoids violent Doppler rumble during live slider drags
        c.smoothCoeff = 1.0f - std::exp(-1.0f / (0.050f * c.sr));
        c.mixAlpha = 1.0f - std::exp(-1.0f / (0.016f * c.sr));
        // Studio High-Pass (185 Hz Digital/Ping-Pong, 220 Hz Tape) blocks sub-bass thunder & DC offset
        const float hpCut = (c.mode == 1) ? 220.0f : 185.0f;
        c.hpAlpha = std::exp(-kFxTwoPi * hpCut / c.sr);
        const float lpCut = (c.mode == 1) ? 1850.0f : ((c.mode == 2) ? 9500.0f : 16500.0f);
        c.fbLpAlpha = std::exp(-kFxTwoPi * lpCut / c.sr);
        c.wowInc = (kFxTwoPi * 0.65f) / c.sr;
        return c;
    }
};

struct StereoDelayState {
    std::vector<float> bufL;
    std::vector<float> bufR;
    int writeIdx = 0;
    float inHpLpL = 0.0f;
    float inHpLpR = 0.0f;
    float fbLpL = 0.0f;
    float fbLpR = 0.0f;
    float fbHpLpL = 0.0f;
    float fbHpLpR = 0.0f;
    float smoothedDelaySamples = 12480.0f;
    float smoothedMix = 0.0f;
    float tapeWowPhase = 0.0f;

    void init(int sampleRate, float initialDelayMs = 260.0f) {
        const int safeSr = std::max(8000, sampleRate);
        const int delayLen = std::max(4096, static_cast<int>(safeSr * 1.1f));
        bufL.assign(delayLen, 0.0f);
        bufR.assign(delayLen, 0.0f);
        const float initSamples = std::clamp(
            initialDelayMs * 0.001f * static_cast<float>(safeSr),
            1.0f,
            static_cast<float>(delayLen - 4)
        );
        reset(initSamples);
    }

    void reset(float targetSamples) {
        std::fill(bufL.begin(), bufL.end(), 0.0f);
        std::fill(bufR.begin(), bufR.end(), 0.0f);
        writeIdx = 0;
        inHpLpL = inHpLpR = 0.0f;
        fbLpL = fbLpR = 0.0f;
        fbHpLpL = fbHpLpR = 0.0f;
        smoothedDelaySamples = targetSamples;
        smoothedMix = 0.0f;
        tapeWowPhase = 0.0f;
    }

    bool empty() const { return bufL.empty(); }
    int size() const { return static_cast<int>(bufL.size()); }

    void processSample(float& l, float& rS, bool active, const DelayBlockCoeffs& c) {
        if (bufL.empty()) return;
        const int dSize = static_cast<int>(bufL.size());
        if (!active) {
            bufL[writeIdx] = 0.0f;
            bufR[writeIdx] = 0.0f;
            inHpLpL = l;
            inHpLpR = rS;
            fbLpL = fbLpR = 0.0f;
            fbHpLpL = fbHpLpR = 0.0f;
            smoothedDelaySamples = c.targetSamples;
            smoothedMix = 0.0f;
            writeIdx = (writeIdx + 1) % dSize;
            return;
        }

        // High-pass send prevents sub-bass kicks from accumulating into roaring background thunder
        inHpLpL = flushDenormal((1.0f - c.hpAlpha) * l + c.hpAlpha * inHpLpL);
        inHpLpR = flushDenormal((1.0f - c.hpAlpha) * rS + c.hpAlpha * inHpLpR);
        const float sendL = l - inHpLpL;
        const float sendR = rS - inHpLpR;

        smoothedDelaySamples += c.smoothCoeff * (c.targetSamples - smoothedDelaySamples);
        smoothedMix += c.mixAlpha * (c.targetMix - smoothedMix);
        const float m = smoothedMix;
        const float dryGain = 1.0f - std::max(0.0f, m - 0.5f) * 0.44f;
        const float wetGain = m * 0.82f;

        if (c.mode == 1) {
            // Analog Tape: gentle capstan wow + warm 1.85 kHz head + unity-slope soft saturation (never self-oscillates)
            tapeWowPhase += c.wowInc;
            if (tapeWowPhase >= kFxTwoPi) tapeWowPhase -= kFxTwoPi;
            const float wow = c.sr * (0.0012f * std::sin(tapeWowPhase) + 0.00020f * std::sin(tapeWowPhase * 9.0f));
            const float delL = readFracDelay(bufL, writeIdx, std::clamp(smoothedDelaySamples + wow, 1.0f, c.maxSamples));
            const float delR = readFracDelay(bufR, writeIdx, std::clamp(smoothedDelaySamples - wow * 0.45f, 1.0f, c.maxSamples));
            fbLpL = flushDenormal((1.0f - c.fbLpAlpha) * delL + c.fbLpAlpha * fbLpL);
            fbLpR = flushDenormal((1.0f - c.fbLpAlpha) * delR + c.fbLpAlpha * fbLpR);
            fbHpLpL = flushDenormal((1.0f - c.hpAlpha) * fbLpL + c.hpAlpha * fbHpLpL);
            fbHpLpR = flushDenormal((1.0f - c.hpAlpha) * fbLpR + c.hpAlpha * fbHpLpR);
            const float cleanL = fbLpL - fbHpLpL;
            const float cleanR = fbLpR - fbHpLpR;
            // Unity small-signal gain (slope <= 1.0 at 0) guarantees unconditional stability at all feedback levels
            const float satL = std::tanh(cleanL * 1.15f) * (1.0f / 1.15f);
            const float satR = std::tanh(cleanR * 1.15f) * (1.0f / 1.15f);
            bufL[writeIdx] = flushDenormal(std::tanh(sendL + satL * c.feedback));
            bufR[writeIdx] = flushDenormal(std::tanh(sendR + satR * c.feedback));
            l = l * dryGain + satL * (wetGain * 1.04f);
            rS = rS * dryGain + satR * (wetGain * 1.04f);
        } else if (c.mode == 2) {
            // Ping-Pong: crisp alternating L <-> R echo on band-limited stereo send (zero bass bounce mud)
            const float halfSamples = std::clamp(smoothedDelaySamples * 0.5f, 1.0f, c.maxSamples);
            const float delL = readFracDelay(bufL, writeIdx, halfSamples);
            const float delR = readFracDelay(bufR, writeIdx, halfSamples);
            fbLpL = flushDenormal((1.0f - c.fbLpAlpha) * delL + c.fbLpAlpha * fbLpL);
            fbLpR = flushDenormal((1.0f - c.fbLpAlpha) * delR + c.fbLpAlpha * fbLpR);
            fbHpLpL = flushDenormal((1.0f - c.hpAlpha) * fbLpL + c.hpAlpha * fbHpLpL);
            fbHpLpR = flushDenormal((1.0f - c.hpAlpha) * fbLpR + c.hpAlpha * fbHpLpR);
            const float cleanL = fbLpL - fbHpLpL;
            const float cleanR = fbLpR - fbHpLpR;
            const float monoSend = (sendL + sendR) * 0.5f;
            bufL[writeIdx] = flushDenormal(std::tanh(monoSend * 0.90f + cleanR * c.bounceFb));
            bufR[writeIdx] = flushDenormal(std::tanh(cleanL * c.bounceFb));
            l = l * dryGain + cleanL * (wetGain * 1.04f);
            rS = rS * dryGain + cleanR * (wetGain * 1.04f);
        } else {
            // Digital Clean: pristine studio echo with 185 Hz feedback high-pass / DC blocker
            const float delL = readFracDelay(bufL, writeIdx, std::clamp(smoothedDelaySamples, 1.0f, c.maxSamples));
            const float delR = readFracDelay(bufR, writeIdx, std::clamp(smoothedDelaySamples, 1.0f, c.maxSamples));
            fbLpL = flushDenormal((1.0f - c.fbLpAlpha) * delL + c.fbLpAlpha * fbLpL);
            fbLpR = flushDenormal((1.0f - c.fbLpAlpha) * delR + c.fbLpAlpha * fbLpR);
            fbHpLpL = flushDenormal((1.0f - c.hpAlpha) * fbLpL + c.hpAlpha * fbHpLpL);
            fbHpLpR = flushDenormal((1.0f - c.hpAlpha) * fbLpR + c.hpAlpha * fbHpLpR);
            const float cleanL = fbLpL - fbHpLpL;
            const float cleanR = fbLpR - fbHpLpR;
            bufL[writeIdx] = flushDenormal(std::tanh(sendL + cleanL * c.feedback));
            bufR[writeIdx] = flushDenormal(std::tanh(sendR + cleanR * c.feedback));
            l = l * dryGain + cleanL * wetGain;
            rS = rS * dryGain + cleanR * wetGain;
        }

        writeIdx = (writeIdx + 1) % dSize;
    }
};

/**
 * Click-free 128-sample crossfade helper across 0-latency <-> 512-frame Convolver transitions.
 */
struct ConvolverToggleSmoother {
    static constexpr int kFadeLen = 128;
    int enterRem = 0;
    int exitRem = 0;
    float lastOutL = 0.0f;
    float lastOutR = 0.0f;

    void reset() {
        enterRem = 0;
        exitRem = 0;
        lastOutL = 0.0f;
        lastOutR = 0.0f;
    }

    void beginEnter() {
        enterRem = kFadeLen;
        exitRem = 0;
    }

    void beginExit() {
        exitRem = kFadeLen;
        enterRem = 0;
    }

    inline void applyEnter(const float* dryL, const float* dryR, float* convL, float* convR, int n) {
        if (n <= 0) return;
        for (int i = 0; i < n; ++i) {
            if (enterRem > 0) {
                const float dryRamp = static_cast<float>(enterRem) / static_cast<float>(kFadeLen);
                convL[i] += dryL[i] * dryRamp;
                convR[i] += dryR[i] * dryRamp;
                --enterRem;
            }
            lastOutL = convL[i];
            lastOutR = convR[i];
        }
    }

    inline void applyExit(const float* inL, const float* inR, float* outL, float* outR, int n) {
        if (n <= 0) return;
        for (int i = 0; i < n; ++i) {
            if (exitRem > 0) {
                const float dryMix = static_cast<float>(kFadeLen + 1 - exitRem) / static_cast<float>(kFadeLen);
                const float wetDecay = static_cast<float>(exitRem) / static_cast<float>(kFadeLen);
                lastOutL = flushDenormal(lastOutL * 0.985f);
                lastOutR = flushDenormal(lastOutR * 0.985f);
                outL[i] = inL[i] * dryMix + lastOutL * wetDecay;
                outR[i] = inR[i] * dryMix + lastOutR * wetDecay;
                --exitRem;
            } else {
                outL[i] = inL[i];
                outR[i] = inR[i];
                lastOutL = inL[i];
                lastOutR = inR[i];
            }
        }
    }
};

} // namespace rfx
