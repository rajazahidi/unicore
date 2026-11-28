#pragma once

#include <algorithm>
#include <array>
#include <cmath>

namespace rfx {

constexpr float kPi = 3.14159265358979323846f;

constexpr float kGraphicEqFreqs[10] = {
    31.25f, 62.5f, 125.0f, 250.0f, 500.0f,
    1000.0f, 2000.0f, 4000.0f, 8000.0f, 16000.0f
};

inline float flushDenormal(float v) {
    return (!std::isfinite(v) || std::abs(v) < 1e-24f) ? 0.0f : v;
}

/**
 * Click-free per-sample exponential gain smoother (snaps cleanly on cold start/reset).
 */
struct GainSmoother {
    float current = 1.0f;
    bool initialized = false;

    void reset(float initial = 1.0f) {
        current = initial;
        initialized = false;
    }

    static float stepCoeff(float sr, float tauSec = 0.015f) {
        return 1.0f - std::exp(-1.0f / (std::max(0.001f, tauSec) * std::max(8000.0f, sr)));
    }

    inline float next(float target, float alpha) {
        if (!initialized) {
            current = target;
            initialized = true;
            return current;
        }
        current += alpha * (target - current);
        if (std::abs(target - current) < 1e-5f) current = target;
        return current;
    }
};

/**
 * Stereo biquad supporting both static DF2T and modulation-clean DF1 (click-free EQ transitions).
 */
struct StereoBiquad {
    float b0 = 1.0f, b1 = 0.0f, b2 = 0.0f;
    float a1 = 0.0f, a2 = 0.0f;
    // Direct Form II Transposed state (for static/slow filters)
    float z1L = 0.0f, z2L = 0.0f;
    float z1R = 0.0f, z2R = 0.0f;
    // Direct Form I state (modulation-safe audio history for live EQ bands)
    float x1L = 0.0f, x2L = 0.0f, y1L = 0.0f, y2L = 0.0f;
    float x1R = 0.0f, x2R = 0.0f, y1R = 0.0f, y2R = 0.0f;

    void resetState() {
        z1L = z2L = z1R = z2R = 0.0f;
        x1L = x2L = y1L = y2L = 0.0f;
        x1R = x2R = y1R = y2R = 0.0f;
    }

    void reset() {
        b0 = 1.0f; b1 = 0.0f; b2 = 0.0f;
        a1 = 0.0f; a2 = 0.0f;
        resetState();
    }

    void setPeaking(float freq, float sampleRate, float gainDb, float q = 1.41f) {
        if (!std::isfinite(freq) || !std::isfinite(sampleRate) || !std::isfinite(gainDb) || !std::isfinite(q) ||
            sampleRate < 8000.0f || std::abs(gainDb) < 0.02f) {
            b0 = 1.0f; b1 = 0.0f; b2 = 0.0f;
            a1 = 0.0f; a2 = 0.0f;
            return;
        }
        const float safeGain = std::clamp(gainDb, -24.0f, 24.0f);
        const float safeQ = std::clamp(q, 0.1f, 24.0f);
        const float safeFreq = std::clamp(freq, 10.0f, sampleRate * 0.45f);
        const float A = std::pow(10.0f, safeGain / 40.0f);
        const float w0 = 2.0f * kPi * safeFreq / sampleRate;
        const float sinW0 = std::sin(w0);
        const float cosW0 = std::cos(w0);
        const float alpha = sinW0 / (2.0f * safeQ);

        const float a0 = 1.0f + alpha / A;
        b0 = (1.0f + alpha * A) / a0;
        b1 = (-2.0f * cosW0) / a0;
        b2 = (1.0f - alpha * A) / a0;
        a1 = (-2.0f * cosW0) / a0;
        a2 = (1.0f - alpha / A) / a0;
    }

