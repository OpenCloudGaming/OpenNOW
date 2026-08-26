use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSPanel, NSScreen, NSView,
    NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_quartz_core::{CALayer, CAMetalLayer};

use crate::format::{RendererRect, ScreenRect, SurfaceTarget};
use crate::lifecycle::AttachmentLifecycle;
use crate::overlay_should_be_ordered;

use super::{BackendError, NativeSurfaceHandle};

define_class!(
    // NSView has no additional initialization or destruction requirements for a subclass with no
    // ivars. MainThreadOnly preserves AppKit's thread contract.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct PassiveSurfaceView;

    impl PassiveSurfaceView {
        // Returning nil lets hit testing continue through the BrowserWindow content view instead
        // of routing pointer, gesture, or drag input to the video surface.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: CGPoint) -> Option<&NSView> {
            None
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            false
        }

        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            false
        }

        #[unsafe(method(canBecomeKeyView))]
        fn can_become_key_view(&self) -> bool {
            false
        }
    }
);

enum Attachment {
    Dedicated {
        previous_layer: Option<Retained<CALayer>>,
        previous_wants_layer: bool,
    },
    /// SDL_Metal_CreateView owns this CAMetalLayer and keeps its geometry and
    /// drawable size synchronized across resize, fullscreen, Space, and
    /// display transitions. The backend borrows it for presentation but must
    /// never replace or detach it.
    ExistingMetal,
    WindowChild {
        parent: Retained<NSView>,
    },
}

/// Creates the overlay window exactly as `SurfaceOwner::attach` does for `OwnedOverlay`,
/// for isolating window-server behavior without a streaming session.
pub(super) fn debug_overlay_window(main_thread: MainThreadMarker) {
    let rect = ScreenRect::new(200.0, 167.0, 1400.0, 868.0);
    let surface = SurfaceOwner::attach(
        SurfaceTarget::OwnedOverlay(crate::format::OwnedOverlayConfig::new(rect, true)),
        main_thread,
    )
    .expect("production overlay surface");
    if let Some(window) = &surface.window {
        window.orderFrontRegardless();
    }
    surface.visible.store(true, Ordering::Release);
    eprintln!("NVST debug-overlay-window created");
    std::mem::forget(surface);
}

pub(super) struct SurfaceOwner {
    window: Option<Retained<NSWindow>>,
    view: Retained<NSView>,
    layer: Retained<CAMetalLayer>,
    attachment: Attachment,
    visible: Arc<AtomicBool>,
    requested_visible: bool,
    parent_pid: Option<libc::pid_t>,
    owns_window: bool,
    overlay: bool,
    lifecycle: AttachmentLifecycle,
}

