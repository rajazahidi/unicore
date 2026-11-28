#include "ResonanceWinEngine.h"

#include <algorithm>
#include <cmath>
#include <cstring>

namespace {
constexpr float kPi = rfx::kPi;
constexpr float kSpectrumFreqs[28] = {
    31.0f, 42.0f, 56.0f, 75.0f, 100.0f, 130.0f, 175.0f,
    230.0f, 300.0f, 400.0f, 520.0f, 680.0f, 900.0f, 1180.0f,
    1550.0f, 2000.0f, 2600.0f, 3400.0f, 4400.0f, 5600.0f, 7100.0f,
    8800.0f, 10500.0f, 12200.0f, 13800.0f, 15200.0f, 16800.0f, 18500.0f
};

inline float safeFinite(float v, float fallback = 0.0f) {
    return std::isfinite(v) ? v : fallback;
}
using rfx::flushDenormal;
} // namespace

// ============================================================================
// WinDspCore
// ============================================================================

WinDspCore::WinDspCore() {
    haasDelayR_.resize(kMaxDelaySamples, 0.0f);
    crossDelayL_.resize(kBinauralDelayLen, 0.0f);
    crossDelayR_.resize(kBinauralDelayLen, 0.0f);
    panItdL_.resize(kBinauralDelayLen, 0.0f);
    panItdR_.resize(kBinauralDelayLen, 0.0f);
    tmpConvL_.resize(8192, 0.0f);
    tmpConvR_.resize(8192, 0.0f);
    meterRing_.fill(0.0f);
    for (auto& b : spectrumBins_) b.store(0.0f, std::memory_order_relaxed);
    setSampleRate(48000);
}

void WinDspCore::setSampleRate(int sampleRate) {
    const int safeSr = std::clamp(sampleRate, 8000, 384000);
    std::lock_guard<std::mutex> rbLock(rtBufferMutex_);
    std::lock_guard<std::mutex> lock(paramWriteMutex_);
    sampleRate_.store(safeSr, std::memory_order_release);
    reverb_.init(safeSr);
    initChorusAndDelayBuffers(safeSr);
    resetFxBuffers();
    const int presetToLoad = (lastBuiltInConvolverIdx_.load(std::memory_order_acquire) >= 0)
        ? lastBuiltInConvolverIdx_.load(std::memory_order_acquire)
        : stagedParams_.convolverPreset;
    if (!usingCustomIr_.load(std::memory_order_acquire)) {
        convolver_.loadBuiltInPreset(presetToLoad, safeSr);
        lastBuiltInConvolverIdx_.store(presetToLoad, std::memory_order_release);
    } else {
        applyStoredCustomIrForSampleRate(safeSr);
    }
    coeffsDirty_ = true;
}

void WinDspCore::initChorusAndDelayBuffers(int sampleRate) {
    const int safeSr = std::max(8000, sampleRate);
    chorus_.init(safeSr);
    delay_.init(safeSr, static_cast<float>(stagedParams_.delayTimeMs));
}

void WinDspCore::resetFxBuffers() {
    eqBank_.reset();
    bassShelf_.resetState();
    bassCharacterFilter_.resetState();
    clarityShelf_.resetState();
    clarityVoicingFilter_.resetState();
    antiMaskFilter_.resetState();
    tubeBodyShelf_.resetState();
    loudShelfLow_.resetState();
    loudShelfHigh_.resetState();
    subProtector_.reset();
    exciterInLpL_ = exciterInLpR_ = 0.0f;
    exciterDcLpL_ = exciterDcLpR_ = 0.0f;
    tubeStage_.reset();
    convolver_.resetState();
    convToggleSmoother_.reset();
    for (auto& ex : extraTonalStates_) ex.reset();
    std::fill(haasDelayR_.begin(), haasDelayR_.end(), 0.0f);
    haasIdx_ = 0;
    std::fill(crossDelayL_.begin(), crossDelayL_.end(), 0.0f);
    std::fill(crossDelayR_.begin(), crossDelayR_.end(), 0.0f);
    std::fill(panItdL_.begin(), panItdL_.end(), 0.0f);
    std::fill(panItdR_.begin(), panItdR_.end(), 0.0f);
    crossIdx_ = 0;
    panItdIdx_ = 0;
    crossLpL_ = crossLpR_ = shadowLpL_ = shadowLpR_ = 0.0f;
    orbitPhase_ = 0.85f;
    reverb_.reset();
    chorus_.reset();
    if (!delay_.empty()) {
        const float sr = static_cast<float>(std::max(8000, sampleRate_.load(std::memory_order_acquire)));
        const int delayMs = stagedParams_.delayTimeMs;
        delay_.reset(std::clamp(
            static_cast<float>(delayMs) * 0.001f * sr,
            1.0f,
            static_cast<float>(std::max<int>(5, delay_.size()) - 4)
        ));
    }
    limiter_.reset();
    gainSmoother_.reset(1.0f);
    prevPureBassEnabled_ = false;
    prevClarityEnabled_ = false;
    prevConvolverEnabled_ = false;
    prevChorusActive_ = false;
    prevDelayActive_ = false;
    prevLoudnessEnabled_ = false;
}