    void setLowShelf(float freq, float sampleRate, float gainDb, float slope = 0.85f) {
        if (!std::isfinite(freq) || !std::isfinite(sampleRate) || !std::isfinite(gainDb) || !std::isfinite(slope) ||
            sampleRate < 8000.0f || std::abs(gainDb) < 0.02f) {
            b0 = 1.0f; b1 = 0.0f; b2 = 0.0f;
            a1 = 0.0f; a2 = 0.0f;
            return;
        }
        const float safeGain = std::clamp(gainDb, -24.0f, 24.0f);
        const float safeSlope = std::clamp(slope, 0.1f, 2.0f);
        const float safeFreq = std::clamp(freq, 10.0f, sampleRate * 0.45f);
        const float A = std::pow(10.0f, safeGain / 40.0f);
        const float w0 = 2.0f * kPi * safeFreq / sampleRate;
        const float sinW0 = std::sin(w0);
        const float cosW0 = std::cos(w0);
        const float alpha = (sinW0 / 2.0f) * std::sqrt(std::max(0.01f, (A + 1.0f / A) * (1.0f / safeSlope - 1.0f) + 2.0f));
        const float twoSqrtAAlpha = 2.0f * std::sqrt(A) * alpha;

        const float a0 = (A + 1.0f) + (A - 1.0f) * cosW0 + twoSqrtAAlpha;
        b0 = (A * ((A + 1.0f) - (A - 1.0f) * cosW0 + twoSqrtAAlpha)) / a0;
        b1 = (2.0f * A * ((A - 1.0f) - (A + 1.0f) * cosW0)) / a0;
        b2 = (A * ((A + 1.0f) - (A - 1.0f) * cosW0 - twoSqrtAAlpha)) / a0;
        a1 = (-2.0f * ((A - 1.0f) + (A + 1.0f) * cosW0)) / a0;
        a2 = ((A + 1.0f) + (A - 1.0f) * cosW0 - twoSqrtAAlpha) / a0;
    }

    void setHighShelf(float freq, float sampleRate, float gainDb, float slope = 0.85f) {
        if (!std::isfinite(freq) || !std::isfinite(sampleRate) || !std::isfinite(gainDb) || !std::isfinite(slope) ||
            sampleRate < 8000.0f || std::abs(gainDb) < 0.02f) {
            b0 = 1.0f; b1 = 0.0f; b2 = 0.0f;
            a1 = 0.0f; a2 = 0.0f;
            return;
        }
        const float safeGain = std::clamp(gainDb, -24.0f, 24.0f);
        const float safeSlope = std::clamp(slope, 0.1f, 2.0f);
        const float safeFreq = std::clamp(freq, 10.0f, sampleRate * 0.45f);
        const float A = std::pow(10.0f, safeGain / 40.0f);
        const float w0 = 2.0f * kPi * safeFreq / sampleRate;
        const float sinW0 = std::sin(w0);
        const float cosW0 = std::cos(w0);
        const float alpha = (sinW0 / 2.0f) * std::sqrt(std::max(0.01f, (A + 1.0f / A) * (1.0f / safeSlope - 1.0f) + 2.0f));
        const float twoSqrtAAlpha = 2.0f * std::sqrt(A) * alpha;

        const float a0 = (A + 1.0f) - (A - 1.0f) * cosW0 + twoSqrtAAlpha;
        b0 = (A * ((A + 1.0f) + (A - 1.0f) * cosW0 + twoSqrtAAlpha)) / a0;
        b1 = (-2.0f * A * ((A - 1.0f) + (A + 1.0f) * cosW0)) / a0;
        b2 = (A * ((A + 1.0f) + (A - 1.0f) * cosW0 - twoSqrtAAlpha)) / a0;
        a1 = (2.0f * ((A - 1.0f) - (A + 1.0f) * cosW0)) / a0;
        a2 = ((A + 1.0f) - (A - 1.0f) * cosW0 - twoSqrtAAlpha) / a0;
    }

