// MACOS COMPILE REQUIRED — this file requires macOS + node-addon-api
// Cannot be compiled or verified on Linux CI.

#include <napi.h>
#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#include <IOSurface/IOSurface.h>
#include <iostream>
#include <memory>
#include <map>

// Holds the state for a single IOSurface + NSView pair
class EmbeddedRendererState {
public:
  IOSurfaceRef iosurface = nullptr;
  NSView* nsview = nullptr;
  CALayer* layer = nullptr;
  std::string window_handle;
  uint32_t iosurface_id = 0;
  int width = 0;
  int height = 0;
  
  ~EmbeddedRendererState() {
    if (iosurface != nullptr) {
      IOSurfaceDecrementUseCount(iosurface);
      iosurface = nullptr;
    }
    if (nsview != nullptr) {
      [nsview removeFromSuperview];
      nsview = nil;
    }
    if (layer != nullptr) {
      layer = nil;
    }
  }
};

static std::unique_ptr<EmbeddedRendererState> g_state;

// Get BrowserWindow's contentView from the hex window handle string.
// Electron's getNativeWindowHandle() on macOS returns a pointer to NSWindow.
// The handle is serialized as a hex string (e.g. "0x7f9a3c800000").
NSView* GetContentViewFromHandle(const std::string& handle) {
  if (handle.empty()) {
    return nil;
  }
  // Parse the hex string to a pointer value.
  unsigned long long ptr_val = 0;
  try {
    ptr_val = std::stoull(handle, nullptr, 16);
  } catch (...) {
    return nil;
  }
  if (ptr_val == 0) {
    return nil;
  }
  // Cast to NSWindow directly — valid because this addon is loaded in-process
  // by Electron main, so the pointer is in the same address space.
  NSWindow* window = (__bridge NSWindow*)(reinterpret_cast<void*>(ptr_val));
  if (![window isKindOfClass:[NSWindow class]]) {
    return nil;
  }
  return window.contentView;
}

// Create an IOSurface for pixel data (BGRA format)
IOSurfaceRef CreateIOSurface(int width, int height, uint32_t* out_id) {
  NSDictionary* properties = @{
    (__bridge NSString*)kIOSurfaceWidth: @(width),
    (__bridge NSString*)kIOSurfaceHeight: @(height),
    (__bridge NSString*)kIOSurfacePixelFormat: @(kCVPixelFormatType_32BGRA),
    (__bridge NSString*)kIOSurfaceBytesPerElement: @4,
  };
  
  IOSurfaceRef surface = IOSurfaceCreate((__bridge CFDictionaryRef)properties);
  if (surface == nullptr) {
    std::cerr << "[renderer.mm] Failed to create IOSurface" << std::endl;
    return nullptr;
  }
  
  IOSurfaceIncrementUseCount(surface);
  *out_id = IOSurfaceGetID(surface);
  
  std::cout << "[renderer.mm] Created IOSurface: id=" << *out_id 
            << " " << width << "x" << height << std::endl;
  return surface;
}

// NAPI: createSurface
Napi::Object CreateSurface(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  
  if (info.Length() < 4) {
    Napi::TypeError::New(env, "createSurface requires 4 arguments")
      .ThrowAsJavaScriptException();
    return env.Null().As<Napi::Object>();
  }
  
  std::string window_handle = info[0].As<Napi::String>();
  int width = info[1].As<Napi::Number>().Int32Value();
  int height = info[2].As<Napi::Number>().Int32Value();
  double scale = info[3].As<Napi::Number>().DoubleValue();
  
  if (width <= 0 || height <= 0) {
    Napi::TypeError::New(env, "Invalid surface dimensions")
      .ThrowAsJavaScriptException();
    return env.Null().As<Napi::Object>();
  }
  
  // Dispose old state if present
  if (g_state) {
    g_state.reset();
  }
  
  g_state = std::make_unique<EmbeddedRendererState>();
  g_state->window_handle = window_handle;
  g_state->width = width;
  g_state->height = height;
  
  // Create IOSurface
  uint32_t iosurface_id = 0;
  g_state->iosurface = CreateIOSurface(width, height, &iosurface_id);
  if (g_state->iosurface == nullptr) {
    g_state.reset();
    Napi::Error::New(env, "Failed to create IOSurface")
      .ThrowAsJavaScriptException();
    return env.Null().As<Napi::Object>();
  }
  
  g_state->iosurface_id = iosurface_id;
  
  // Find the content view and add NSView
  NSView* content_view = GetContentViewFromHandle(window_handle);
  if (content_view == nil) {
    std::cerr << "[renderer.mm] Could not find content view for window handle" << std::endl;
    g_state.reset();
    Napi::Error::New(env, "Could not find window content view")
      .ThrowAsJavaScriptException();
    return env.Null().As<Napi::Object>();
  }
  
  // Create NSView with CALayer for IOSurface
  NSRect frame = NSMakeRect(0, 0, width, height);
  g_state->nsview = [[NSView alloc] initWithFrame:frame];
  g_state->nsview.wantsLayer = YES;
  
  g_state->layer = [CALayer layer];
  g_state->layer.contents = (__bridge id)g_state->iosurface;
  g_state->layer.frame = CGRectMake(0, 0, width, height);
  g_state->nsview.layer = g_state->layer;
  
  [content_view addSubview:g_state->nsview];
  
  std::cout << "[renderer.mm] Created NSView with IOSurface layer" << std::endl;
  
  // Return { iosurfaceId }
  Napi::Object result = Napi::Object::New(env);
  result.Set("iosurfaceId", Napi::Number::New(env, iosurface_id));
  return result;
}

// NAPI: updateSurface
void UpdateSurface(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  
  if (info.Length() < 7 || !g_state || !g_state->nsview) {
    return; // No-op if state not initialized
  }
  
  std::string window_handle = info[0].As<Napi::String>();
  int x = info[1].As<Napi::Number>().Int32Value();
  int y = info[2].As<Napi::Number>().Int32Value();
  int width = info[3].As<Napi::Number>().Int32Value();
  int height = info[4].As<Napi::Number>().Int32Value();
  double scale = info[5].As<Napi::Number>().DoubleValue();
  bool visible = info[6].As<Napi::Boolean>().Value();
  
  // Update NSView frame
  g_state->nsview.frame = NSMakeRect(x, y, width, height);
  g_state->nsview.hidden = !visible;
  
  // Update layer frame
  if (g_state->layer) {
    g_state->layer.frame = CGRectMake(0, 0, width, height);
  }
}

// NAPI: destroySurface
void DestroySurface(const Napi::CallbackInfo& info) {
  if (g_state) {
    g_state.reset();
  }
}

// NAPI: notifyFrameReady
void NotifyFrameReady(const Napi::CallbackInfo& info) {
  if (!g_state || !g_state->layer) {
    return;
  }
  
  // Update layer contents and mark for redisplay
  if (g_state->iosurface) {
    g_state->layer.contents = (__bridge id)g_state->iosurface;
  }
  
  [g_state->layer setNeedsDisplay];
  [g_state->nsview setNeedsDisplay:YES];
}

// Init function for the NAPI module
Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set(Napi::String::New(env, "createSurface"),
              Napi::Function::New(env, CreateSurface));
  exports.Set(Napi::String::New(env, "updateSurface"),
              Napi::Function::New(env, UpdateSurface));
  exports.Set(Napi::String::New(env, "destroySurface"),
              Napi::Function::New(env, DestroySurface));
  exports.Set(Napi::String::New(env, "notifyFrameReady"),
              Napi::Function::New(env, NotifyFrameReady));
  return exports;
}

NODE_API_MODULE(renderer, Init)