void WinDspCore::setRuntimeGainOffsetDb(float offsetDb) {
    const float safe = std::isfinite(offsetDb) ? std::clamp(offsetDb, -36.0f, 12.0f) : 0.0f;
    runtimeGainOffsetDb_.store(safe, std::memory_order_release);
}

WinDspParams WinDspCore::sanitizeDspParams(const WinDspParams& params) {
    WinDspParams clean = params;
    for (int i = 0; i < 10; ++i) {
        clean.eqBandsDb[i] = rfx::clampFinite(clean.eqBandsDb[i], -24.0f, 24.0f, 0.0f);
    }
    clean.surroundLevel = rfx::clampFinite(clean.surroundLevel, 0.0f, 1.0f, 0.35f);
    clean.crossfeedLevel = rfx::clampFinite(clean.crossfeedLevel, 0.0f, 1.0f, 0.3f);
    clean.panBalance = rfx::clampFinite(clean.panBalance, -1.0f, 1.0f, 0.0f);
    clean.orbitSpeed = rfx::clampFinite(clean.orbitSpeed, 0.1f, 1.5f, 0.35f);
    clean.pureBassGainDb = rfx::clampFinite(clean.pureBassGainDb, 0.0f, 24.0f, 0.0f);
    clean.clarityGainDb = rfx::clampFinite(clean.clarityGainDb, 0.0f, 24.0f, 0.0f);
    clean.tubeWarmth = rfx::clampFinite(clean.tubeWarmth, 0.0f, 1.0f, 0.4f);
    clean.limiterThresholdDb = rfx::clampFinite(clean.limiterThresholdDb, -24.0f, 0.0f, -0.5f);
    clean.preampDb = rfx::clampFinite(clean.preampDb, -36.0f, 24.0f, 0.0f);
    clean.loudnessGainDb = rfx::clampFinite(clean.loudnessGainDb, -36.0f, 24.0f, 0.0f);
    clean.chorusRateHz = rfx::clampFinite(clean.chorusRateHz, 0.1f, 5.0f, 0.85f);
    clean.chorusDepth = rfx::clampFinite(clean.chorusDepth, 0.0f, 1.0f, 0.6f);
    clean.chorusMix = rfx::clampFinite(clean.chorusMix, 0.0f, 1.0f, 0.5f);
    clean.delayFeedback = rfx::clampFinite(clean.delayFeedback, 0.0f, 0.85f, 0.42f);
    clean.delayMix = rfx::clampFinite(clean.delayMix, 0.0f, 1.0f, 0.4f);
    return clean;
}

void WinDspCore::updateParameters(const WinDspParams& params, bool allowBlockingConvolverLoad) {
    WinDspParams clean = sanitizeDspParams(params);

    bool shouldLoadBuiltIn = false;
    int builtInIdx = 0;
    {
        std::lock_guard<std::mutex> lock(paramWriteMutex_);
        bool convolverPresetChanged = (clean.convolverPreset != stagedParams_.convolverPreset);
        uint32_t s = paramSeq_.load(std::memory_order_relaxed);
        if (s & 1u) ++s;
        paramSeq_.store(s + 1u, std::memory_order_release);
        std::atomic_thread_fence(std::memory_order_release);
        stagedParams_ = clean;
        std::atomic_thread_fence(std::memory_order_release);
        paramSeq_.store(s + 2u, std::memory_order_release);
        if (allowBlockingConvolverLoad && !usingCustomIr_.load(std::memory_order_acquire) && convolverPresetChanged) {
            shouldLoadBuiltIn = true;
            builtInIdx = clean.convolverPreset;
            lastBuiltInConvolverIdx_.store(clean.convolverPreset, std::memory_order_release);
        }
    }
    if (shouldLoadBuiltIn) {
        convolver_.loadBuiltInPreset(builtInIdx, sampleRate_.load(std::memory_order_acquire));
    }
}