    void setLowPass(float freq, float sampleRate, float q = 0.70710678f) {
        if (!std::isfinite(freq) || !std::isfinite(sampleRate) || !std::isfinite(q) || sampleRate < 8000.0f) {
            b0 = 1.0f; b1 = 0.0f; b2 = 0.0f;
            a1 = 0.0f; a2 = 0.0f;
            return;
        }
        const float safeQ = std::clamp(q, 0.1f, 24.0f);
        const float safeFreq = std::clamp(freq, 10.0f, sampleRate * 0.48f);
        const float w0 = 2.0f * kPi * safeFreq / sampleRate;
        const float cosW0 = std::cos(w0);
        const float alpha = std::sin(w0) / (2.0f * safeQ);
        const float a0 = 1.0f + alpha;
        b0 = ((1.0f - cosW0) * 0.5f) / a0;
        b1 = (1.0f - cosW0) / a0;
        b2 = b0;
        a1 = (-2.0f * cosW0) / a0;
        a2 = (1.0f - alpha) / a0;
    }

    void configureGraphicBand(int bandIdx, float sampleRate, float filterGainDb) {
        if (bandIdx == 0) {
            setLowShelf(44.0f, sampleRate, filterGainDb, 0.72f);
        } else if (bandIdx == 9) {
            setHighShelf(11500.0f, sampleRate, filterGainDb, 0.72f);
        } else {
            setPeaking(kGraphicEqFreqs[bandIdx], sampleRate, filterGainDb, 1.00f);
        }
    }

    double evalMagnitudeDb(float freqHz, float sampleRate) const {
        const double w = 2.0 * 3.14159265358979323846 * static_cast<double>(freqHz) / static_cast<double>(sampleRate);
        const double c1 = std::cos(w), s1 = -std::sin(w);
        const double c2 = std::cos(2.0 * w), s2 = -std::sin(2.0 * w);
        const double numR = b0 + b1 * c1 + b2 * c2;
        const double numI = b1 * s1 + b2 * s2;
        const double denR = 1.0 + a1 * c1 + a2 * c2;
        const double denI = a1 * s1 + a2 * s2;
        const double numSq = numR * numR + numI * numI;
        const double denSq = std::max(1e-24, denR * denR + denI * denI);
        return 10.0 * std::log10(std::max(1e-24, numSq / denSq));
    }

    inline void processStereo(float& l, float& r) {
        float outL = b0 * l + z1L;
        z1L = flushDenormal(b1 * l - a1 * outL + z2L);
        z2L = flushDenormal(b2 * l - a2 * outL);
        l = std::isfinite(outL) ? outL : 0.0f;

        float outR = b0 * r + z1R;
        z1R = flushDenormal(b1 * r - a1 * outR + z2R);
        z2R = flushDenormal(b2 * r - a2 * outR);
        r = std::isfinite(outR) ? outR : 0.0f;
    }

    inline void processStereoDf1(float& l, float& r) {
        const float inL = std::isfinite(l) ? l : 0.0f;
        float outL = b0 * inL + b1 * x1L + b2 * x2L - a1 * y1L - a2 * y2L;
        if (!std::isfinite(outL)) outL = inL;
        x2L = x1L;
        x1L = inL;
        y2L = y1L;
        y1L = flushDenormal(outL);
        l = outL;

        const float inR = std::isfinite(r) ? r : 0.0f;
        float outR = b0 * inR + b1 * x1R + b2 * x2R - a1 * y1R - a2 * y2R;
        if (!std::isfinite(outR)) outR = inR;
        x2R = x1R;
        x1R = inR;
        y2R = y1R;
        y1R = flushDenormal(outR);
        r = outR;
    }
};

/**
 * Coupled 10-band octave graphic EQ solver + DF1 exponential slew smoother.
 * Eliminates adjacent-band summing overshoot (+6.4 dB ripple), 11.3 kHz Nyquist cramping dip,
 * and DF2T step clicks when adjusting faders live one by one.
 */
class GraphicEqBank {
public:
    std::array<StereoBiquad, 10> filters{};

