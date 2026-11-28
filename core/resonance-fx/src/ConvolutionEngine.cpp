#include "ConvolutionEngine.h"
#include <algorithm>
#include <cmath>
#include <cstring>

namespace {
constexpr float kPi = 3.14159265358979323846f;
}

ConvolutionEngine::ConvolutionEngine() {
    inQueueL_.resize(kBlockSize, 0.0f);
    inQueueR_.resize(kBlockSize, 0.0f);
    outQueueL_.resize(kBlockSize, 0.0f);
    outQueueR_.resize(kBlockSize, 0.0f);
    overlapL_.resize(kBlockSize, 0.0f);
    overlapR_.resize(kBlockSize, 0.0f);
    dryDelayL_.resize(kBlockSize, 0.0f);
    dryDelayR_.resize(kBlockSize, 0.0f);
    fftBuf_.resize(kFftSize);
    accumSpecL_.resize(kFftSize);
    accumSpecR_.resize(kFftSize);
    blockOutL_.resize(kBlockSize, 0.0f);
    blockOutR_.resize(kBlockSize, 0.0f);

    fdlL_.assign(kMaxPartitions, std::vector<std::complex<float>>(kFftSize, {0.0f, 0.0f}));
    fdlR_.assign(kMaxPartitions, std::vector<std::complex<float>>(kFftSize, {0.0f, 0.0f}));

    loadBuiltInPreset(0, 48000);
}

void ConvolutionEngine::resetState() {
    std::lock_guard<std::mutex> lock(irMutex_);
    inQueueFill_ = 0;
    fdlIndex_ = 0;
    hasProcessedAudio_ = false;
    fadeInPending_ = false;
    fadeInRemaining_ = 0;
    std::fill(inQueueL_.begin(), inQueueL_.end(), 0.0f);
    std::fill(inQueueR_.begin(), inQueueR_.end(), 0.0f);
    std::fill(outQueueL_.begin(), outQueueL_.end(), 0.0f);
    std::fill(outQueueR_.begin(), outQueueR_.end(), 0.0f);
    std::fill(overlapL_.begin(), overlapL_.end(), 0.0f);
    std::fill(overlapR_.begin(), overlapR_.end(), 0.0f);
    std::fill(dryDelayL_.begin(), dryDelayL_.end(), 0.0f);
    std::fill(dryDelayR_.begin(), dryDelayR_.end(), 0.0f);
    for (auto& v : fdlL_) std::fill(v.begin(), v.end(), std::complex<float>{0.0f, 0.0f});
    for (auto& v : fdlR_) std::fill(v.begin(), v.end(), std::complex<float>{0.0f, 0.0f});
}

void ConvolutionEngine::fft(std::vector<std::complex<float>>& a, bool inverse) {
    const int n = static_cast<int>(a.size());
    for (int i = 1, j = 0; i < n; ++i) {
        int bit = n >> 1;
        for (; j & bit; bit >>= 1) {
            j ^= bit;
        }
        j ^= bit;
        if (i < j) std::swap(a[i], a[j]);
    }

    for (int len = 2; len <= n; len <<= 1) {
        const float ang = 2.0f * kPi / static_cast<float>(len) * (inverse ? 1.0f : -1.0f);
        const std::complex<float> wlen(std::cos(ang), std::sin(ang));
        for (int i = 0; i < n; i += len) {
            std::complex<float> w(1.0f, 0.0f);
            const int half = len >> 1;
            for (int j = 0; j < half; ++j) {
                std::complex<float> u = a[i + j];
                std::complex<float> v = a[i + j + half] * w;
                a[i + j] = u + v;
                a[i + j + half] = u - v;
                w *= wlen;
            }
        }
    }

    if (inverse) {
        const float invN = 1.0f / static_cast<float>(n);
        for (auto& x : a) x *= invN;
    }
}