void WinDspCore::updateParametersRt(const WinDspParams& params) {
    // Lock-free RT path invoked directly on the audio thread immediately before processBlock()
    rtParams_ = sanitizeDspParams(params);
    rtAppliedSeq_ = paramSeq_.load(std::memory_order_acquire);
    coeffsDirty_ = true;
}

void WinDspCore::applyStoredCustomIrForSampleRate(int targetSr) {
    std::vector<float> srcL;
    std::vector<float> srcR;
    int srcSr = 48000;
    {
        std::lock_guard<std::mutex> lock(irStoreMutex_);
        if (customIrRawL_.empty()) return;
        srcL = customIrRawL_;
        srcR = customIrRawR_;
        srcSr = customIrRawSr_;
    }
    const int safeTargetSr = std::clamp(targetSr, 8000, 384000);
    if (srcSr == safeTargetSr || srcL.size() < 2) {
        convolver_.loadImpulseResponse(srcL.data(), srcR.data(), static_cast<int>(srcL.size()));
        return;
    }
    const int inLen = static_cast<int>(srcL.size());
    const int outLen = std::clamp(
        static_cast<int>(std::llround(static_cast<double>(inLen) * safeTargetSr / static_cast<double>(srcSr))),
        1, 65536
    );
    std::vector<float> resL(static_cast<size_t>(outLen));
    std::vector<float> resR(static_cast<size_t>(outLen));
    const double ratio = static_cast<double>(srcSr) / static_cast<double>(safeTargetSr);
    const float normScale = static_cast<float>(srcSr) / static_cast<float>(safeTargetSr);
    for (int i = 0; i < outLen; ++i) {
        const double pos = i * ratio;
        const int idx0 = std::min(inLen - 1, static_cast<int>(std::floor(pos)));
        const int idx1 = std::min(inLen - 1, idx0 + 1);
        const float frac = static_cast<float>(pos - idx0);
        resL[i] = (srcL[idx0] + frac * (srcL[idx1] - srcL[idx0])) * normScale;
        resR[i] = (srcR[idx0] + frac * (srcR[idx1] - srcR[idx0])) * normScale;
    }
    convolver_.loadImpulseResponse(resL.data(), resR.data(), outLen);
}

void WinDspCore::resetPlaybackTails() {
    std::lock_guard<std::mutex> rbLock(rtBufferMutex_);
    std::lock_guard<std::mutex> lock(paramWriteMutex_);
    resetFxBuffers();
}

void WinDspCore::loadCustomImpulseResponse(const float* irLeft, const float* irRight, int length, int irSampleRate) {
    if (!irLeft || length <= 0) return;
    const int clampedLen = std::min(length, 65536);
    const int curSr = std::clamp(sampleRate_.load(std::memory_order_acquire), 8000, 384000);
    const int effectiveIrSr = (irSampleRate > 0) ? std::clamp(irSampleRate, 8000, 384000) : curSr;
    {
        std::lock_guard<std::mutex> lock(irStoreMutex_);
        customIrRawSr_ = effectiveIrSr;
        customIrRawL_.assign(irLeft, irLeft + clampedLen);
        if (irRight) {
            customIrRawR_.assign(irRight, irRight + clampedLen);
        } else {
            customIrRawR_ = customIrRawL_;
        }
    }
    usingCustomIr_.store(true, std::memory_order_release);
    applyStoredCustomIrForSampleRate(curSr);
}

void WinDspCore::loadBuiltInConvolverPreset(int presetIndex) {
    usingCustomIr_.store(false, std::memory_order_release);
    lastBuiltInConvolverIdx_.store(presetIndex, std::memory_order_release);
    convolver_.loadBuiltInPreset(presetIndex, sampleRate_.load(std::memory_order_acquire));
}