    void reset() {
        for (auto& f : filters) f.reset();
        curGains_.fill(0.0f);
        targetGains_.fill(0.0f);
        rampCounter_ = 0;
        rampReady_ = false;
    }

    void setTargets(float sampleRate, bool eqActive, const float targetDb[10]) {
        const float safeSr = std::clamp(sampleRate, 8000.0f, 384000.0f);
        const bool srChanged = std::abs(safeSr - lastSr_) > 0.5f;
        lastSr_ = safeSr;
        float cleanTargets[10] = {0};
        bool anyNonZero = false;
        if (eqActive) {
            for (int i = 0; i < 10; ++i) {
                const float g = std::isfinite(targetDb[i]) ? std::clamp(targetDb[i], -24.0f, 24.0f) : 0.0f;
                cleanTargets[i] = g;
                if (std::abs(g) >= 0.02f) anyNonZero = true;
            }
        }
        if (!anyNonZero) {
            targetGains_.fill(0.0f);
        } else {
            solveCoupledGains(safeSr, cleanTargets, targetGains_.data());
        }

        if (!rampReady_ || srChanged) {
            curGains_ = targetGains_;
            for (int i = 0; i < 10; ++i) {
                if (srChanged) filters[i].resetState();
                filters[i].configureGraphicBand(i, safeSr, curGains_[i]);
            }
        }
    }

    inline void processStereo(float& l, float& r) {
        if (!rampReady_) rampReady_ = true;
        if ((rampCounter_ & 7) == 0) {
            bool needCoeffStep = false;
            for (int i = 0; i < 10; ++i) {
                if (std::abs(targetGains_[i] - curGains_[i]) > 0.005f) {
                    needCoeffStep = true;
                    break;
                }
            }
            if (needCoeffStep) {
                const float alpha = 1.0f - std::exp(-8.0f / (0.018f * lastSr_));
                for (int i = 0; i < 10; ++i) {
                    const float diff = targetGains_[i] - curGains_[i];
                    if (std::abs(diff) <= 0.015f) {
                        if (curGains_[i] != targetGains_[i]) {
                            curGains_[i] = targetGains_[i];
                            filters[i].configureGraphicBand(i, lastSr_, curGains_[i]);
                        }
                    } else {
                        curGains_[i] += alpha * diff;
                        filters[i].configureGraphicBand(i, lastSr_, curGains_[i]);
                    }
                }
            }
        }
        ++rampCounter_;
        for (int b = 0; b < 10; ++b) {
            if (std::abs(curGains_[b]) >= 0.015f || std::abs(targetGains_[b]) >= 0.015f) {
                filters[b].processStereoDf1(l, r);
            } else {
                // Keep DF1 input/output delay history continuously primed for zero-pop re-engagement
                filters[b].x2L = filters[b].x1L; filters[b].x1L = l;
                filters[b].y2L = filters[b].y1L; filters[b].y1L = l;
                filters[b].x2R = filters[b].x1R; filters[b].x1R = r;
                filters[b].y2R = filters[b].y1R; filters[b].y1R = r;
            }
        }
    }

    inline bool isBandActive(int b) const {
        return b >= 0 && b < 10 &&
               (std::abs(curGains_[b]) >= 0.015f || std::abs(targetGains_[b]) >= 0.015f);
    }

private:
    float lastSr_ = 48000.0f;
    float matrixSr_ = 0.0f;
    double invMatrix_[10][10]{};
    bool matrixValid_ = false;
    std::array<float, 10> curGains_{};
    std::array<float, 10> targetGains_{};
    int rampCounter_ = 0;
    bool rampReady_ = false;