void ConvolutionEngine::loadImpulseResponse(const float* irLeft, const float* irRight, int length) {
    if (!irLeft || length <= 0) return;
    const float* irR = irRight ? irRight : irLeft;

    const int cappedLen = std::min(length, kBlockSize * kMaxPartitions);
    const int numParts = std::max(1, (cappedLen + kBlockSize - 1) / kBlockSize);

    // Compute RMS/energy scale across both L/R channels to keep unity loudness gain
    double sumSq = 0.0;
    for (int i = 0; i < cappedLen; ++i) {
        const float sL = std::isfinite(irLeft[i]) ? std::clamp(irLeft[i], -16.0f, 16.0f) : 0.0f;
        const float sR = std::isfinite(irR[i]) ? std::clamp(irR[i], -16.0f, 16.0f) : 0.0f;
        sumSq += 0.5 * (static_cast<double>(sL) * sL + static_cast<double>(sR) * sR);
    }
    const float rms = static_cast<float>(std::sqrt(std::max(1e-8, sumSq)));
    const float normScale = (rms > 0.001f) ? std::min(2.0f, 1.0f / rms) : 1.0f;

    PartitionSet nextParts;
    nextParts.numPartitions = numParts;
    nextParts.irSpecL.resize(numParts, std::vector<std::complex<float>>(kFftSize, {0.0f, 0.0f}));
    nextParts.irSpecR.resize(numParts, std::vector<std::complex<float>>(kFftSize, {0.0f, 0.0f}));

    std::vector<std::complex<float>> spec(kFftSize);
    for (int p = 0; p < numParts; ++p) {
        const int start = p * kBlockSize;
        const int count = std::min(kBlockSize, cappedLen - start);

        std::fill(spec.begin(), spec.end(), std::complex<float>{0.0f, 0.0f});
        for (int i = 0; i < count; ++i) {
            const float rawL = irLeft[start + i];
            const float cleanL = std::isfinite(rawL) ? std::clamp(rawL, -16.0f, 16.0f) : 0.0f;
            spec[i] = {cleanL * normScale, 0.0f};
        }
        fft(spec, false);
        nextParts.irSpecL[p] = spec;

        std::fill(spec.begin(), spec.end(), std::complex<float>{0.0f, 0.0f});
        for (int i = 0; i < count; ++i) {
            const float rawR = irR[start + i];
            const float cleanR = std::isfinite(rawR) ? std::clamp(rawR, -16.0f, 16.0f) : 0.0f;
            spec[i] = {cleanR * normScale, 0.0f};
        }
        fft(spec, false);
        nextParts.irSpecR[p] = spec;
    }

    PartitionSet oldParts;
    {
        std::lock_guard<std::mutex> lock(irMutex_);
        const bool wasLive = hasValidIr_ && hasProcessedAudio_;
        oldParts = std::move(partitions_);
        partitions_ = std::move(nextParts);
        hasValidIr_ = true;
        fadeInPending_ = wasLive;
        fadeInRemaining_ = 0;
        fdlIndex_ = 0;
        std::fill(overlapL_.begin(), overlapL_.end(), 0.0f);
        std::fill(overlapR_.begin(), overlapR_.end(), 0.0f);
        for (auto& v : fdlL_) std::fill(v.begin(), v.end(), std::complex<float>{0.0f, 0.0f});
        for (auto& v : fdlR_) std::fill(v.begin(), v.end(), std::complex<float>{0.0f, 0.0f});
        if (!wasLive) {
            inQueueFill_ = 0;
            std::fill(inQueueL_.begin(), inQueueL_.end(), 0.0f);
            std::fill(inQueueR_.begin(), inQueueR_.end(), 0.0f);
            std::fill(dryDelayL_.begin(), dryDelayL_.end(), 0.0f);
            std::fill(dryDelayR_.begin(), dryDelayR_.end(), 0.0f);
            std::fill(outQueueL_.begin(), outQueueL_.end(), 0.0f);
            std::fill(outQueueR_.begin(), outQueueR_.end(), 0.0f);
        } else {
            // Preserve dryDelay & input framing queue across live IR swaps to avoid a 512-sample dry dropout;
            // zero only unconsumed old-IR wet samples in outQueue.
            const int fillStart = std::clamp(inQueueFill_, 0, kBlockSize);
            std::fill(outQueueL_.begin() + fillStart, outQueueL_.end(), 0.0f);
            std::fill(outQueueR_.begin() + fillStart, outQueueR_.end(), 0.0f);
        }
    }
}

