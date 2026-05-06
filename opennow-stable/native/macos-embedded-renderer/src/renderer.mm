// MACOS COMPILE REQUIRED — this file requires macOS + node-addon-api
// Cannot be compiled or verified on Linux CI.

#include <napi.h>
#import <Cocoa/Cocoa.h>
#import <CoreImage/CoreImage.h>
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
  NSView* host_view = nullptr;
  std::string window_handle;
  uint32_t iosurface_id = 0;
  int width = 0;
  int height = 0;
  bool attached = false;
  bool attachment_failed_logged = false;
  uint64_t frame_ready_count = 0;
  uint64_t draw_count = 0;
  uint64_t draw_skip_count = 0;
  CFTimeInterval last_draw_log_time = 0;

  ~EmbeddedRendererState() {
    if (iosurface != nullptr) {
      IOSurfaceDecrementUseCount(iosurface);
      CFRelease(iosurface);
      iosurface = nullptr;
    }
    if (nsview != nullptr) {
      [nsview removeFromSuperview];
      nsview = nil;
    }
  }
};

static std::unique_ptr<EmbeddedRendererState> g_state;

@interface EmbeddedIOSurfaceView : NSView
- (void)setIOSurface:(IOSurfaceRef)surface;
- (void)presentCurrentFrame;
@end

@implementation EmbeddedIOSurfaceView {
  IOSurfaceRef _surface;
  CIContext* _ciContext;
}

- (instancetype)initWithFrame:(NSRect)frameRect {
  self = [super initWithFrame:frameRect];
  return self;
}

- (BOOL)isOpaque {
  return YES;
}

- (void)dealloc {
  if (_surface != nullptr) {
    CFRelease(_surface);
    _surface = nullptr;
  }
  _ciContext = nil;
}

- (void)setIOSurface:(IOSurfaceRef)surface {
  if (surface == _surface) {
    return;
  }

  if (surface != nullptr) {
    CFRetain(surface);
  }
  IOSurfaceRef previous = _surface;
  _surface = surface;
  if (previous != nullptr) {
    CFRelease(previous);
  }

  [self setNeedsDisplay:YES];
}

- (void)presentCurrentFrame {
  [self setNeedsDisplay:YES];
  [self display];
}

- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];

  CGContextRef context = NSGraphicsContext.currentContext.CGContext;
  if (context == nullptr) {
    return;
  }

  CGContextSetFillColorWithColor(context, NSColor.blackColor.CGColor);
  CGContextFillRect(context, NSRectToCGRect(self.bounds));

  IOSurfaceRef surface = _surface;
  if (surface == nullptr) {
    if (g_state) {
      g_state->draw_skip_count += 1;
    }
    return;
  }

  NSDictionary* options = @{
    kCIImageColorSpace: [NSNull null],
  };
  CIImage* image = [[CIImage alloc] initWithIOSurface:surface options:options];
  if (image == nil) {
    if (g_state) {
      g_state->draw_skip_count += 1;
    }
    return;
  }

  CGRect imageRect = CGRectMake(0, 0, IOSurfaceGetWidth(surface), IOSurfaceGetHeight(surface));
  if (_ciContext == nil) {
    _ciContext = [CIContext contextWithOptions:@{
      kCIContextWorkingColorSpace: [NSNull null],
      kCIContextOutputColorSpace: [NSNull null],
    }];
  }

  CGImageRef cgImage = [_ciContext createCGImage:image fromRect:imageRect];
  if (cgImage == nullptr) {
    if (g_state) {
      g_state->draw_skip_count += 1;
    }
    return;
  }

  CGRect bounds = NSRectToCGRect(self.bounds);
  CGContextSaveGState(context);
  CGContextTranslateCTM(context, CGRectGetMinX(bounds), CGRectGetMinY(bounds) + CGRectGetHeight(bounds));
  CGContextScaleCTM(context, 1.0, -1.0);
  CGContextDrawImage(context, CGRectMake(0, 0, CGRectGetWidth(bounds), CGRectGetHeight(bounds)), cgImage);
  CGContextRestoreGState(context);
  CGImageRelease(cgImage);

  if (g_state) {
    g_state->draw_count += 1;
  }
}

@end

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

static NSView* ResolveCurrentHostView(EmbeddedRendererState* state) {
  if (!state) {
    return nil;
  }

  if (state->host_view != nil) {
    return state->host_view;
  }

  state->host_view = ResolveHostView(state->window_handle);
  return state->host_view;
}

