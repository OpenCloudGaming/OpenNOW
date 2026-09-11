#pragma once

#include <memory>

class NativeStreamRuntime;
class StreamVideoRenderCallback;

[[nodiscard]] std::shared_ptr<StreamVideoRenderCallback> createNativeStreamRenderCallback(
    NativeStreamRuntime *runtime);