void ConvolutionEngine::loadBuiltInPreset(int presetIndex, int sampleRate) {
    const int sr = std::max(16000, sampleRate);
    const float srScale = static_cast<float>(sr) / 48000.0f;
    // Synthesize distinct physical acoustic impulse responses (up to ~120ms, scaled with sample rate)
    const int irLen = std::min(kBlockSize * kMaxPartitions, std::max(kBlockSize, sr / 8));
    std::vector<float> irL(irLen, 0.0f);
    std::vector<float> irR(irLen, 0.0f);

    // Deterministic LCG noise generator for realistic early/late diffuse reflections
    uint32_t seed = 0x9E3779B9u + static_cast<uint32_t>(presetIndex * 7919);
    auto nextRand = [&seed]() -> float {
        seed = seed * 1664525u + 1013904223u;
        return (static_cast<float>(seed & 0xFFFFu) / 32768.0f) - 1.0f;
    };

    auto scaleTap = [srScale](int t48k) -> int {
        return std::max(1, static_cast<int>(std::lround(static_cast<float>(t48k) * srScale)));
    };

    // Direct Dirac impulse scaled per preset character
    switch (presetIndex) {
        case 0: { // Studio Vacuum Tube 6N1 (Warm low-mid triode body + gentle upper sheen)
            irL[0] = 0.78f; irR[0] = 0.78f;
            const int t1 = scaleTap(1);
            const int t2 = std::max(t1 + 1, scaleTap(2));
            if (irLen > t2 + 2) {
                irL[t1] = 0.14f; irL[t2] = 0.045f;
                irR[t1] = 0.14f; irR[t2] = 0.045f;
            }
            const int end5 = std::min(irLen, scaleTap(320));
            const int start5 = scaleTap(5);
            for (int i = start5; i < end5; ++i) {
                const float x = static_cast<float>(i) / srScale;
                float env = std::exp(-x / 42.0f);
                irL[i] = 0.014f * env * std::cos(x * 0.029f);
                irR[i] = 0.014f * env * std::cos(x * 0.029f);
            }
            break;
        }
        case 1: { // Cinema 3D Holographic Acoustic (Wide early stereo reflections + air-damped decay)
            irL[0] = 0.68f; irR[0] = 0.68f;
            int taps[] = {43, 97, 173, 281, 419, 641, 911};
            for (int idx = 0; idx < 7; ++idx) {
                int pL = scaleTap(taps[idx]);
                int pR = scaleTap(taps[idx] + 19 + (idx * 5));
                float g = 0.19f * std::pow(0.76f, static_cast<float>(idx));
                if (pL < irLen) irL[pL] += (idx % 2 == 0 ? g : -g);
                if (pR < irLen) irR[pR] += (idx % 2 == 1 ? g : -g);
            }
            const int diffStart = scaleTap(120);
            for (int i = diffStart; i < irLen; ++i) {
                float env = std::exp(-static_cast<float>(i) / (1600.0f * srScale));
                irL[i] += nextRand() * 0.038f * env;
                irR[i] += nextRand() * 0.038f * env;
            }
            const float lpAlpha = std::exp(-1.0f / (1.67f * srScale));
            const float lpIn = 1.0f - lpAlpha;
            float lpL = 0.0f, lpR = 0.0f;
            for (int i = 1; i < irLen; ++i) {
                lpL = lpIn * irL[i] + lpAlpha * lpL;
                lpR = lpIn * irR[i] + lpAlpha * lpR;
                irL[i] = lpL;
                irR[i] = lpR;
            }
            break;
        }
        case 2: { // Studio Sonic Exciter (Crisp phase-aligned high-frequency air emphasis)
            irL[0] = 0.96f; irR[0] = 0.96f;
            const int t2 = scaleTap(2);
            const int t4 = std::max(t2 + 1, scaleTap(4));
            const int t16 = scaleTap(16);
            const int t17 = std::max(t16 + 1, scaleTap(17));
            if (irLen > t17 + 1) {
                irL[t2] = -0.24f; irL[t4] = 0.07f; irL[t16] = 0.03f;
                irR[t2] = -0.24f; irR[t4] = 0.07f; irR[t17] = -0.025f;
            }
            break;
        }
        case 3: { // Analog Tape 15ips (Classic head bump & warm magnetic HF rolloff)
            irL[0] = 0.68f; irR[0] = 0.68f;
            const int t1 = scaleTap(1);
            const int t2 = std::max(t1 + 1, scaleTap(2));
            const int t3 = std::max(t2 + 1, scaleTap(3));
            if (irLen > t3 + 1) {
                irL[t1] = 0.21f; irL[t2] = 0.09f; irL[t3] = 0.03f;
                irR[t1] = 0.21f; irR[t2] = 0.09f; irR[t3] = 0.03f;
            }
            break;
        }
        case 4: { // Studio Reference Monitor (Near-field asymmetric L/R console & room cues)
            irL[0] = 0.78f; irR[0] = 0.78f;
            const int tapsL[] = {37, 89, 157, 241};
            const int tapsR[] = {54, 113, 181, 271};
            for (int idx = 0; idx < 4; ++idx) {
                float g = 0.18f * std::pow(0.72f, static_cast<float>(idx));
                int pL = scaleTap(tapsL[idx]);
                int pR = scaleTap(tapsR[idx]);
                if (pL < irLen) irL[pL] += g;
                if (pR < irLen) irR[pR] += g * 0.94f;
            }
            break;
        }
        default: { // Gold-Plate Studio Reverb (Shimmering bright EMT-style stereo plate tail)
            irL[0] = 0.68f; irR[0] = 0.68f;
            float lpL = 0.0f, lpR = 0.0f;
            const int plateStart = scaleTap(24);
            const float pAlpha = std::exp(-1.0f / (0.785f * srScale));
            const float pIn = 1.0f - pAlpha;
            for (int i = plateStart; i < irLen; ++i) {
                float env = std::exp(-static_cast<float>(i) / (2500.0f * srScale));
                lpL = pIn * (nextRand() * 0.044f * env) + pAlpha * lpL;
                lpR = pIn * (nextRand() * 0.044f * env) + pAlpha * lpR;
                irL[i] += lpL;
                irR[i] += lpR;
            }
            break;
        }
    }

    loadImpulseResponse(irL.data(), irR.data(), irLen);
}