void WinDspCore::rebuildCoefficients() {
    const float sr = static_cast<float>(sampleRate_.load(std::memory_order_acquire));
    eqBank_.setTargets(sr, rtParams_.masterEnabled && rtParams_.eqEnabled, rtParams_.eqBandsDb);

    const float bassScale = rtParams_.pureBassEnabled ? rtParams_.pureBassGainDb : 0.0f;
    rfx::configurePureBassFilters(
        rtParams_.masterEnabled && rtParams_.pureBassEnabled,
        rtParams_.pureBassMode,
        bassScale,
        sr,
        bassShelf_,
        bassCharacterFilter_,
        antiMaskFilter_
    );

    const float clarityGain = rtParams_.clarityEnabled ? rtParams_.clarityGainDb : 0.0f;
    rfx::configureClarityFilters(
        rtParams_.masterEnabled && rtParams_.clarityEnabled,
        rtParams_.clarityMode,
        clarityGain,
        sr,
        clarityShelf_,
        clarityVoicingFilter_
    );

    rfx::configureTubeAndLoudnessFilters(
        rtParams_.masterEnabled != 0,
        rtParams_.tubeEnabled != 0,
        rtParams_.tubeWarmth,
        rtParams_.loudnessEnabled != 0,
        rtParams_.loudnessGainDb,
        sr,
        tubeBodyShelf_,
        loudShelfLow_,
        loudShelfHigh_
    );
    coeffsDirty_ = false;
}