impl SurfaceOwner {
    pub(super) fn attach(
        target: SurfaceTarget,
        main_thread: MainThreadMarker,
    ) -> Result<Self, BackendError> {
        let ordered_visible;
        let (
            window,
            view,
            owns_window,
            overlay,
            child_parent,
            requested_visible,
            parent_pid,
            reuse_existing_metal_layer,
        ) = match target {
            SurfaceTarget::OwnedOverlay(config) => {
                let application = NSApplication::sharedApplication(main_thread);
                // A bare executable defaults to NSApplicationActivationPolicyProhibited,
                // which makes the window server refuse to show any of its windows.
                // Accessory lets the overlay appear without a dock icon or menu bar.
                application
                    .setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
                application.finishLaunching();
                config.validate()?;
                let parent_pid = unsafe { libc::getppid() };
                let frontmost = application_is_frontmost(parent_pid);
                let ordered = overlay_should_be_ordered(config.visible, frontmost);
                let styles = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
                // Use AppKit's concrete NSPanel rather than a runtime-defined subclass.
                // The helper's application bundle gives WindowServer the process metadata
                // it needs to composite this passive cross-process overlay reliably.
                let window: Retained<NSPanel> = unsafe {
                    msg_send![
                        main_thread.alloc::<NSPanel>(),
                        initWithContentRect: appkit_screen_frame(config.screen_rect, main_thread)?,
                        styleMask: styles,
                        backing: NSBackingStoreType::Buffered,
                        defer: false
                    ]
                };
                unsafe { window.setReleasedWhenClosed(false) };
                window.setBecomesKeyOnlyIfNeeded(true);
                // The panel belongs to the accessory helper, while Electron remains the
                // active application. NSPanel may otherwise hide itself as soon as the
                // helper deactivates, leaving a live native surface that is never visible.
                window.setHidesOnDeactivate(false);
                // Cross-process child windows are not supported by AppKit. Keep this passive
                // panel above the Electron content while it is ordered, and explicitly order
                // it out whenever the Electron parent is no longer frontmost.
                window.setLevel(NSFloatingWindowLevel);
                window.setCollectionBehavior(
                    NSWindowCollectionBehavior::CanJoinAllSpaces
                        | NSWindowCollectionBehavior::FullScreenAuxiliary
                        | NSWindowCollectionBehavior::IgnoresCycle,
                );
                window.setIgnoresMouseEvents(true);
                window.setAcceptsMouseMovedEvents(false);
                window.setHasShadow(false);
                window.setOpaque(true);
                window.setBackgroundColor(Some(&NSColor::blackColor()));
                let view = window
                    .contentView()
                    .ok_or(BackendError::MissingContentView)?;
                if ordered {
                    window.orderFrontRegardless();
                } else {
                    window.orderOut(None);
                }
                ordered_visible = ordered;
                let panel: Retained<NSPanel> = window;
                let window: Retained<NSWindow> = panel.into_super();
                (
                    Some(window),
                    view,
                    true,
                    true,
                    None,
                    config.visible,
                    Some(parent_pid),
                    false,
                )
            }
            SurfaceTarget::NsView(view) => {
                let view = unsafe { Retained::retain(view.as_ptr().as_ptr().cast::<NSView>()) }
                    .ok_or(BackendError::MissingContentView)?;
                let window = view.window();
                ordered_visible = true;
                (window, view, false, false, None, true, None, true)
            }
            SurfaceTarget::NsWindow(config) => {
                config.validate()?;
                let window =
                    unsafe { Retained::retain(config.window.as_ptr().as_ptr().cast::<NSWindow>()) }
                        .ok_or(BackendError::MissingContentView)?;
                let parent = window
                    .contentView()
                    .ok_or(BackendError::MissingContentView)?;
                let frame = appkit_frame(&parent, config.bounds);
                let child: Retained<PassiveSurfaceView> = unsafe {
                    msg_send![main_thread.alloc::<PassiveSurfaceView>(), initWithFrame: frame]
                };
                child.setHidden(!config.visible);
                ordered_visible = config.visible;
                (
                    Some(window),
                    child.into_super(),
                    false,
                    false,
                    Some(parent),
                    config.visible,
                    None,
                    false,
                )
            }
        };

        let attachment = if reuse_existing_metal_layer {
            Attachment::ExistingMetal
        } else if let Some(parent) = child_parent {
            Attachment::WindowChild { parent }
        } else {
            Attachment::Dedicated {
                previous_layer: view.layer(),
                previous_wants_layer: view.wantsLayer(),
            }
        };
        let layer = if reuse_existing_metal_layer {
            let layer = view.layer().ok_or_else(|| {
                BackendError::Metal("SDL Metal view has no backing CAMetalLayer".to_owned())
            })?;
            layer.downcast::<CAMetalLayer>().map_err(|_| {
                BackendError::Metal("SDL Metal view has a non-Metal backing layer".to_owned())
            })?
        } else {
            CAMetalLayer::new()
        };
        if !reuse_existing_metal_layer {
            layer.setFrame(view.bounds());
        }
        let scale = window
            .as_ref()
            .map_or(1.0, |window| window.backingScaleFactor());
        layer.setContentsScale(scale);
        if !reuse_existing_metal_layer {
            view.setWantsLayer(true);
            view.setLayer(Some(&layer));
        } else {
            eprintln!("macOS presentation using SDL-managed CAMetalLayer");
        }
        if let Attachment::WindowChild { parent } = &attachment {
            parent.addSubview(&view);
        }

        Ok(Self {
            window,
            view,
            layer,
            attachment,
            visible: Arc::new(AtomicBool::new(ordered_visible)),
            requested_visible,
            parent_pid,
            owns_window,
            overlay,
            lifecycle: AttachmentLifecycle::attached(),
        })
    }

    pub(super) fn metal_layer(&self) -> Retained<CAMetalLayer> {
        self.layer.clone()
    }