void ConvolutionEngine::processBlock512(
    const float* blkInL, const float* blkInR,
    float* blkOutL, float* blkOutR,
    const PartitionSet& parts
) {
    const int nParts = parts.numPartitions;
    if (nParts <= 0) {
        std::memcpy(blkOutL, blkInL, kBlockSize * sizeof(float));
        std::memcpy(blkOutR, blkInR, kBlockSize * sizeof(float));
        return;
    }

    // Store current block FFT into circular Frequency-Domain Delay Line
    std::fill(fftBuf_.begin(), fftBuf_.end(), std::complex<float>{0.0f, 0.0f});
    for (int i = 0; i < kBlockSize; ++i) fftBuf_[i] = {blkInL[i], 0.0f};
    fft(fftBuf_, false);
    fdlL_[fdlIndex_] = fftBuf_;

    std::fill(fftBuf_.begin(), fftBuf_.end(), std::complex<float>{0.0f, 0.0f});
    for (int i = 0; i < kBlockSize; ++i) fftBuf_[i] = {blkInR[i], 0.0f};
    fft(fftBuf_, false);
    fdlR_[fdlIndex_] = fftBuf_;

    std::fill(accumSpecL_.begin(), accumSpecL_.end(), std::complex<float>{0.0f, 0.0f});
    std::fill(accumSpecR_.begin(), accumSpecR_.end(), std::complex<float>{0.0f, 0.0f});

    for (int p = 0; p < nParts; ++p) {
        int slot = fdlIndex_ - p;
        if (slot < 0) slot += kMaxPartitions;
        const auto& specInL = fdlL_[slot];
        const auto& specInR = fdlR_[slot];
        const auto& irL = parts.irSpecL[p];
        const auto& irR = parts.irSpecR[p];
        for (int k = 0; k < kFftSize; ++k) {
            accumSpecL_[k] += specInL[k] * irL[k];
            accumSpecR_[k] += specInR[k] * irR[k];
        }
    }

    fft(accumSpecL_, true);
    for (int i = 0; i < kBlockSize; ++i) {
        blkOutL[i] = accumSpecL_[i].real() + overlapL_[i];
        overlapL_[i] = accumSpecL_[i + kBlockSize].real();
    }

    fft(accumSpecR_, true);
    for (int i = 0; i < kBlockSize; ++i) {
        blkOutR[i] = accumSpecR_[i].real() + overlapR_[i];
        overlapR_[i] = accumSpecR_[i + kBlockSize].real();
    }

    fdlIndex_ = (fdlIndex_ + 1) % kMaxPartitions;
}