    void ensureInverseMatrix(float sr) {
        if (matrixValid_ && std::abs(matrixSr_ - sr) < 1.0f) return;
        matrixSr_ = sr;
        if (sr < 24000.0f) {
            matrixValid_ = false;
            return;
        }
        double aug[10][20]{};
        constexpr float kRefGain = 6.0f;
        for (int j = 0; j < 10; ++j) {
            StereoBiquad probe;
            probe.configureGraphicBand(j, sr, kRefGain);
            for (int i = 0; i < 10; ++i) {
                aug[i][j] = probe.evalMagnitudeDb(kGraphicEqFreqs[i], sr) / kRefGain;
            }
            aug[j][10 + j] = 1.0;
        }
        for (int col = 0; col < 10; ++col) {
            int piv = col;
            double best = std::abs(aug[col][col]);
            for (int r = col + 1; r < 10; ++r) {
                double v = std::abs(aug[r][col]);
                if (v > best) { best = v; piv = r; }
            }
            if (best < 1e-6) {
                matrixValid_ = false;
                return;
            }
            if (piv != col) {
                for (int c = 0; c < 20; ++c) std::swap(aug[col][c], aug[piv][c]);
            }
            const double diag = aug[col][col];
            for (int c = 0; c < 20; ++c) aug[col][c] /= diag;
            for (int r = 0; r < 10; ++r) {
                if (r == col) continue;
                const double f = aug[r][col];
                for (int c = 0; c < 20; ++c) aug[r][c] -= f * aug[col][c];
            }
        }
        for (int r = 0; r < 10; ++r) {
            for (int c = 0; c < 10; ++c) {
                invMatrix_[r][c] = aug[r][10 + c];
            }
        }
        matrixValid_ = true;
    }

    void solveCoupledGains(float sr, const float targetDb[10], float outGainsDb[10]) {
        ensureInverseMatrix(sr);
        if (!matrixValid_) {
            for (int i = 0; i < 10; ++i) outGainsDb[i] = std::clamp(targetDb[i], -18.0f, 18.0f);
            return;
        }
        // Pass 1: linear decoupled estimate M^-1 * target
        double fg[10]{};
        for (int i = 0; i < 10; ++i) {
            double s = 0.0;
            for (int j = 0; j < 10; ++j) s += invMatrix_[i][j] * static_cast<double>(targetDb[j]);
            fg[i] = std::clamp(s, -18.0, 18.0);
        }
        // Pass 2: one Gauss-Newton residual refinement eliminates high-gain RBJ Q compression error
        StereoBiquad trial[10];
        for (int j = 0; j < 10; ++j) {
            trial[j].configureGraphicBand(j, sr, static_cast<float>(fg[j]));
        }
        double res[10]{};
        for (int i = 0; i < 10; ++i) {
            double actual = 0.0;
            for (int j = 0; j < 10; ++j) actual += trial[j].evalMagnitudeDb(kGraphicEqFreqs[i], sr);
            res[i] = static_cast<double>(targetDb[i]) - actual;
        }
        for (int i = 0; i < 10; ++i) {
            double corr = 0.0;
            for (int j = 0; j < 10; ++j) corr += invMatrix_[i][j] * res[j];
            outGainsDb[i] = std::clamp(static_cast<float>(fg[i] + corr), -18.0f, 18.0f);
        }
    }
};

struct LimiterCoeffs {
    float limitLin = 0.944f;
    float attCoeff = 0.08f;
    float attIn = 0.92f;
    float relCoeff = 0.999f;
    float relIn = 0.001f;
    int holdSamples = 1152;
    float knee = 0.92f;
    float headroom = 0.024f;
};

/**
 * Peak-Hold Smooth Stereo Limiter: instant catch guarantees ceiling compliance while
 * 24ms peak hold + 200ms smooth release eliminate 80-160Hz bass AM buzz in vocals.
 */
struct PeakHoldLimiter {
    float env = 0.0f;
    int holdCounter = 0;

    void reset() {
        env = 0.0f;
        holdCounter = 0;
    }

