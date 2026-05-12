APP_NAME := OpenNOW
BUILD_DIR := build
SRC := $(shell find src -name '*.mm')
BIN := $(BUILD_DIR)/$(APP_NAME)

CXX := clang++
WEBRTC_FRAMEWORK_DIR ?= third_party/webrtc-official
ifneq ($(strip $(WEBRTC_FRAMEWORK_DIR)),)
WEBRTC_FLAT_FRAMEWORK := $(WEBRTC_FRAMEWORK_DIR)/WebRTC.framework
WEBRTC_MACOS_XCFRAMEWORK := $(WEBRTC_FRAMEWORK_DIR)/WebRTC.xcframework/macos-x86_64_arm64
ifneq ($(shell test -d '$(WEBRTC_FLAT_FRAMEWORK)' && printf yes),)
WEBRTC_FRAMEWORK_SEARCH_DIR := $(WEBRTC_FRAMEWORK_DIR)
else
ifneq ($(shell test -d '$(WEBRTC_MACOS_XCFRAMEWORK)/WebRTC.framework' && printf yes),)
WEBRTC_FRAMEWORK_SEARCH_DIR := $(WEBRTC_MACOS_XCFRAMEWORK)
else
WEBRTC_FRAMEWORK_SEARCH_DIR := $(WEBRTC_FRAMEWORK_DIR)
endif
endif
WEBRTC_CFLAGS := -DOPN_HAVE_LIBWEBRTC=1 -F$(WEBRTC_FRAMEWORK_SEARCH_DIR)
WEBRTC_LIBS := -F$(WEBRTC_FRAMEWORK_SEARCH_DIR) -framework WebRTC -Wl,-rpath,$(WEBRTC_FRAMEWORK_SEARCH_DIR)
else
WEBRTC_CFLAGS :=
WEBRTC_LIBS :=
endif

CXXFLAGS := -std=c++20 -Wall -Wextra -Wpedantic -Wno-deprecated-declarations -Wno-gnu-conditional-omitted-operand -fobjc-arc -Isrc $(WEBRTC_CFLAGS)
LDFLAGS := -framework Cocoa -framework QuartzCore -framework Metal -framework MetalKit -framework CoreImage -framework AuthenticationServices -framework AVFoundation -framework AVKit -framework CoreMedia -framework OpenGL -framework GameController -framework ApplicationServices -framework CoreAudio $(WEBRTC_LIBS)

.PHONY: all run clean

all: $(BIN)

$(BIN): $(SRC)
	mkdir -p $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) $(SRC) $(LDFLAGS) -o $(BIN)

run: $(BIN)
	./$(BIN)

clean:
	rm -rf $(BUILD_DIR)