static void ApplyGeometry(EmbeddedRendererState* state,
                          int x,
                          int y,
                          int width,
                          int height,
                          double scale,
                          const char* source) {
  if (!state || !state->nsview) {
    return;
  }

  NSView* host = ResolveCurrentHostView(state);
  if (!host) {
    std::cerr << "[renderer.mm] [" << source << "] Missing host view for geometry update" << std::endl;
    return;
  }

  const double safe_scale = scale > 0.0 ? scale : 1.0;
  const NSRect host_bounds = host.bounds;
  const CGFloat x_pts = static_cast<CGFloat>(x / safe_scale);
  const CGFloat y_pts_top = static_cast<CGFloat>(y / safe_scale);
  const CGFloat width_pts = static_cast<CGFloat>(width / safe_scale);
  const CGFloat height_pts = static_cast<CGFloat>(height / safe_scale);
  const CGFloat y_pts = [host isFlipped]
    ? y_pts_top
    : NSMaxY(host_bounds) - y_pts_top - height_pts;
  const NSRect requested_frame = NSMakeRect(x_pts, y_pts, width_pts, height_pts);
  NSRect frame = NSIntersectionRect(requested_frame, host_bounds);
  if (NSIsEmptyRect(frame)) {
    frame = NSZeroRect;
  }

  state->nsview.frame = frame;
  [state->nsview setNeedsDisplay:YES];

  std::cout << "[renderer.mm] [" << source << "] hostBoundsPts="
            << host_bounds.origin.x << "," << host_bounds.origin.y << " "
            << host_bounds.size.width << "x" << host_bounds.size.height
            << " inputPx=" << x << "," << y << " " << width << "x" << height
            << " scale=" << safe_scale
            << " convertedPts=" << x_pts << "," << y_pts_top << " " << width_pts << "x" << height_pts
            << " requestedFramePts=" << requested_frame.origin.x << "," << requested_frame.origin.y << " "
            << requested_frame.size.width << "x" << requested_frame.size.height
            << " appliedFramePts=" << frame.origin.x << "," << frame.origin.y << " "
            << frame.size.width << "x" << frame.size.height << std::endl;
}

static void AttachToHost(EmbeddedRendererState* state) {
  if (!state || !state->nsview || state->attached) {
    return;
  }

  NSView* host = ResolveHostView(state->window_handle);
  if (!host) {
    state->host_view = nil;
    if (!state->attachment_failed_logged) {
      state->attachment_failed_logged = true;
      std::cerr << "[renderer.mm] Could not find host view for window handle; "
                << "will retry on next update" << std::endl;
    }
    return;
  }

  state->host_view = host;

  if (state->nsview.superview != nil) {
    [state->nsview removeFromSuperview];
  }

  [host addSubview:state->nsview];

  state->attached = true;
  state->attachment_failed_logged = false;

  std::cout << "[renderer.mm] Attached embedded IOSurface view to host view ("
            << [[host className] UTF8String] << ") "
            << state->width << "x" << state->height << std::endl;
}

static void RefreshPresentedSurface(EmbeddedRendererState* state) {
  if (!state || !state->nsview || !state->iosurface) {
    return;
  }

  EmbeddedIOSurfaceView* surfaceView = (EmbeddedIOSurfaceView*)state->nsview;
  [surfaceView presentCurrentFrame];

  state->frame_ready_count += 1;
  const CFTimeInterval now = CACurrentMediaTime();
  if ((now - state->last_draw_log_time) >= 2.0) {
    state->last_draw_log_time = now;
    std::cout << "[renderer.mm] Explicit IOSurface draw path active frameReady="
              << state->frame_ready_count << " draws=" << state->draw_count
              << " skipped=" << state->draw_skip_count << " surfaceId=" << state->iosurface_id
              << " size=" << state->width << "x" << state->height << std::endl;
  }
}

static IOSurfaceRef CreateNewIOSurface(int width, int height, uint32_t* out_id) {
  NSDictionary* properties = @{
    (__bridge NSString*)kIOSurfaceWidth: @(width),
    (__bridge NSString*)kIOSurfaceHeight: @(height),
    (__bridge NSString*)kIOSurfacePixelFormat: @(kCVPixelFormatType_32BGRA),
    (__bridge NSString*)kIOSurfaceBytesPerElement: @4,
    (__bridge NSString*)kIOSurfaceIsGlobal: @YES,
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

  // Create NSView on main thread, then attempt attachment.
  // Uses sync so the view exists before we return to JS.
  RunOnMain(^{
    if (!g_state) return;

    const double safe_scale = scale > 0.0 ? scale : 1.0;
    NSRect frame = NSMakeRect(0, 0, width / safe_scale, height / safe_scale);
    EmbeddedIOSurfaceView* surfaceView = [[EmbeddedIOSurfaceView alloc] initWithFrame:frame];
    [surfaceView setIOSurface:g_state->iosurface];
    g_state->nsview = surfaceView;

    AttachToHost(g_state.get());
    ApplyGeometry(g_state.get(), 0, 0, width, height, safe_scale, "createSurface");
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
      g_state->host_view = nil;
      AttachToHost(g_state.get());
    }

    ApplyGeometry(g_state.get(), x, y, width, height, scale, "updateSurface");
    g_state->nsview.hidden = !visible;
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
  if (!g_state || !g_state->nsview) {
    return;
  }

  dispatch_async(dispatch_get_main_queue(), ^{
    if (!g_state || !g_state->nsview) {
      return;
    }
    RefreshPresentedSurface(g_state.get());
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