    pub(super) fn presentation_visibility(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.visible)
    }

    pub(super) fn native_handle(&self) -> NativeSurfaceHandle {
        NativeSurfaceHandle {
            ns_window: self.window.as_ref().map(|window| {
                NonNull::new(Retained::as_ptr(window).cast_mut().cast::<c_void>())
                    .expect("retained NSWindow is non-null")
            }),
            ns_view: NonNull::new(Retained::as_ptr(&self.view).cast_mut().cast::<c_void>())
                .expect("retained NSView is non-null"),
        }
    }

    pub(super) fn update_window_child(
        &mut self,
        bounds: RendererRect,
        visible: bool,
        _main_thread: MainThreadMarker,
    ) -> Result<(), BackendError> {
        bounds.validate()?;
        let Attachment::WindowChild { parent } = &self.attachment else {
            return Err(BackendError::NotWindowSurface);
        };
        self.view.setFrame(appkit_frame(parent, bounds));
        self.view.setHidden(!visible);
        self.layer.setFrame(self.view.bounds());
        self.visible.store(visible, Ordering::Release);
        Ok(())
    }

    pub(super) fn update_owned_overlay(
        &mut self,
        screen_rect: ScreenRect,
        visible: bool,
        main_thread: MainThreadMarker,
    ) -> Result<(), BackendError> {
        if !self.overlay {
            return Err(BackendError::NotOwnedOverlay);
        }
        screen_rect.validate()?;
        let window = self.window.as_ref().ok_or(BackendError::Stopped)?;
        window.setFrame_display(appkit_screen_frame(screen_rect, main_thread)?, true);
        self.layer.setFrame(self.view.bounds());
        self.layer.setContentsScale(window.backingScaleFactor());
        self.requested_visible = visible;
        let frontmost = self.parent_pid.is_some_and(application_is_frontmost);
        let ordered = overlay_should_be_ordered(visible, frontmost);
        if self.visible.swap(ordered, Ordering::AcqRel) != ordered {
            if ordered {
                window.orderFrontRegardless();
            } else {
                window.orderOut(None);
            }
        }
        Ok(())
    }

    pub(super) fn refresh_overlay_ordering(&mut self) -> Result<(), BackendError> {
        if !self.overlay {
            return Ok(());
        }
        let window = self.window.as_ref().ok_or(BackendError::Stopped)?;
        let frontmost = self.parent_pid.is_some_and(application_is_frontmost);
        let ordered = overlay_should_be_ordered(self.requested_visible, frontmost);
        if self.visible.swap(ordered, Ordering::AcqRel) == ordered {
            return Ok(());
        }
        if ordered {
            window.orderFrontRegardless();
        } else {
            window.orderOut(None);
        }
        Ok(())
    }

    pub(super) fn detach(&mut self) {
        if !self.lifecycle.begin_detach() {
            return;
        }
        match &self.attachment {
            Attachment::Dedicated {
                previous_layer,
                previous_wants_layer,
            } => {
                if self.view.layer().is_some_and(|current| {
                    Retained::as_ptr(&current) == Retained::as_ptr(&self.layer).cast()
                }) {
                    self.view.setLayer(previous_layer.as_deref());
                    self.view.setWantsLayer(*previous_wants_layer);
                }
            }
            Attachment::WindowChild { .. } => self.view.removeFromSuperview(),
            Attachment::ExistingMetal => {}
        }
        if self.owns_window {
            if let Some(window) = &self.window {
                window.close();
            }
        }
    }
}

impl Drop for SurfaceOwner {
    fn drop(&mut self) {
        self.detach();
    }
}

fn appkit_frame(parent: &NSView, bounds: RendererRect) -> CGRect {
    let parent_bounds = parent.bounds();
    let parent_rect = RendererRect::new(
        parent_bounds.origin.x,
        parent_bounds.origin.y,
        parent_bounds.size.width,
        parent_bounds.size.height,
    );
    let frame = bounds.to_parent_coordinates(parent_rect, parent.isFlipped());
    CGRect::new(
        CGPoint::new(frame.x, frame.y),
        CGSize::new(frame.width, frame.height),
    )
}

fn appkit_screen_frame(
    screen_rect: ScreenRect,
    main_thread: MainThreadMarker,
) -> Result<CGRect, BackendError> {
    screen_rect.validate()?;
    let primary = NSScreen::screens(main_thread)
        .firstObject()
        .ok_or(BackendError::MissingPrimaryScreen)?;
    let primary_frame = primary.frame();
    Ok(CGRect::new(
        CGPoint::new(
            primary_frame.origin.x + screen_rect.x,
            primary_frame.origin.y + primary_frame.size.height - screen_rect.y - screen_rect.height,
        ),
        CGSize::new(screen_rect.width, screen_rect.height),
    ))
}

fn application_is_frontmost(process_id: libc::pid_t) -> bool {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .is_some_and(|application| application.processIdentifier() == process_id)
}
