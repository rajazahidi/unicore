#pragma once

#include <array>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include "ConvolutionEngine.h"
#include "ResonanceBiquadEq.h"
#include "ResonanceEffectsDsp.h"

#ifdef RESONANCE_WIN_EXPORTS
#define RFX_API __declspec(dllexport)
#else
#define RFX_API
#endif

struct WinDspParams {
    int32_t masterEnabled = 1;
    int32_t eqEnabled = 1;
    float eqBandsDb[10] = {0};

    int32_t convolverEnabled = 0;
    int32_t convolverPreset = 0;

    int32_t surroundEnabled = 1;
    float surroundLevel = 0.35f;

    int32_t binauralEnabled = 1;
    int32_t binauralMode = 0; // 0: Natural Stage, 1: Studio Pan, 2: 8D Orbit
    float crossfeedLevel = 0.30f;
    float panBalance = 0.0f;
    float orbitSpeed = 0.35f;

    int32_t reverbEnabled = 0;
    int32_t reverbPreset = 0; // 0..6

    int32_t pureBassEnabled = 1;
    int32_t pureBassMode = 1; // 0: Smooth Lows, 1: Deep Impact, 2: Subwoofer
    float pureBassGainDb = 4.2f;

    int32_t clarityEnabled = 1;
    int32_t clarityMode = 0;  // 0: Studio Natural, 1: Air Dynamic, 2: Ultra Hi-Fi
    float clarityGainDb = 2.5f;

    int32_t tubeEnabled = 0;
    float tubeWarmth = 0.40f;

    float limiterThresholdDb = -0.5f;
    float preampDb = 0.0f;

    int32_t loudnessEnabled = 0;
    float loudnessGainDb = 2.0f;

    int32_t chorusEnabled = 0;
    int32_t chorusMode = 0;
    float chorusRateHz = 0.85f;
    float chorusDepth = 0.60f;
    float chorusMix = 0.50f;

    int32_t delayEnabled = 0;
    int32_t delayMode = 2;
    int32_t delayTimeMs = 260;
    float delayFeedback = 0.42f;
    float delayMix = 0.40f;
};

using WinBiquadFilter = rfx::StereoBiquad;

/**
 * Pure C++17 Resonance FX DSP Pipeline shared 1-to-1 with Android Hi-Res Engine:
 * 10-Band EQ -> Split-Band Bass (<300Hz limiter) + Anti-Masking Vocal Lift ->
 * Silky Clarity -> UPOLA Convolver -> Tube -> Binaural/Pan/8D Orbit ->
 * Zero-Comb 3D Surround -> Chorus -> Delay -> Schroeder Stereo Reverb -> Soft Limiter.
 */
class WinDspCore {
public:
    static constexpr int kMaxExtraTonalCh = 6;

    WinDspCore();

    void setSampleRate(int sampleRate);
    int sampleRate() const { return sampleRate_.load(std::memory_order_acquire); }

    void updateParameters(const WinDspParams& params, bool allowBlockingConvolverLoad = true);
    void updateParametersRt(const WinDspParams& params);
    void setRuntimeGainOffsetDb(float offsetDb);
    void resetPlaybackTails();
    void loadCustomImpulseResponse(const float* irLeft, const float* irRight, int length, int irSampleRate = 0);
    void loadBuiltInConvolverPreset(int presetIndex);

    void processBlock(float* chL, float* chR, int numFrames);
    void processExtraChannelBlock(int extraSlot, float* ch, int numFrames, bool isLfe = false);
    void getSpectrumLevels(float* out28);

private:
    static WinDspParams sanitizeDspParams(const WinDspParams& params);
    void rebuildCoefficients();
    void resetFxBuffers();
    void initChorusAndDelayBuffers(int sampleRate);

    std::atomic<int> sampleRate_{48000};
    std::mutex rtBufferMutex_;
    std::mutex paramWriteMutex_;
    WinDspParams stagedParams_;
    std::atomic<uint32_t> paramSeq_{0};
    uint32_t rtAppliedSeq_ = 0;
    WinDspParams rtParams_;
    bool coeffsDirty_ = true;
    bool prevMasterEnabled_ = true;
    bool prevPureBassEnabled_ = true;
    bool prevClarityEnabled_ = true;
    bool prevConvolverEnabled_ = false;
    bool prevChorusActive_ = false;
    bool prevDelayActive_ = false;
    bool prevLoudnessEnabled_ = false;
    std::atomic<float> runtimeGainOffsetDb_{0.0f};

    rfx::GraphicEqBank eqBank_;
    WinBiquadFilter bassShelf_;
    WinBiquadFilter bassCharacterFilter_;
    WinBiquadFilter clarityShelf_;
    WinBiquadFilter clarityVoicingFilter_;
    WinBiquadFilter antiMaskFilter_;
    WinBiquadFilter tubeBodyShelf_;
    WinBiquadFilter loudShelfLow_;
    WinBiquadFilter loudShelfHigh_;
    rfx::SubBassProtectorState subProtector_;
    float exciterInLpL_ = 0.0f;
    float exciterInLpR_ = 0.0f;
    float exciterDcLpL_ = 0.0f;
    float exciterDcLpR_ = 0.0f;
    rfx::TubeStageState tubeStage_;