void ConvolutionEngine::process(
    const float* inL, const float* inR,
    float* outL, float* outR,
    int numFrames, float wetMix
) {
    if (!inL || !inR || !outL || !outR || numFrames <= 0) return;

    if (!irMutex_.try_lock()) {
        std::memcpy(outL, inL, static_cast<size_t>(numFrames) * sizeof(float));
        std::memcpy(outR, inR, static_cast<size_t>(numFrames) * sizeof(float));
        return;
    }
    std::unique_lock<std::mutex> lock(irMutex_, std::adopt_lock);

    if (!hasValidIr_) {
        std::memcpy(outL, inL, static_cast<size_t>(numFrames) * sizeof(float));
        std::memcpy(outR, inR, static_cast<size_t>(numFrames) * sizeof(float));
        return;
    }

    const float clampedWet = std::clamp(std::isfinite(wetMix) ? wetMix : 0.0f, 0.0f, 1.0f);
    const float dry = 1.0f - clampedWet;
    hasProcessedAudio_ = true;
    for (int i = 0; i < numFrames; ++i) {
        const float cleanInL = std::isfinite(inL[i]) ? inL[i] : 0.0f;
        const float cleanInR = std::isfinite(inR[i]) ? inR[i] : 0.0f;
        const float dryDelayedL = dryDelayL_[inQueueFill_];
        const float dryDelayedR = dryDelayR_[inQueueFill_];
        dryDelayL_[inQueueFill_] = cleanInL;
        dryDelayR_[inQueueFill_] = cleanInR;
        inQueueL_[inQueueFill_] = cleanInL;
        inQueueR_[inQueueFill_] = cleanInR;
        const float wetL = outQueueL_[inQueueFill_];
        const float wetR = outQueueR_[inQueueFill_];
        float wetEnv = 1.0f;
        if (fadeInRemaining_ > 0) {
            wetEnv = static_cast<float>(129 - fadeInRemaining_) / 128.0f;
            --fadeInRemaining_;
        }

        outL[i] = dryDelayedL * dry + wetL * (clampedWet * wetEnv);
        outR[i] = dryDelayedR * dry + wetR * (clampedWet * wetEnv);

        inQueueFill_++;
        if (inQueueFill_ >= kBlockSize) {
            processBlock512(
                inQueueL_.data(), inQueueR_.data(),
                outQueueL_.data(), outQueueR_.data(),
                partitions_
            );
            if (fadeInPending_) {
                fadeInRemaining_ = 128;
                fadeInPending_ = false;
            }
            inQueueFill_ = 0;
        }
    }
}
