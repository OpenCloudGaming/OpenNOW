APP_NAME := OpenNOW
BUILD_DIR := build
SRC := $(shell find src -name '*.mm')
BIN := $(BUILD_DIR)/$(APP_NAME)
INFO_PLIST := OpenNOW-Info.plist

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
LDFLAGS := -framework Cocoa -framework QuartzCore -framework Metal -framework MetalKit -framework CoreImage -framework AuthenticationServices -framework AVFoundation -framework AVKit -framework CoreMedia -framework OpenGL -framework GameController -framework ApplicationServices -framework CoreAudio -framework ScreenCaptureKit -Wl,-sectcreate,__TEXT,__info_plist,$(INFO_PLIST) $(WEBRTC_LIBS)

.PHONY: all run clean qt-configure qt-build qt-run qt-clean libwebrtc-sdk qt-configure-webrtc qt-build-webrtc qt-run-webrtc

all: $(BIN)

$(BIN): $(SRC)
	mkdir -p $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) $(SRC) $(LDFLAGS) -o $(BIN)

run: $(BIN)
	./$(BIN)

clean:
	rm -rf $(BUILD_DIR)

qt-configure:
	cmake -S qt -B $(BUILD_DIR)/qt -DCMAKE_BUILD_TYPE=RelWithDebInfo

qt-build: qt-configure
	cmake --build $(BUILD_DIR)/qt

qt-run: qt-build
	cmake --build $(BUILD_DIR)/qt --target run

qt-clean:
	rm -rf $(BUILD_DIR)/qt

libwebrtc-sdk:
	scripts/build-libwebrtc-sdk.sh

qt-configure-webrtc:
	cmake -S qt -B $(BUILD_DIR)/qt-webrtc -DCMAKE_BUILD_TYPE=RelWithDebInfo -DOPNQT_ENABLE_LIBWEBRTC=ON

qt-build-webrtc: qt-configure-webrtc
	cmake --build $(BUILD_DIR)/qt-webrtc

qt-run-webrtc: qt-build-webrtc
	cmake --build $(BUILD_DIR)/qt-webrtc --target run
