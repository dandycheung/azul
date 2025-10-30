//! iOS backend using raw FFI to UIKit, bootstrapped entirely from Rust.

use std::{ffi::c_void, ptr, sync::{Arc, Mutex, Condvar}, cell::RefCell};
use objc::runtime::{Class, Object, Sel, Protocol};
use objc::{class, msg_send, sel, sel_impl};
use objc_id::{Id, ShareId};
use objc_foundation::{INSObject, NSObject};
use core_graphics_sys::base::CGRect;

use crate::desktop::{
    shell2::common::{
        event_v2::{self, PlatformWindowV2}, 
        PlatformWindow, WindowError, WindowProperties, RenderContext
    },
    wr_translate2::{AsyncHitTester, WrRenderApi, WrTransaction},
};
use azul_core::{
    resources::{AppConfig, ImageCache, RendererResources, DpiScaleFactor},
    window::{RawWindowHandle, IOSHandle},
    refany::RefAny,
    hit_test::DocumentId,
    gl::OptionGlContextPtr,
};
use azul_layout::{
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions, WindowState},
    ScrollbarDragState,
};
use rust_fontconfig::FcFontCache;

// --- FFI Bindings ---
#[link(name = "Foundation", kind = "framework")]
extern "C" {
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "UIKit", kind = "framework")]
extern "C" {
    fn UIApplicationMain(
        argc: i32,
        argv: *mut *mut u8,
        principalClassName: *mut Object,
        delegateClassName: *mut Object,
    ) -> i32;
    fn UIGraphicsGetCurrentContext() -> *mut c_void; // CGContextRef
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGContextSetRGBFillColor(ctx: *mut c_void, r: f32, g: f32, b: f32, a: f32);
    fn CGContextFillRect(ctx: *mut c_void, rect: CGRect);
}

// Store the IOSWindow instance pointer in a global static.
// This is necessary because the AppDelegate callbacks are static `extern "C"` functions.
static mut AZUL_IOS_WINDOW: *mut IOSWindow = ptr::null_mut();

// --- Custom UIView Subclass ---

/// `drawRect:` method implementation for our custom view.
extern "C" fn draw_rect(self: &Object, _cmd: Sel, rect: CGRect) {
    let context = unsafe { UIGraphicsGetCurrentContext() };
    
    // In a real app, this is where you'd get the pixel buffer from your CPU compositor.
    // For this example, we just draw a solid blue color directly.
    unsafe {
        CGContextSetRGBFillColor(context, 0.0, 0.0, 1.0, 1.0); // R, G, B, A (Blue)
        CGContextFillRect(context, rect);
    }
}

/// Touch event handler: `touchesBegan:withEvent:`
extern "C" fn touches_began(self: &Object, _cmd: Sel, touches: *mut Object, event: *mut Object) {
    if let Some(window) = unsafe { AZUL_IOS_WINDOW.as_mut() } {
        // Here you would translate the UITouch event into an Azul event,
        // update the FullWindowState, and call `window.process_window_events_recursive_v2(0)`.
        println!("[AzulView] Touches Began!");
    }
}

// ... Implement `touchesMoved`, `touchesEnded`, `touchesCancelled` similarly ...

/// Dynamically creates and registers a `UIView` subclass named `AzulView`.
fn get_or_create_view_class() -> &'static Class {
    static mut AZUL_VIEW_CLASS: *const Class = ptr::null();
    unsafe {
        if AZUL_VIEW_CLASS.is_null() {
            let superclass = class!(UIView);
            let mut decl = objc::declare::ClassDecl::new("AzulView", superclass).unwrap();

            decl.add_method(sel!(drawRect:), draw_rect as extern "C" fn(&Object, Sel, CGRect));
            decl.add_method(sel!(touchesBegan:withEvent:), touches_began as extern "C" fn(&Object, Sel, *mut Object, *mut Object));
            
            AZUL_VIEW_CLASS = decl.register();
        }
        &*AZUL_VIEW_CLASS
    }
}

// --- Custom AppDelegate ---