void WinDspCore::processBlock(float* chL, float* chR, int totalFrames) {
    if (!chL || !chR || totalFrames <= 0) return;

    std::unique_lock<std::mutex> rbLock(rtBufferMutex_, std::try_to_lock);
    if (!rbLock.owns_lock()) {
        // Output silence while buffers resize rather than bypassing limiter/preamp
        std::fill_n(chL, totalFrames, 0.0f);
        std::fill_n(chR, totalFrames, 0.0f);
        return;
    }

    const uint32_t curSeq = paramSeq_.load(std::memory_order_acquire);
    if (curSeq != rtAppliedSeq_ && !(curSeq & 1u)) {
        WinDspParams snap;
        for (int attempt = 0; attempt < 3; ++attempt) {
            const uint32_t s1 = paramSeq_.load(std::memory_order_acquire);
            if (s1 & 1u) continue;
            std::atomic_thread_fence(std::memory_order_acquire);
            snap = stagedParams_;
            std::atomic_thread_fence(std::memory_order_acquire);
            const uint32_t s2 = paramSeq_.load(std::memory_order_acquire);
            if (s1 == s2) {
                rtParams_ = snap;
                coeffsDirty_ = true;
                rtAppliedSeq_ = s1;
                break;
            }
        }
    }
    if (coeffsDirty_) rebuildCoefficients();
    const WinDspParams params = rtParams_;
    const int srInt = std::max(8000, sampleRate_.load(std::memory_order_acquire));
    const float srSafe = static_cast<float>(srInt);
    const float rtGainLin = std::pow(10.0f, std::clamp(runtimeGainOffsetDb_.load(std::memory_order_acquire), -36.0f, 12.0f) / 20.0f);

    for (int offset = 0; offset < totalFrames; ) {
        const int numFrames = std::min(totalFrames - offset, static_cast<int>(tmpConvL_.size()));
        float* blkL = chL + offset;
        float* blkR = chR + offset;

        if (params.masterEnabled) {
            if (!params.pureBassEnabled && prevPureBassEnabled_) {
                subProtector_.reset();
                bassShelf_.resetState();
                bassCharacterFilter_.resetState();
                antiMaskFilter_.resetState();
            }
            prevPureBassEnabled_ = (params.pureBassEnabled != 0);
            if (!params.clarityEnabled && prevClarityEnabled_) {
                clarityVoicingFilter_.resetState();
                clarityShelf_.resetState();
            }
            prevClarityEnabled_ = (params.clarityEnabled != 0);

            const float subAlpha = std::exp(-2.0f * kPi * 300.0f / srSafe);
            const rfx::ClarityExciterSpec excSpec = rfx::prepareClarityExciter(
                srSafe, params.clarityEnabled != 0, params.clarityMode, params.clarityGainDb
            );

            // 1. Sanitize + 10-Band EQ + Split-Band Bass Protector + Anti-Masking Vocal Lift + Harmonic Clarity Exciter
            for (int i = 0; i < numFrames; ++i) {
                float l = safeFinite(blkL[i], 0.0f);
                float rS = safeFinite(blkR[i], 0.0f);
                eqBank_.processStereo(l, rS);
                if (params.pureBassEnabled) {
                    subProtector_.processSample(l, rS, bassShelf_, bassCharacterFilter_, antiMaskFilter_, subAlpha);
                }
                if (params.clarityEnabled) {
                    clarityVoicingFilter_.processStereo(l, rS);
                    clarityShelf_.processStereo(l, rS);
                    rfx::processClarityExciterSample(
                        l, rS, excSpec, exciterInLpL_, exciterInLpR_, exciterDcLpL_, exciterDcLpR_
                    );
                } else {
                    exciterInLpL_ = l;
                    exciterInLpR_ = rS;
                    exciterDcLpL_ = exciterDcLpR_ = 0.0f;
                }
                blkL[i] = l;
                blkR[i] = rS;
            }

            // 2. Partitioned FFT Convolver with click-free latency crossfade
            if (params.convolverEnabled) {
                if (!prevConvolverEnabled_) {
                    convToggleSmoother_.beginEnter();
                    prevConvolverEnabled_ = true;
                }
                convolver_.process(blkL, blkR, tmpConvL_.data(), tmpConvR_.data(), numFrames, 0.85f);
                convToggleSmoother_.applyEnter(blkL, blkR, tmpConvL_.data(), tmpConvR_.data(), numFrames);
            } else {
                if (prevConvolverEnabled_) {
                    convolver_.resetState();
                    convToggleSmoother_.beginExit();
                    prevConvolverEnabled_ = false;
                }
                convToggleSmoother_.applyExit(blkL, blkR, tmpConvL_.data(), tmpConvR_.data(), numFrames);
            }

            // 3. Tube -> Binaural/Pan/8D Orbit -> Zero-Comb 3D Surround -> Chorus -> Delay -> Reverb -> Limiter
            const float sr = srSafe;
            const float autoTrimDb = rfx::computeHeadroomAutoTrimDb(
                true,
                params.eqEnabled != 0,
                params.eqBandsDb,
                params.clarityEnabled != 0,
                params.clarityGainDb,
                params.pureBassEnabled != 0,
                params.pureBassGainDb,
                params.tubeEnabled != 0,
                params.tubeWarmth,
                params.preampDb
            );
            const float autoTrimLin = std::pow(10.0f, autoTrimDb / 20.0f);
            const float activeLoudDb = params.loudnessEnabled ? std::clamp(params.loudnessGainDb, 0.0f, 12.0f) : 0.0f;
            const float totalUserDb = std::clamp(params.preampDb + activeLoudDb, -36.0f, 24.0f);
            const float paramGainLin = (std::abs(totalUserDb) > 0.01f)
                ? std::pow(10.0f, totalUserDb / 20.0f)
                : 1.0f;
            const float loudGain = paramGainLin * autoTrimLin * rtGainLin;
            const float tubeDcAlpha = std::exp(-2.0f * kPi * 10.0f / sr);
            const float tubeHfAlpha = std::exp(-2.0f * kPi * 11500.0f / sr);
            const float gainAlpha = rfx::GainSmoother::stepCoeff(sr, 0.016f);
            const rfx::LimiterCoeffs limCoeffs = rfx::PeakHoldLimiter::prepare(sr, params.limiterThresholdDb);
            const int haasOffset = std::clamp(static_cast<int>(sr * 0.011f), 40, kMaxDelaySamples - 16);
            const int activeReverbPreset = (params.reverbEnabled && params.reverbPreset > 0) ? std::clamp(params.reverbPreset, 1, 6) : 0;

            const bool chorusActive = params.chorusEnabled && params.chorusMix > 0.005f && !chorus_.empty();
            if (!chorusActive && prevChorusActive_) {
                chorus_.reset();
            }
            prevChorusActive_ = chorusActive;
            const rfx::ChorusBlockCoeffs chorusCoeffs = rfx::ChorusBlockCoeffs::prepare(
                sr, params.chorusMode, params.chorusRateHz, params.chorusDepth, params.chorusMix
            );

            const int ambStep = std::max(1, static_cast<int>(std::lround(sr / 48000.0f)));
            const int ambDelta = std::max(4, static_cast<int>(std::lround(10.0f * sr / 48000.0f)));
            const bool delayActive = params.delayEnabled && params.delayMix > 0.005f && !delay_.empty();
            const rfx::DelayBlockCoeffs delayCoeffs = rfx::DelayBlockCoeffs::prepare(
                sr, params.delayMode, static_cast<float>(params.delayTimeMs), params.delayFeedback, params.delayMix, delay_.size()
            );
            if (!delayActive && prevDelayActive_) {
                delay_.reset(delayCoeffs.targetSamples);
            }
            prevDelayActive_ = delayActive;
            if (!params.loudnessEnabled && prevLoudnessEnabled_) {
                loudShelfLow_.resetState();
                loudShelfHigh_.resetState();
            }
            prevLoudnessEnabled_ = (params.loudnessEnabled != 0);

            for (int i = 0; i < numFrames; ++i) {
                float l = tmpConvL_[i];
                float rS = tmpConvR_[i];

                tubeStage_.processSample(
                    l, rS,
                    params.tubeEnabled != 0,
                    params.tubeWarmth,
                    tubeDcAlpha,
                    tubeHfAlpha,
                    tubeBodyShelf_
                );

                rfx::processBinauralStageSample(
                    l, rS,
                    params.binauralEnabled != 0,
                    params.binauralMode,
                    params.crossfeedLevel,
                    params.panBalance,
                    params.orbitSpeed,
                    sr,
                    crossDelayL_, crossDelayR_, crossIdx_, crossLpL_, crossLpR_,
                    panItdL_, panItdR_, panItdIdx_, shadowLpL_, shadowLpR_,
                    orbitPhase_
                );

                rfx::processSurroundStageSample(
                    l, rS,
                    params.surroundEnabled != 0,
                    params.surroundLevel,
                    haasOffset,
                    ambStep,
                    ambDelta,
                    haasDelayR_,
                    haasIdx_
                );

                // Stereo Chorus (High-passed send + unity-slope regen)
                chorus_.processSample(l, rS, chorusActive, chorusCoeffs);

                // Stereo / Analog Tape / Ping-Pong Delay (High-passed send & feedback)
                delay_.processSample(l, rS, delayActive, delayCoeffs);

                // Schroeder Algorithmic Room Reverb
                if (activeReverbPreset > 0) {
                    reverb_.processSample(l, rS, activeReverbPreset);
                } else {
                    reverb_.clearIdleSample();
                }

                // Digital Loudness Fletcher-Munson Contour & Gain + Peak-Hold Smooth Limiter
                if (params.loudnessEnabled) {
                    loudShelfLow_.processStereo(l, rS);
                    loudShelfHigh_.processStereo(l, rS);
                }
                const float gCur = gainSmoother_.next(loudGain, gainAlpha);
                l *= gCur;
                rS *= gCur;
                limiter_.processStereo(l, rS, limCoeffs);
                blkL[i] = l;
                blkR[i] = rS;
            }
            prevMasterEnabled_ = true;
        } else {
            if (prevMasterEnabled_) {
                resetFxBuffers();
                prevMasterEnabled_ = false;
            }
            const float gainAlpha = rfx::GainSmoother::stepCoeff(srSafe, 0.016f);
            for (int i = 0; i < numFrames; ++i) {
                const float gCur = gainSmoother_.next(rtGainLin, gainAlpha);
                const float outL = safeFinite(blkL[i], 0.0f) * gCur;
                const float outR = safeFinite(blkR[i], 0.0f) * gCur;
                blkL[i] = std::clamp(outL, -0.998f, 0.998f);
                blkR[i] = std::clamp(outR, -0.998f, 0.998f);
            }
        }

        // Update live 28-band spectrum meter from processed audio using windowed Goertzel resonators
        const int meterHop = std::clamp(static_cast<int>(std::lround(srSafe * 0.020f)), 160, 4096);
        const int meterN = std::clamp(static_cast<int>(std::lround(256.0f * srSafe / 48000.0f)), 256, 1024);
        constexpr int kRingMaskSize = 2048;
        for (int i = 0; i < numFrames; ++i) {
            meterRing_[meterWritePos_] = 0.5f * (blkL[i] + blkR[i]);
            meterWritePos_ = (meterWritePos_ + 1) % kRingMaskSize;
            meterSampleCounter_++;
            if (meterSampleCounter_ >= meterHop) {
                meterSampleCounter_ = 0;
                const float sr = srSafe;
                for (int b = 0; b < 28; ++b) {
                    const float omega = 2.0f * kPi * std::min(kSpectrumFreqs[b], sr * 0.46f) / sr;
                    const float coeff = 2.0f * std::cos(omega);
                    float q1 = 0.0f, q2 = 0.0f;
                    int readIdx = (meterWritePos_ - meterN + kRingMaskSize) % kRingMaskSize;
                    for (int k = 0; k < meterN; ++k) {
                        const float win = 0.5f * (1.0f - std::cos(2.0f * kPi * k / (meterN - 1)));
                        const float x = meterRing_[readIdx] * win;
                        const float q0 = coeff * q1 - q2 + x;
                        q2 = q1;
                        q1 = q0;
                        readIdx = (readIdx + 1) % kRingMaskSize;
                    }
                    const float magSq = std::max(0.0f, q1 * q1 + q2 * q2 - q1 * q2 * coeff);
                    const float rms = std::sqrt(magSq) / (static_cast<float>(meterN) * 0.25f);
                    const float db = 20.0f * std::log10(std::max(1e-5f, rms));
                    const float norm = std::clamp((db + 54.0f) / 52.0f, 0.04f, 1.0f);
                    const float prev = spectrumBins_[b].load(std::memory_order_relaxed);
                    const float smooth = (norm > prev) ? (0.4f * prev + 0.6f * norm) : (0.78f * prev + 0.22f * norm);
                    spectrumBins_[b].store(smooth, std::memory_order_relaxed);
                }
            }
        }
        offset += numFrames;
    }
}

