#pragma once

#include <complex>
#include <vector>
#include <mutex>

/**
 * Uniformly Partitioned Overlap-Add (UPOLA) Stereo FFT Convolver.
 * Runs real-time convolution for loaded .irs / .wav impulse responses or synthesized acoustic IRs.
 */
class ConvolutionEngine {
public:
    static constexpr int kBlockSize = 512;
    static constexpr int kFftSize = 1024;
    static constexpr int kMaxPartitions = 32; // Up to 16,384 samples (~340ms @ 48kHz)

    ConvolutionEngine();

    void resetState();
    void loadImpulseResponse(const float* irLeft, const float* irRight, int length);
    void loadBuiltInPreset(int presetIndex, int sampleRate);

    // Processes exactly numFrames stereo samples in-place or to output arrays (thread-safe try_lock)
    void process(const float* inL, const float* inR, float* outL, float* outR, int numFrames, float wetMix);

private:
    struct PartitionSet {
        int numPartitions = 0;
        std::vector<std::vector<std::complex<float>>> irSpecL;
        std::vector<std::vector<std::complex<float>>> irSpecR;
    };

    static void fft(std::vector<std::complex<float>>& buf, bool inverse);
    void processBlock512(const float* blkInL, const float* blkInR, float* blkOutL, float* blkOutR, const PartitionSet& parts);

    std::mutex irMutex_;
    PartitionSet partitions_;
    bool hasValidIr_ = false;
    bool hasProcessedAudio_ = false;
    bool fadeInPending_ = false;
    int fadeInRemaining_ = 0;

    // Input/output framing queues to handle arbitrary Oboe callback sizes seamlessly
    std::vector<float> inQueueL_;
    std::vector<float> inQueueR_;
    int inQueueFill_ = 0;

    std::vector<float> outQueueL_;
    std::vector<float> outQueueR_;

    // Overlap-add & FDL (Frequency-domain Delay Line) state
    std::vector<std::vector<std::complex<float>>> fdlL_;
    std::vector<std::vector<std::complex<float>>> fdlR_;
    int fdlIndex_ = 0;

    std::vector<float> overlapL_;
    std::vector<float> overlapR_;
    std::vector<float> dryDelayL_;
    std::vector<float> dryDelayR_;

    // Scratch buffers
    std::vector<std::complex<float>> fftBuf_;
    std::vector<std::complex<float>> accumSpecL_;
    std::vector<std::complex<float>> accumSpecR_;
    std::vector<float> blockOutL_;
    std::vector<float> blockOutR_;
};