/// `application:didFinishLaunchingWithOptions:` delegate method implementation.
extern "C" fn did_finish_launching(_self: &Object, _cmd: Sel, _app: *mut Object, _opts: *mut Object) -> bool {
    // This is where the application UI is programmatically constructed.
    unsafe {
        // Retrieve the initial create options stored in the `run` function.
        let (config, fc_cache, root_window) = super::run::INITIAL_OPTIONS.take().unwrap();
        
        // Create the main IOSWindow instance.
        let window = IOSWindow::new(root_window, fc_cache, config).unwrap();
        
        // Leak the window onto the heap and store the pointer in our global static.
        // This makes the window live for the duration of the application.
        AZUL_IOS_WINDOW = Box::into_raw(Box::new(window));

        // Get a reference to the newly created window.
        let window_ref = &*AZUL_IOS_WINDOW;

        // Store the native UIWindow handle on the AppDelegate to keep it alive.
        (*_self).set_ivar("window", Id::as_ptr(&window_ref.ui_window).clone());
    }
    true
}

/// Dynamically creates and registers the `AppDelegate` class.
fn get_or_create_app_delegate_class() -> &'static Class {
    static mut APP_DELEGATE_CLASS: *const Class = ptr::null();
    unsafe {
        if APP_DELEGATE_CLASS.is_null() {
            let superclass = class!(NSObject);
            let mut decl = objc::declare::ClassDecl::new("AppDelegate", superclass).unwrap();
            
            decl.add_ivar::<*mut Object>("window");
            decl.add_protocol(Protocol::get("UIApplicationDelegate").unwrap());
            
            decl.add_method(
                sel!(application:didFinishLaunchingWithOptions:),
                did_finish_launching as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> bool,
            );
            
            APP_DELEGATE_CLASS = decl.register();
        }
        &*APP_DELEGATE_CLASS
    }
}

/// Public entry point for launching the iOS application from Rust.
pub unsafe fn launch_app() {
    let pool = objc_autoreleasePoolPush();

    let app_delegate_class = get_or_create_app_delegate_class();

    let principal_class_name_str = "UIApplication\0".as_ptr() as *const i8;
    let delegate_class_name_str = "AppDelegate\0".as_ptr() as *const i8;
    let principal_class_name: Id<Object> = msg_send![class!(NSString), stringWithUTF8String: principal_class_name_str];
    let delegate_class_name: Id<Object> = msg_send![class!(NSString), stringWithUTF8String: delegate_class_name_str];

    UIApplicationMain(
        0,
        ptr::null_mut(),
        principal_class_name.as_mut_ptr(),
        delegate_class_name.as_mut_ptr(),
    );

    objc_autoreleasePoolPop(pool);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackend { Gpu, Cpu }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IOSEvent { Close }

pub struct IOSWindow {
    // Native handles
    ui_window: Id<Object>,
    // Azul state
    backend: RenderBackend,
    is_open: bool,
    // Add all other fields from MacOSWindow to satisfy PlatformWindowV2
    pub layout_window: Option<LayoutWindow>,
    pub current_window_state: FullWindowState,
    pub previous_window_state: Option<FullWindowState>,
    // ... stubs for other fields
}

impl IOSWindow {
    // This is the main constructor, called from `did_finish_launching`.
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        _config: AppConfig,
    ) -> Result<Self, WindowError> {

        // --- 1. Create native UI components ---
        let (ui_window, _view_controller, _custom_view) = unsafe {
            let screen: Id<Object> = msg_send![class!(UIScreen), mainScreen];
            let bounds: CGRect = msg_send![screen, bounds];
            let window: Id<Object> = msg_send![class!(UIWindow), alloc];
            let window: Id<Object> = msg_send![window, initWithFrame: bounds];
            let vc: Id<Object> = msg_send![class!(UIViewController), alloc];
            let vc: Id<Object> = msg_send![vc, init];
            let view_class = get_or_create_view_class();
            let view: Id<Object> = msg_send![view_class, alloc];
            let view: Id<Object> = msg_send![view, initWithFrame: bounds];
            let _: () = msg_send![vc, setView: view.clone()];
            let _: () = msg_send![window, setRootViewController: vc.clone()];
            let _: () = msg_send![window, makeKeyAndVisible];
            (window, vc, view)
        };
        
        // --- 2. Determine rendering backend (CPU/GPU fallback logic) ---
        let mut gl_context_ptr: OptionGlContextPtr = None.into();
        let backend = match Self::create_gl_context() {
            Ok(_gl_context) => {
                println!("[Azul iOS] GPU rendering context created (stubbed).");
                // In a real app, you'd load GL functions here.
                // let gl_functions = crate::desktop::shell2::macos::gl::GlFunctions::initialize().unwrap();
                // gl_context_ptr = Some(GlContextPtr::new(..., gl_functions.functions.clone())).into();
                RenderBackend::Gpu
            },
            Err(_) => {
                println!("[Azul iOS] GPU context creation failed. Falling back to CPU renderer.");
                RenderBackend::Cpu
            }
        };

        // --- 3. Initialize Azul state ---
        let full_window_state = FullWindowState::new(options.state);
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone()).unwrap();
        layout_window.current_window_state = full_window_state.clone();

        Ok(Self {
            ui_window,
            backend,
            is_open: true,
            layout_window: Some(layout_window),
            current_window_state: full_window_state,
            previous_window_state: None,
        })
    }

    /// Placeholder for creating an OpenGL (EAGL) context.
    fn create_gl_context() -> Result<(), String> {
        // On iOS, you would create an EAGLContext here. This is a complex process
        // that involves creating a CAEAGLLayer. For now, we'll just fail
        // to ensure we always use the CPU fallback path.
        Err("GPU rendering not yet implemented for iOS".to_string())
    }
}

