CXX ?= g++
CXXFLAGS ?= -O3 -std=c++17 -Wall -Wextra -Wno-unused-parameter -fPIC -Isrc

DSP_SRCS = src/ResonanceWinCore.cpp src/ConvolutionEngine.cpp src/resonance_ladspa.cpp

all: libunicore_resonance_fx.so unicore-resonance-test

libunicore_resonance_fx.so: $(DSP_SRCS)
	$(CXX) $(CXXFLAGS) -shared -o $@ $(DSP_SRCS) -lm

unicore-resonance-test: src/resonance_test.cpp
	$(CXX) -O2 -std=c++17 -Wall -Wextra -o $@ src/resonance_test.cpp -ldl -lm

test: all
	./unicore-resonance-test ./libunicore_resonance_fx.so

clean:
	rm -f libunicore_resonance_fx.so unicore-resonance-test