void WinDspCore::getSpectrumLevels(float* out28) {
    if (!out28) return;
    for (int i = 0; i < 28; ++i) {
        out28[i] = spectrumBins_[i].load(std::memory_order_relaxed);
    }
}

void WinDspCore::processExtraChannelBlock(int extraSlot, float* ch, int numFrames, bool isLfe) {
    if (!ch || numFrames <= 0 || extraSlot < 0 || extraSlot >= kMaxExtraTonalCh) return;
    if (!rtParams_.masterEnabled) return;
    auto& st = extraTonalStates_[static_cast<size_t>(extraSlot)];
    const bool applyBass = (rtParams_.pureBassEnabled != 0);
    const bool applyClarity = (!isLfe && rtParams_.clarityEnabled != 0);
    const bool applyLoud = (rtParams_.loudnessEnabled != 0);

    for (int i = 0; i < numFrames; ++i) {
        float x = safeFinite(ch[i], 0.0f);
        for (int b = 0; b < 10; ++b) {
            if (eqBank_.isBandActive(b)) {
                x = st.eq[static_cast<size_t>(b)].processDf1(eqBank_.filters[static_cast<size_t>(b)], x);
            } else {
                st.eq[static_cast<size_t>(b)].prime(x);
            }
        }
        if (applyBass) {
            x = st.bassShelf.processDf1(bassShelf_, x);
        } else {
            st.bassShelf.prime(x);
        }
        if (applyClarity) {
            x = st.clarityVoicing.processDf1(clarityVoicingFilter_, x);
            x = st.clarityShelf.processDf1(clarityShelf_, x);
        } else {
            st.clarityVoicing.prime(x);
            st.clarityShelf.prime(x);
        }
        if (applyLoud) {
            x = st.loudLow.processDf1(loudShelfLow_, x);
            if (!isLfe) {
                x = st.loudHigh.processDf1(loudShelfHigh_, x);
            } else {
                st.loudHigh.prime(x);
            }
        } else {
            st.loudLow.prime(x);
            st.loudHigh.prime(x);
        }
        ch[i] = x;
    }
}
