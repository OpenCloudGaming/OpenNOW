// MACOS COMPILE REQUIRED — this file requires macOS + node-addon-api
// Cannot be compiled or verified on Linux CI.

#include <napi.h>
#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#include <IOSurface/IOSurface.h>
#include <mach/mach.h>
#include <iostream>
#include <memory>
#include <string>

class EmbeddedRendererState {
public:
  IOSurfaceRef iosurface = nullptr;
  NSView* nsview = nullptr;
  CALayer* layer = nullptr;
  std::string window_handle;
  uint32_t iosurface_id = 0;
  int width = 0;
  int height = 0;
  bool attached = false;
  bool attachment_failed_logged = false;

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

// Safely run a block on the main thread.
// If already on main thread, executes inline (avoids dispatch_sync deadlock).
// Otherwise dispatches sync or async as requested.
static void RunOnMain(dispatch_block_t block, bool sync) {
  if ([NSThread isMainThread]) {
    block();
  } else if (sync) {
    dispatch_sync(dispatch_get_main_queue(), block);
  } else {
    dispatch_async(dispatch_get_main_queue(), block);
  }
}

// Resolve a host NSView from a hex pointer string.
// Electron's getNativeWindowHandle() on macOS returns an NSView*, but we also
// handle NSWindow* and other objects that can resolve to an NSView.
static NSView* ResolveHostView(const std::string& handle) {
  if (handle.empty()) {
    return nil;
  }

  unsigned long long ptr_val = 0;
  try {
    ptr_val = std::stoull(handle, nullptr, 16);
  } catch (...) {
    std::cerr << "[renderer.mm] Failed to parse hex handle: " << handle << std::endl;
    return nil;
  }
  if (ptr_val == 0) {
    return nil;
  }

  id obj = (__bridge id)(reinterpret_cast<void*>(ptr_val));

  // 1. Direct NSView (most common — Electron macOS returns NSView*)
  if ([obj isKindOfClass:[NSView class]]) {
    std::cout << "[renderer.mm] Resolved native handle as NSView ("
              << [[obj className] UTF8String] << ")" << std::endl;
    return (NSView*)obj;
  }

  // 2. NSWindow -> contentView
  if ([obj isKindOfClass:[NSWindow class]]) {
    NSWindow* window = (NSWindow*)obj;
    NSView* cv = [window contentView];
    if (cv != nil) {
      std::cout << "[renderer.mm] Resolved native handle as NSWindow ("
                << [[obj className] UTF8String] << ").contentView" << std::endl;
      return cv;
    }
    std::cerr << "[renderer.mm] NSWindow has nil contentView" << std::endl;
    return nil;
  }

  // 3. Object with -contentView selector
  if ([obj respondsToSelector:@selector(contentView)]) {
    id cv = [obj performSelector:@selector(contentView)];
    if (cv != nil && [cv isKindOfClass:[NSView class]]) {
      std::cout << "[renderer.mm] Resolved native handle via -contentView ("
                << [[obj className] UTF8String] << ")" << std::endl;
      return (NSView*)cv;
    }
  }

  // 4. Object with -window selector -> NSWindow.contentView
  if ([obj respondsToSelector:@selector(window)]) {
    id win = [obj performSelector:@selector(window)];
    if (win != nil && [win isKindOfClass:[NSWindow class]]) {
      NSView* cv = [(NSWindow*)win contentView];
      if (cv != nil) {
        std::cout << "[renderer.mm] Resolved native handle via -window.contentView ("
                  << [[obj className] UTF8String] << ")" << std::endl;
        return cv;
      }
    }
  }

  std::cerr << "[renderer.mm] Native handle class was "
            << [[obj className] UTF8String] << "; unsupported" << std::endl;
  return nil;
}

static void AttachToHost(EmbeddedRendererState* state) {
  if (!state || !state->nsview || state->attached) {
    return;
  }

  NSView* host = ResolveHostView(state->window_handle);
  if (!host) {
    if (!state->attachment_failed_logged) {
      state->attachment_failed_logged = true;
      std::cerr << "[renderer.mm] Could not find host view for window handle; "
                << "will retry on next update" << std::endl;
    }
    return;
  }

  if (state->nsview.superview != nil) {
    [state->nsview removeFromSuperview];
  }

  [host addSubview:state->nsview];

  state->nsview.frame = NSMakeRect(0, 0, state->width, state->height);
  if (state->layer) {
    state->layer.frame = CGRectMake(0, 0, state->width, state->height);
  }

  state->attached = true;
  state->attachment_failed_logged = false;

  std::cout << "[renderer.mm] Attached embedded IOSurface view to host view ("
            << [[host className] UTF8String] << ") "
            << state->width << "x" << state->height << std::endl;
}

static IOSurfaceRef CreateNewIOSurface(int width, int height, uint32_t* out_id) {
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
  uint32_t surface_id = IOSurfaceGetID(surface);
  *out_id = surface_id;

  std::cout << "[renderer.mm] Created IOSurface: id=" << surface_id
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

  if (g_state) {
    g_state.reset();
  }

  g_state = std::make_unique<EmbeddedRendererState>();
  g_state->window_handle = window_handle;
  g_state->width = width;
  g_state->height = height;

  uint32_t surface_id = 0;
  g_state->iosurface = CreateNewIOSurface(width, height, &surface_id);
  if (g_state->iosurface == nullptr) {
    g_state.reset();
    Napi::Error::New(env, "Failed to create IOSurface")
      .ThrowAsJavaScriptException();
    return env.Null().As<Napi::Object>();
  }

  g_state->iosurface_id = surface_id;

  // Create NSView + CALayer on main thread, then attempt attachment.
  // Uses sync so the view exists before we return to JS.
  RunOnMain(^{
    if (!g_state) return;

    NSRect frame = NSMakeRect(0, 0, width, height);
    g_state->nsview = [[NSView alloc] initWithFrame:frame];
    g_state->nsview.wantsLayer = YES;

    g_state->layer = [CALayer layer];
    g_state->layer.contents = (__bridge id)g_state->iosurface;
    g_state->layer.frame = CGRectMake(0, 0, width, height);
    g_state->nsview.layer = g_state->layer;

    AttachToHost(g_state.get());
  }, true);

  Napi::Object result = Napi::Object::New(env);
  result.Set("iosurfaceId", Napi::Number::New(env, surface_id));
  return result;
}

// NAPI: updateSurface
void UpdateSurface(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();

  if (info.Length() < 7 || !g_state || !g_state->nsview) {
    return;
  }

  std::string window_handle = info[0].As<Napi::String>();
  int x = info[1].As<Napi::Number>().Int32Value();
  int y = info[2].As<Napi::Number>().Int32Value();
  int width = info[3].As<Napi::Number>().Int32Value();
  int height = info[4].As<Napi::Number>().Int32Value();
  double scale = info[5].As<Napi::Number>().DoubleValue();
  bool visible = info[6].As<Napi::Boolean>().Value();

  g_state->width = width;
  g_state->height = height;
  if (!window_handle.empty()) {
    g_state->window_handle = window_handle;
  }

  RunOnMain(^{
    if (!g_state || !g_state->nsview) return;

    // Retry attachment if not yet attached or if superview disappeared
    if (!g_state->attached || g_state->nsview.superview == nil) {
      g_state->attached = false;
      AttachToHost(g_state.get());
    }

    g_state->nsview.frame = NSMakeRect(x, y, width, height);
    g_state->nsview.hidden = !visible;

    if (g_state->layer) {
      g_state->layer.frame = CGRectMake(0, 0, width, height);
    }
  }, true);
}

// NAPI: destroySurface
// Synchronous — must complete before a subsequent createSurface call.
void DestroySurface(const Napi::CallbackInfo& info) {
  if (g_state) {
    RunOnMain(^{
      g_state.reset();
    }, true);
  }
}

// NAPI: notifyFrameReady
void NotifyFrameReady(const Napi::CallbackInfo& info) {
  if (!g_state || !g_state->layer) {
    return;
  }

  dispatch_async(dispatch_get_main_queue(), ^{
    if (!g_state || !g_state->layer) {
      return;
    }
    if (g_state->iosurface) {
      g_state->layer.contents = (__bridge id)g_state->iosurface;
    }
    [g_state->layer setNeedsDisplay];
    [g_state->nsview setNeedsDisplay:YES];
  });
}

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