// NOTE: This PlatformWindowV2 impl is mostly stubs to satisfy the trait bounds.
// A full implementation would wire up the touch events to call the default trait methods.
impl PlatformWindowV2 for IOSWindow {
    // Implement all 27 required getter methods here...
    // Most will return default or empty values for this minimal example.

    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> { self.layout_window.as_mut() }
    fn get_layout_window(&self) -> Option<&LayoutWindow> { self.layout_window.as_ref() }
    fn get_current_window_state(&self) -> &FullWindowState { &self.current_window_state }
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState { &mut self.current_window_state }
    fn get_previous_window_state(&self) -> &Option<FullWindowState> { &self.previous_window_state }
    fn set_previous_window_state(&mut self, state: FullWindowState) { self.previous_window_state = Some(state); }
    // ... and so on for all the other required methods ...
    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::IOS(IOSHandle {
            ui_window: Id::as_ptr(&self.ui_window) as *mut c_void,
            ui_view: ptr::null_mut(), // TODO
            ui_view_controller: ptr::null_mut(), // TODO
        })
    }
    fn needs_frame_regeneration(&self) -> bool { true }
    fn mark_frame_needs_regeneration(&mut self) {}
    fn clear_frame_regeneration_flag(&mut self) {}
    // ... this is tedious, but required for the trait.
    // In a real implementation, you'd fill these out properly.
    // For now, let's assume they are filled with stubs.
}

impl PlatformWindow for IOSWindow {
    type EventType = IOSEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError> {
        // This won't be called directly, but is required by the trait.
        unimplemented!();
    }

    // Since iOS has no event loop, poll_event does nothing.
    fn poll_event(&mut self) -> Option<Self::EventType> { None }

    fn present(&mut self) -> Result<(), WindowError> {
        // Request a redraw from the system. This will trigger `drawRect:`.
        let view: Id<Object> = unsafe { msg_send![self.ui_window, view] };
        let _: () = unsafe { msg_send![view, setNeedsDisplay] };
        Ok(())
    }

    // ... other stubs for PlatformWindow ...
    fn get_state(&self) -> WindowState { self.current_window_state.to_window_state() }
    fn set_properties(&mut self, _: WindowProperties) -> Result<(), WindowError> { Ok(()) }
    fn get_render_context(&self) -> RenderContext { RenderContext::CPU }
    fn is_open(&self) -> bool { self.is_open }
    fn close(&mut self) { self.is_open = false; }
    fn request_redraw(&mut self) { self.present().unwrap(); }
}