    ConvolutionEngine convolver_;
    rfx::ConvolverToggleSmoother convToggleSmoother_;
    std::atomic<int> lastBuiltInConvolverIdx_{-1};
    std::atomic<bool> usingCustomIr_{false};
    std::mutex irStoreMutex_;
    std::vector<float> customIrRawL_;
    std::vector<float> customIrRawR_;
    int customIrRawSr_ = 48000;
    void applyStoredCustomIrForSampleRate(int targetSr);

    static constexpr int kMaxDelaySamples = 8192;
    static constexpr int kBinauralDelayLen = 512;
    std::vector<float> haasDelayR_;
    int haasIdx_ = 0;

    std::vector<float> crossDelayL_;
    std::vector<float> crossDelayR_;
    std::vector<float> panItdL_;
    std::vector<float> panItdR_;
    int crossIdx_ = 0;
    int panItdIdx_ = 0;
    float crossLpL_ = 0.0f;
    float crossLpR_ = 0.0f;
    float shadowLpL_ = 0.0f;
    float shadowLpR_ = 0.0f;
    float orbitPhase_ = 0.85f;

    rfx::SchroederReverbState reverb_;

    rfx::StereoChorusState chorus_;
    rfx::StereoDelayState delay_;

    rfx::PeakHoldLimiter limiter_;
    rfx::GainSmoother gainSmoother_;

    std::vector<float> tmpConvL_;
    std::vector<float> tmpConvR_;

    // Live 28-band spectral metering (atomic lock-free UI read)
    std::array<std::atomic<float>, 28> spectrumBins_{};
    std::array<float, 2048> meterRing_{};
    int meterWritePos_ = 0;
    int meterSampleCounter_ = 0;

    struct ExtraTonalState {
        std::array<rfx::MonoBiquadState, 10> eq{};
        rfx::MonoBiquadState bassShelf{};
        rfx::MonoBiquadState clarityVoicing{};
        rfx::MonoBiquadState clarityShelf{};
        rfx::MonoBiquadState loudLow{};
        rfx::MonoBiquadState loudHigh{};
        void reset() {
            for (auto& b : eq) b.reset();
            bassShelf.reset();
            clarityVoicing.reset();
            clarityShelf.reset();
            loudLow.reset();
            loudHigh.reset();
        }
    };
    std::array<ExtraTonalState, kMaxExtraTonalCh> extraTonalStates_{};
};

#ifdef __cplusplus
extern "C" {
#endif

RFX_API void* rfx_win_create();
RFX_API void rfx_win_destroy(void* handle);
RFX_API int32_t rfx_win_list_devices_json(void* handle, char* outBuf, int32_t outBufSize);
RFX_API void rfx_win_update_params(void* handle, const WinDspParams* params);
RFX_API void rfx_win_set_runtime_gain_offset_db(void* handle, float offsetDb);
RFX_API int32_t rfx_win_load_custom_ir(void* handle, const float* irL, const float* irR, int32_t length);
RFX_API int32_t rfx_win_load_custom_ir_ex(void* handle, const float* irL, const float* irR, int32_t length, int32_t irSampleRate);
RFX_API void rfx_win_load_builtin_convolver(void* handle, int32_t presetIdx);

RFX_API int32_t rfx_win_play_file(void* handle, const wchar_t* filePath, const wchar_t* renderDeviceId);
RFX_API int32_t rfx_win_play_file_ex(void* handle, const wchar_t* filePath, const wchar_t* renderDeviceId, int64_t initialSeekMs, int32_t startPaused);
RFX_API int32_t rfx_win_start_loopback(void* handle, const wchar_t* sourceDeviceId, int32_t isLoopbackRenderEndpoint, const wchar_t* renderDeviceId);
RFX_API void rfx_win_stop(void* handle);
RFX_API void rfx_win_set_paused(void* handle, int32_t paused);
RFX_API void rfx_win_seek_ms(void* handle, int64_t posMs);

RFX_API int32_t rfx_win_is_playing(void* handle);
RFX_API int32_t rfx_win_is_paused(void* handle);
RFX_API int32_t rfx_win_is_loopback_active(void* handle);
RFX_API int32_t rfx_win_get_last_error(void* handle);
RFX_API int32_t rfx_win_get_sample_rate(void* handle);
RFX_API int64_t rfx_win_get_position_ms(void* handle);
RFX_API int64_t rfx_win_get_duration_ms(void* handle);
RFX_API void rfx_win_get_spectrum(void* handle, float* out28Bins);

RFX_API int32_t rfx_win_apo_publish_params(const WinDspParams* params);
RFX_API int32_t rfx_win_apo_publish_ir(const float* irL, const float* irR, int32_t length, int32_t useCustomIr);
RFX_API int32_t rfx_win_apo_publish_ir_ex(const float* irL, const float* irR, int32_t length, int32_t irSampleRate, int32_t useCustomIr);
RFX_API int32_t rfx_win_apo_read_telemetry(int32_t* outHeartbeat, int32_t* outSampleRate, int32_t* outChannels, float* out28Bins);

#ifdef __cplusplus
}
#endif