    static LimiterCoeffs prepare(float sr, float limitThresholdDb) {
        const float safeSr = std::max(8000.0f, sr);
        LimiterCoeffs c;
        c.limitLin = std::pow(10.0f, std::clamp(limitThresholdDb, -12.0f, 0.0f) / 20.0f);
        c.attCoeff = std::exp(-1.0f / (0.0000083f * safeSr));
        c.attIn = 1.0f - c.attCoeff;
        c.relCoeff = std::exp(-1.0f / (0.200f * safeSr));
        c.relIn = 1.0f - c.relCoeff;
        c.holdSamples = std::clamp(static_cast<int>(std::lround(0.024f * safeSr)), 64, 16384);
        c.knee = c.limitLin * 0.975f;
        c.headroom = std::max(0.01f, c.limitLin - c.knee);
        return c;
    }

    inline void processStereo(float& l, float& r, const LimiterCoeffs& c) {
        if (!std::isfinite(env)) env = 0.0f;
        const float peak = std::max(std::abs(l), std::abs(r));
        if (peak > env) {
            env = c.attCoeff * env + c.attIn * peak;
            holdCounter = c.holdSamples;
        } else if (holdCounter > 0) {
            --holdCounter;
        } else {
            env = flushDenormal(c.relCoeff * env + c.relIn * peak);
        }
        if (env > c.limitLin) {
            const float gain = c.limitLin / env;
            l *= gain;
            r *= gain;
        }
        auto softKnee = [&c](float x) {
            if (!std::isfinite(x)) return 0.0f;
            const float ax = std::abs(x);
            if (ax <= c.knee) return x;
            const float shaped = c.knee + c.headroom * std::tanh((ax - c.knee) / c.headroom);
            return (x < 0.0f) ? -shaped : shaped;
        };
        l = std::clamp(softKnee(l), -0.998f, 0.998f);
        r = std::clamp(softKnee(r), -0.998f, 0.998f);
    }
};

struct MonoBiquadState {
    float x1 = 0.0f, x2 = 0.0f, y1 = 0.0f, y2 = 0.0f;

    void reset() {
        x1 = x2 = y1 = y2 = 0.0f;
    }

    inline void prime(float x) {
        x2 = x1; x1 = x;
        y2 = y1; y1 = x;
    }

    inline float processDf1(const StereoBiquad& f, float x) {
        const float y = f.b0 * x + f.b1 * x1 + f.b2 * x2 - f.a1 * y1 - f.a2 * y2;
        x2 = x1; x1 = x;
        y2 = y1; y1 = flushDenormal(y);
        return y1;
    }
};

inline float computeHeadroomAutoTrimDb(
    bool masterEnabled,
    bool eqEnabled,
    const float eqBandsDb[10],
    bool clarityEnabled,
    float clarityGainDb,
    bool pureBassEnabled,
    float pureBassGainDb,
    bool tubeEnabled,
    float tubeWarmth,
    float preampDb
) {
    if (!masterEnabled) return 0.0f;
    float peakBoostDb = 0.0f;
    if (eqEnabled && eqBandsDb) {
        for (int b = 0; b < 10; ++b) {
            peakBoostDb = std::max(peakBoostDb, eqBandsDb[b]);
        }
    }
    if (pureBassEnabled) {
        peakBoostDb = std::max(peakBoostDb, std::clamp(pureBassGainDb * 0.25f, 0.0f, 5.0f));
    }
    if (clarityEnabled) {
        peakBoostDb = std::max(peakBoostDb, std::clamp(clarityGainDb * 0.45f, 0.0f, 7.5f));
    }
    const float tubePostDb = tubeEnabled ? (2.2f * std::clamp(tubeWarmth, 0.0f, 1.0f)) : 0.0f;
    peakBoostDb += tubePostDb + std::max(0.0f, preampDb);
    return (peakBoostDb > 3.2f) ? -std::clamp((peakBoostDb - 3.2f) * 0.48f, 0.0f, 6.0f) : 0.0f;
}

} // namespace rfx
