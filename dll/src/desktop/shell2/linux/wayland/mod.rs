//! Wayland implementation for Linux.
//!
//! This module implements the PlatformWindow trait for Wayland.
//! It supports GPU-accelerated rendering via EGL and WebRender, with a
//! fallback to a CPU-rendered surface if GL context creation fails.
//!
//! Note: Uses dynamic loading (dlopen) to avoid linker errors
//! and ensure compatibility across Linux distributions.

mod defines;
mod dlopen;
mod events;
mod gl;
pub mod menu;

use std::{
    cell::RefCell,
    ffi::{c_void, CString},
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::DomId,
    events::{MouseButton, ProcessEventResult},
    geom::LogicalPosition,
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::{DocumentId, FullHitTest},
    refany::RefAny,
    resources::{AppConfig, Au, DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        CursorPosition, HwAcceleration, KeyboardState, MouseCursorType, MouseState,
        RawWindowHandle, RendererType, WaylandHandle, WindowDecorations,
    },
};
use azul_layout::{
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions, WindowState},
    ScrollbarDragState,
};
use rust_fontconfig::FcFontCache;
use webrender::Renderer as WrRenderer;

use self::{
    defines::*,
    dlopen::{Library, Wayland, Xkb},
};
use super::common::gl::GlFunctions;
use crate::desktop::{
    shell2::common::{
        event_v2::{self, PlatformWindowV2},
        PlatformWindow, RenderContext, WindowError, WindowProperties,
    },
    wr_translate2::{self, AsyncHitTester, Notifier},
};

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    /// CPU fallback - initialized lazily after receiving wl_shm from registry
    Cpu(Option<CpuFallbackState>),
}

/// State for CPU fallback rendering.
struct CpuFallbackState {
    wayland: Rc<Wayland>,
    pool: *mut defines::wl_shm_pool,
    buffer: *mut defines::wl_buffer,
    data: *mut u8,
    width: i32,
    height: i32,
    stride: i32,
}

pub struct WaylandWindow {
    wayland: Rc<Wayland>,
    xkb: Rc<Xkb>,
    pub display: *mut defines::wl_display,
    registry: *mut defines::wl_registry,
    compositor: *mut defines::wl_compositor,
    shm: *mut defines::wl_shm,
    seat: *mut defines::wl_seat,
    xdg_wm_base: *mut defines::xdg_wm_base,
    surface: *mut defines::wl_surface,
    xdg_surface: *mut defines::xdg_surface,
    xdg_toplevel: *mut defines::xdg_toplevel,
    event_queue: *mut defines::wl_event_queue,
    keyboard_state: events::KeyboardState,
    pointer_state: events::PointerState,
    is_open: bool,
    configured: bool,

    // Shell2 state
    pub layout_window: Option<LayoutWindow>,
    pub current_window_state: FullWindowState,
    pub previous_window_state: Option<FullWindowState>,
    pub render_api: Option<webrender::RenderApi>,
    pub renderer: Option<WrRenderer>,
    pub hit_tester: Option<AsyncHitTester>,
    pub document_id: Option<DocumentId>,
    pub image_cache: ImageCache,
    pub renderer_resources: RendererResources,
    gl_context_ptr: OptionGlContextPtr,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    id_namespace: Option<IdNamespace>,

    render_mode: RenderMode,

    // V2 Event system state
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    pub last_hovered_node: Option<event_v2::HitTestNode>,
    pub frame_needs_regeneration: bool,
    pub frame_callback_pending: bool, // Wayland frame callback synchronization

    // Shared resources
    pub resources: Arc<super::AppResources>,
    fc_cache: Arc<FcFontCache>,
    app_data: Arc<RefCell<RefAny>>,
}

#[derive(Debug, Clone, Copy)]
pub enum WaylandEvent {
    Redraw,
    Close,
    Other,
}

// ============================================================================
// Wayland Popup Window (for menus using xdg_popup)
// ============================================================================

/// Wayland popup window using xdg_popup protocol
///
/// This is used for menus and other transient popup surfaces. Unlike WaylandWindow
/// which uses xdg_toplevel, this uses xdg_popup which provides:
/// - Parent-relative positioning
/// - Compositor-managed stacking
/// - Automatic grab support
/// - Automatic dismissal on outside clicks
pub struct WaylandPopup {
    wayland: Rc<Wayland>,
    xkb: Rc<Xkb>,
    display: *mut defines::wl_display,
    parent_surface: *mut defines::wl_surface,
    surface: *mut defines::wl_surface,
    xdg_surface: *mut defines::xdg_surface,
    xdg_popup: *mut defines::xdg_popup,
    positioner: *mut defines::xdg_positioner,
    compositor: *mut defines::wl_compositor,
    seat: *mut defines::wl_seat,
    event_queue: *mut defines::wl_event_queue,
    keyboard_state: events::KeyboardState,
    pointer_state: events::PointerState,
    is_open: bool,
    configured: bool,

    // Shell2 state (same as WaylandWindow)
    pub layout_window: Option<LayoutWindow>,
    pub current_window_state: FullWindowState,
    pub previous_window_state: Option<FullWindowState>,
    pub render_api: Option<webrender::RenderApi>,
    pub renderer: Option<WrRenderer>,
    pub hit_tester: Option<AsyncHitTester>,
    pub document_id: Option<DocumentId>,
    pub image_cache: ImageCache,
    pub renderer_resources: RendererResources,
    gl_context_ptr: OptionGlContextPtr,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    id_namespace: Option<IdNamespace>,
    render_mode: RenderMode,

    // V2 Event system state
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    pub last_hovered_node: Option<event_v2::HitTestNode>,
    pub frame_needs_regeneration: bool,
    pub frame_callback_pending: bool,

    // Shared resources
    pub resources: Arc<super::AppResources>,
    fc_cache: Arc<FcFontCache>,
    app_data: Arc<RefCell<RefAny>>,
}

// ============================================================================
// Event Handler Types
// ============================================================================

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events,
    /// keys)
    RootNodes,
}

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct HitTestNode {
    dom_id: u64,
    node_id: u64,
}

// ============================================================================
// XKB Keyboard Translation
// ============================================================================

/// Translate XKB keysym to Azul VirtualKeyCode
///
/// XKB keysyms are defined in <xkbcommon/xkbcommon-keysyms.h>
/// This function maps common keysyms to VirtualKeyCode variants.
fn translate_keysym_to_virtual_keycode(keysym: u32) -> azul_core::window::VirtualKeyCode {
    use azul_core::window::VirtualKeyCode;

    // XKB keysym constants (from xkbcommon-keysyms.h)
    const XKB_KEY_Escape: u32 = 0xff1b;
    const XKB_KEY_Return: u32 = 0xff0d;
    const XKB_KEY_Tab: u32 = 0xff09;
    const XKB_KEY_BackSpace: u32 = 0xff08;
    const XKB_KEY_Delete: u32 = 0xffff;
    const XKB_KEY_Insert: u32 = 0xff63;
    const XKB_KEY_Home: u32 = 0xff50;
    const XKB_KEY_End: u32 = 0xff57;
    const XKB_KEY_Page_Up: u32 = 0xff55;
    const XKB_KEY_Page_Down: u32 = 0xff56;

    const XKB_KEY_Left: u32 = 0xff51;
    const XKB_KEY_Up: u32 = 0xff52;
    const XKB_KEY_Right: u32 = 0xff53;
    const XKB_KEY_Down: u32 = 0xff54;

    const XKB_KEY_F1: u32 = 0xffbe;
    const XKB_KEY_F2: u32 = 0xffbf;
    const XKB_KEY_F3: u32 = 0xffc0;
    const XKB_KEY_F4: u32 = 0xffc1;
    const XKB_KEY_F5: u32 = 0xffc2;
    const XKB_KEY_F6: u32 = 0xffc3;
    const XKB_KEY_F7: u32 = 0xffc4;
    const XKB_KEY_F8: u32 = 0xffc5;
    const XKB_KEY_F9: u32 = 0xffc6;
    const XKB_KEY_F10: u32 = 0xffc7;
    const XKB_KEY_F11: u32 = 0xffc8;
    const XKB_KEY_F12: u32 = 0xffc9;

    const XKB_KEY_Shift_L: u32 = 0xffe1;
    const XKB_KEY_Shift_R: u32 = 0xffe2;
    const XKB_KEY_Control_L: u32 = 0xffe3;
    const XKB_KEY_Control_R: u32 = 0xffe4;
    const XKB_KEY_Alt_L: u32 = 0xffe9;
    const XKB_KEY_Alt_R: u32 = 0xffea;
    const XKB_KEY_Super_L: u32 = 0xffeb;
    const XKB_KEY_Super_R: u32 = 0xffec;

    const XKB_KEY_space: u32 = 0x0020;
    const XKB_KEY_comma: u32 = 0x002c;
    const XKB_KEY_period: u32 = 0x002e;
    const XKB_KEY_slash: u32 = 0x002f;
    const XKB_KEY_semicolon: u32 = 0x003b;
    const XKB_KEY_apostrophe: u32 = 0x0027;
    const XKB_KEY_bracketleft: u32 = 0x005b;
    const XKB_KEY_bracketright: u32 = 0x005d;
    const XKB_KEY_backslash: u32 = 0x005c;
    const XKB_KEY_minus: u32 = 0x002d;
    const XKB_KEY_equal: u32 = 0x003d;
    const XKB_KEY_grave: u32 = 0x0060;

    match keysym {
        // Special keys
        XKB_KEY_Escape => VirtualKeyCode::Escape,
        XKB_KEY_Return => VirtualKeyCode::Return,
        XKB_KEY_Tab => VirtualKeyCode::Tab,
        XKB_KEY_BackSpace => VirtualKeyCode::Back,
        XKB_KEY_Delete => VirtualKeyCode::Delete,
        XKB_KEY_Insert => VirtualKeyCode::Insert,
        XKB_KEY_Home => VirtualKeyCode::Home,
        XKB_KEY_End => VirtualKeyCode::End,
        XKB_KEY_Page_Up => VirtualKeyCode::PageUp,
        XKB_KEY_Page_Down => VirtualKeyCode::PageDown,

        // Arrow keys
        XKB_KEY_Left => VirtualKeyCode::Left,
        XKB_KEY_Up => VirtualKeyCode::Up,
        XKB_KEY_Right => VirtualKeyCode::Right,
        XKB_KEY_Down => VirtualKeyCode::Down,

        // Function keys
        XKB_KEY_F1 => VirtualKeyCode::F1,
        XKB_KEY_F2 => VirtualKeyCode::F2,
        XKB_KEY_F3 => VirtualKeyCode::F3,
        XKB_KEY_F4 => VirtualKeyCode::F4,
        XKB_KEY_F5 => VirtualKeyCode::F5,
        XKB_KEY_F6 => VirtualKeyCode::F6,
        XKB_KEY_F7 => VirtualKeyCode::F7,
        XKB_KEY_F8 => VirtualKeyCode::F8,
        XKB_KEY_F9 => VirtualKeyCode::F9,
        XKB_KEY_F10 => VirtualKeyCode::F10,
        XKB_KEY_F11 => VirtualKeyCode::F11,
        XKB_KEY_F12 => VirtualKeyCode::F12,

        // Modifier keys
        XKB_KEY_Shift_L | XKB_KEY_Shift_R => VirtualKeyCode::LShift,
        XKB_KEY_Control_L | XKB_KEY_Control_R => VirtualKeyCode::LControl,
        XKB_KEY_Alt_L | XKB_KEY_Alt_R => VirtualKeyCode::LAlt,
        XKB_KEY_Super_L | XKB_KEY_Super_R => VirtualKeyCode::LWin,

        // Punctuation
        XKB_KEY_space => VirtualKeyCode::Space,
        XKB_KEY_comma => VirtualKeyCode::Comma,
        XKB_KEY_period => VirtualKeyCode::Period,
        XKB_KEY_slash => VirtualKeyCode::Slash,
        XKB_KEY_semicolon => VirtualKeyCode::Semicolon,
        XKB_KEY_apostrophe => VirtualKeyCode::Apostrophe,
        XKB_KEY_bracketleft => VirtualKeyCode::LBracket,
        XKB_KEY_bracketright => VirtualKeyCode::RBracket,
        XKB_KEY_backslash => VirtualKeyCode::Backslash,
        XKB_KEY_minus => VirtualKeyCode::Minus,
        XKB_KEY_equal => VirtualKeyCode::Equals,
        XKB_KEY_grave => VirtualKeyCode::Grave,

        // Letters a-z (lowercase keysyms 0x0061-0x007a)
        0x0061 => VirtualKeyCode::A,
        0x0062 => VirtualKeyCode::B,
        0x0063 => VirtualKeyCode::C,
        0x0064 => VirtualKeyCode::D,
        0x0065 => VirtualKeyCode::E,
        0x0066 => VirtualKeyCode::F,
        0x0067 => VirtualKeyCode::G,
        0x0068 => VirtualKeyCode::H,
        0x0069 => VirtualKeyCode::I,
        0x006a => VirtualKeyCode::J,
        0x006b => VirtualKeyCode::K,
        0x006c => VirtualKeyCode::L,
        0x006d => VirtualKeyCode::M,
        0x006e => VirtualKeyCode::N,
        0x006f => VirtualKeyCode::O,
        0x0070 => VirtualKeyCode::P,
        0x0071 => VirtualKeyCode::Q,
        0x0072 => VirtualKeyCode::R,
        0x0073 => VirtualKeyCode::S,
        0x0074 => VirtualKeyCode::T,
        0x0075 => VirtualKeyCode::U,
        0x0076 => VirtualKeyCode::V,
        0x0077 => VirtualKeyCode::W,
        0x0078 => VirtualKeyCode::X,
        0x0079 => VirtualKeyCode::Y,
        0x007a => VirtualKeyCode::Z,

        // Letters A-Z (uppercase keysyms 0x0041-0x005a)
        0x0041 => VirtualKeyCode::A,
        0x0042 => VirtualKeyCode::B,
        0x0043 => VirtualKeyCode::C,
        0x0044 => VirtualKeyCode::D,
        0x0045 => VirtualKeyCode::E,
        0x0046 => VirtualKeyCode::F,
        0x0047 => VirtualKeyCode::G,
        0x0048 => VirtualKeyCode::H,
        0x0049 => VirtualKeyCode::I,
        0x004a => VirtualKeyCode::J,
        0x004b => VirtualKeyCode::K,
        0x004c => VirtualKeyCode::L,
        0x004d => VirtualKeyCode::M,
        0x004e => VirtualKeyCode::N,
        0x004f => VirtualKeyCode::O,
        0x0050 => VirtualKeyCode::P,
        0x0051 => VirtualKeyCode::Q,
        0x0052 => VirtualKeyCode::R,
        0x0053 => VirtualKeyCode::S,
        0x0054 => VirtualKeyCode::T,
        0x0055 => VirtualKeyCode::U,
        0x0056 => VirtualKeyCode::V,
        0x0057 => VirtualKeyCode::W,
        0x0058 => VirtualKeyCode::X,
        0x0059 => VirtualKeyCode::Y,
        0x005a => VirtualKeyCode::Z,

        // Numbers 0-9 (keysyms 0x0030-0x0039)
        0x0030 => VirtualKeyCode::Key0,
        0x0031 => VirtualKeyCode::Key1,
        0x0032 => VirtualKeyCode::Key2,
        0x0033 => VirtualKeyCode::Key3,
        0x0034 => VirtualKeyCode::Key4,
        0x0035 => VirtualKeyCode::Key5,
        0x0036 => VirtualKeyCode::Key6,
        0x0037 => VirtualKeyCode::Key7,
        0x0038 => VirtualKeyCode::Key8,
        0x0039 => VirtualKeyCode::Key9,

        // Unknown key - default to Escape
        _ => VirtualKeyCode::Escape,
    }
}

// ============================================================================
// PlatformWindow Implementation
// ============================================================================

impl PlatformWindow for WaylandWindow {
    type EventType = WaylandEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
        let resources = Arc::new(super::AppResources::default_for_testing());
        Self::new(options, resources)
    }

    fn get_state(&self) -> WindowState {
        WindowState {
            title: self.current_window_state.title.clone(),
            size: self.current_window_state.size,
            position: self.current_window_state.position,
            flags: self.current_window_state.flags,
            theme: self.current_window_state.theme,
            debug_state: self.current_window_state.debug_state,
            keyboard_state: self.current_window_state.keyboard_state.clone(),
            mouse_state: self.current_window_state.mouse_state.clone(),
            touch_state: self.current_window_state.touch_state.clone(),
            ime_position: self.current_window_state.ime_position,
            platform_specific_options: self.current_window_state.platform_specific_options.clone(),
            renderer_options: self.current_window_state.renderer_options,
            background_color: self.current_window_state.background_color,
            layout_callback: self.current_window_state.layout_callback.clone(),
            close_callback: self.current_window_state.close_callback.clone(),
            monitor: self.current_window_state.monitor.clone(),
        }
    }

    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError> {
        if let Some(title) = props.title {
            self.current_window_state.title = title.clone().into();
            let c_title = CString::new(title).unwrap();
            unsafe { (self.wayland.xdg_toplevel_set_title)(self.xdg_toplevel, c_title.as_ptr()) };
        }
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        if unsafe {
            (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue)
        } > 0
        {
            Some(WaylandEvent::Redraw) // Events were processed, a redraw might be needed.
        } else {
            None
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match &self.render_mode {
            RenderMode::Gpu(ctx, _) => ctx
                .egl_context
                .map(|c| RenderContext::OpenGL {
                    context: c as *mut _,
                })
                .unwrap_or(RenderContext::CPU),
            RenderMode::Cpu(_) => RenderContext::CPU,
        }
    }

    fn present(&mut self) -> Result<(), WindowError> {
        match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(Some(cpu_state)) => {
                cpu_state.draw_blue();
                unsafe {
                    (self.wayland.wl_surface_attach)(self.surface, cpu_state.buffer, 0, 0);
                    (self.wayland.wl_surface_damage)(
                        self.surface,
                        0,
                        0,
                        cpu_state.width,
                        cpu_state.height,
                    );
                    (self.wayland.wl_surface_commit)(self.surface);
                }
                Ok(())
            }
            RenderMode::Cpu(None) => {
                // CPU fallback not yet initialized - wait for wl_shm from registry
                Ok(())
            }
        }
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
    fn close(&mut self) {
        self.is_open = false;
    }
    fn request_redraw(&mut self) {
        if self.configured {
            self.present().ok();
        }
    }
}

// ============================================================================
// PlatformWindowV2 Trait Implementation (Cross-platform V2 Event System)
// ============================================================================

impl PlatformWindowV2 for WaylandWindow {
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }

    fn get_layout_window(&self) -> Option<&LayoutWindow> {
        self.layout_window.as_ref()
    }

    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }

    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState {
        &mut self.current_window_state
    }

    fn get_previous_window_state(&self) -> &Option<FullWindowState> {
        &self.previous_window_state
    }

    fn set_previous_window_state(&mut self, state: FullWindowState) {
        self.previous_window_state = Some(state);
    }

    fn get_last_hovered_node(&self) -> Option<&event_v2::HitTestNode> {
        self.last_hovered_node.as_ref()
    }

    fn set_last_hovered_node(&mut self, node: Option<event_v2::HitTestNode>) {
        self.last_hovered_node = node;
    }

    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState> {
        self.scrollbar_drag_state.as_ref()
    }

    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState> {
        &mut self.scrollbar_drag_state
    }

    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>) {
        self.scrollbar_drag_state = state;
    }

    fn get_image_cache_mut(&mut self) -> &mut ImageCache {
        &mut self.image_cache
    }

    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources {
        &mut self.renderer_resources
    }

    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr {
        &self.gl_context_ptr
    }

    fn get_fc_cache(&self) -> &Arc<FcFontCache> {
        &self.fc_cache
    }

    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle> {
        &self.resources.system_style
    }

    fn get_app_data(&self) -> &Arc<RefCell<RefAny>> {
        &self.app_data
    }

    fn get_render_api_mut(&mut self) -> &mut webrender::RenderApi {
        self.render_api
            .as_mut()
            .expect("Render API not initialized")
    }

    fn get_render_api(&self) -> &webrender::RenderApi {
        self.render_api
            .as_ref()
            .expect("Render API not initialized")
    }

    fn get_document_id(&self) -> DocumentId {
        self.document_id.expect("Document ID not initialized")
    }

    fn get_id_namespace(&self) -> IdNamespace {
        self.id_namespace.expect("ID namespace not initialized")
    }

    fn get_hit_tester(&self) -> &AsyncHitTester {
        self.hit_tester
            .as_ref()
            .expect("Hit tester not initialized")
    }

    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester {
        self.hit_tester
            .as_mut()
            .expect("Hit tester not initialized")
    }

    fn get_renderer(&self) -> Option<&WrRenderer> {
        self.renderer.as_ref()
    }

    fn get_renderer_mut(&mut self) -> Option<&mut WrRenderer> {
        self.renderer.as_mut()
    }

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Wayland(WaylandHandle {
            surface: self.surface as *mut c_void,
            display: self.display as *mut c_void,
        })
    }

    fn needs_frame_regeneration(&self) -> bool {
        self.frame_needs_regeneration
    }

    fn mark_frame_needs_regeneration(&mut self) {
        self.frame_needs_regeneration = true;
    }

    fn clear_frame_regeneration_flag(&mut self) {
        self.frame_needs_regeneration = false;
    }

    fn prepare_callback_invocation(&mut self) -> event_v2::InvokeSingleCallbackBorrows {
        let layout_window = self
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event_v2::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::Wayland(WaylandHandle {
                surface: self.surface as *mut c_void,
                display: self.display as *mut c_void,
            }),
            gl_context_ptr: &self.gl_context_ptr,
            image_cache: &mut self.image_cache,
            fc_cache_clone: (*self.fc_cache).clone(),
            system_style: self.resources.system_style.clone(),
            previous_window_state: &self.previous_window_state,
            current_window_state: &self.current_window_state,
            renderer_resources: &mut self.renderer_resources,
        }
    }
}

impl WaylandWindow {
    pub fn new(
        options: WindowCreateOptions,
        resources: Arc<super::AppResources>,
    ) -> Result<Self, WindowError> {
        let wayland = Wayland::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libwayland-client: {:?}", e))
        })?;
        let xkb = Xkb::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libxkbcommon: {:?}", e))
        })?;

        let display = unsafe { (wayland.wl_display_connect)(std::ptr::null()) };
        if display.is_null() {
            return Err(WindowError::PlatformError(
                "Failed to connect to Wayland display".into(),
            ));
        }

        let event_queue = unsafe { (wayland.wl_display_create_event_queue)(display) };
        let registry = unsafe { (wayland.wl_display_get_registry)(display) };
        unsafe { (wayland.wl_proxy_set_queue)(registry as _, event_queue) };

        // Initialize LayoutWindow
        let layout_window = LayoutWindow::new((*resources.fc_cache).clone()).map_err(|e| {
            WindowError::PlatformError(format!("LayoutWindow::new failed: {:?}", e))
        })?;

        let mut window = Self {
            wayland: wayland.clone(),
            xkb,
            display,
            event_queue,
            registry,
            compositor: std::ptr::null_mut(),
            shm: std::ptr::null_mut(),
            seat: std::ptr::null_mut(),
            xdg_wm_base: std::ptr::null_mut(),
            surface: std::ptr::null_mut(),
            xdg_surface: std::ptr::null_mut(),
            xdg_toplevel: std::ptr::null_mut(),
            is_open: true,
            configured: false,
            current_window_state: FullWindowState {
                title: options.state.title.clone(),
                size: options.state.size,
                position: options.state.position,
                flags: options.state.flags,
                theme: options.state.theme,
                debug_state: options.state.debug_state,
                keyboard_state: options.state.keyboard_state.clone(),
                mouse_state: options.state.mouse_state.clone(),
                touch_state: options.state.touch_state.clone(),
                ime_position: options.state.ime_position,
                platform_specific_options: options.state.platform_specific_options.clone(),
                renderer_options: options.state.renderer_options,
                background_color: options.state.background_color,
                layout_callback: options.state.layout_callback.clone(),
                close_callback: options.state.close_callback.clone(),
                monitor: options.state.monitor.clone(),
                last_hit_test: FullHitTest::empty(None),
                focused_node: None,
                hovered_file: None,
                dropped_file: None,
                selections: Default::default(),
                window_focused: false,
            },
            previous_window_state: None,
            layout_window: Some(layout_window),
            render_api: None,
            renderer: None,
            hit_tester: None,
            document_id: None,
            image_cache: ImageCache::default(),
            renderer_resources: RendererResources::default(),
            gl_context_ptr: None.into(),
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            id_namespace: None,
            keyboard_state: events::KeyboardState::new(),
            pointer_state: events::PointerState::new(),
            scrollbar_drag_state: None,
            last_hovered_node: None,
            frame_needs_regeneration: false,
            frame_callback_pending: false,
            // CPU rendering state will be initialized after receiving wl_shm from registry
            render_mode: RenderMode::Cpu(None),
            resources: resources.clone(),
            fc_cache: resources.fc_cache.clone(),
            app_data: resources.app_data.clone(),
        };

        let listener = defines::wl_registry_listener {
            global: events::registry_global_handler,
            global_remove: events::registry_global_remove_handler,
        };
        unsafe {
            (window.wayland.wl_proxy_add_listener)(
                registry as _,
                &listener as *const _ as _,
                &mut window as *mut _ as *mut _,
            )
        };
        unsafe { (window.wayland.wl_display_roundtrip)(display) };

        window.surface =
            unsafe { (window.wayland.wl_compositor_create_surface)(window.compositor) };
        window.xdg_surface = unsafe {
            (window.wayland.xdg_wm_base_get_xdg_surface)(window.xdg_wm_base, window.surface)
        };

        let xdg_surface_listener = defines::xdg_surface_listener {
            configure: events::xdg_surface_configure_handler,
        };
        unsafe {
            (window.wayland.xdg_surface_add_listener)(
                window.xdg_surface,
                &xdg_surface_listener,
                &mut window as *mut _ as *mut _,
            )
        };

        window.xdg_toplevel =
            unsafe { (window.wayland.xdg_surface_get_toplevel)(window.xdg_surface) };
        let title = CString::new(options.state.title.as_str()).unwrap();
        unsafe { (window.wayland.xdg_toplevel_set_title)(window.xdg_toplevel, title.as_ptr()) };

        let width = options.state.size.dimensions.width as i32;
        let height = options.state.size.dimensions.height as i32;

        let render_mode = match gl::GlContext::new(&wayland, display, window.surface, width, height)
        {
            Ok(mut gl_context) => {
                let gl_functions =
                    GlFunctions::initialize(gl_context.egl.as_ref().unwrap()).unwrap();
                RenderMode::Gpu(gl_context, gl_functions)
            }
            Err(e) => {
                eprintln!(
                    "[Wayland] GPU context failed: {:?}. Falling back to CPU.",
                    e
                );
                RenderMode::Cpu(Some(CpuFallbackState::new(
                    &wayland, window.shm, width, height,
                )?))
            }
        };
        window.render_mode = render_mode;

        if let RenderMode::Gpu(gl_context, gl_functions) = &mut window.render_mode {
            gl_context.make_current();
            // Borrow gl_functions separately to avoid double mutable borrow
            let gl_funcs_ref = gl_functions as *const GlFunctions;
            window.initialize_webrender(&options, unsafe { &*gl_funcs_ref })?;
        }

        unsafe { (window.wayland.wl_surface_commit)(window.surface) };
        unsafe { (window.wayland.wl_display_flush)(display) };

        Ok(window)
    }

    fn initialize_webrender(
        &mut self,
        options: &WindowCreateOptions,
        gl_functions: &GlFunctions,
    ) -> Result<(), WindowError> {
        let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let (renderer, sender) = webrender::create_webrender_instance(
            gl_functions.functions.clone(),
            Box::new(Notifier {
                new_frame_ready: new_frame_ready.clone(),
            }),
            wr_translate2::default_renderer_options(options),
            None,
        )
        .map_err(|e| WindowError::PlatformError(format!("WebRender init failed: {:?}", e)))?;

        self.renderer = Some(renderer);
        self.render_api = Some(sender.create_api());
        let render_api = self.render_api.as_mut().unwrap();

        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            self.current_window_state.size.dimensions.width as i32,
            self.current_window_state.size.dimensions.height as i32,
        );
        let wr_doc_id = render_api.add_document(framebuffer_size);
        self.document_id = Some(wr_translate2::translate_document_id_wr(wr_doc_id));
        self.id_namespace = Some(wr_translate2::translate_id_namespace_wr(
            render_api.get_namespace_id(),
        ));
        let hit_tester_request = render_api.request_hit_tester(wr_doc_id);
        self.hit_tester = Some(AsyncHitTester::Requested(hit_tester_request));
        self.gl_context_ptr = OptionGlContextPtr::Some(GlContextPtr::new(
            RendererType::Hardware,
            gl_functions.functions.clone(),
        ));
        self.new_frame_ready = new_frame_ready;

        Ok(())
    }

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        if unsafe { (self.wayland.wl_display_dispatch_queue)(self.display, self.event_queue) } == -1
        {
            Err(WindowError::PlatformError(
                "Wayland connection closed".into(),
            ))
        } else {
            Ok(())
        }
    }

    /// Process events using state-diffing architecture.
    /// V2: Uses cross-platform dispatch system with recursive callback handling.
    pub fn process_events(&mut self) -> ProcessEventResult {
        self.process_window_events_recursive_v2(0)
    }

    /// Handle keyboard key event with full XKB translation
    pub fn handle_key(&mut self, key: u32, state: u32) {
        use azul_core::window::{OptionChar, OptionVirtualKeyCode};

        // Only process key press events (state == 1)
        let is_pressed = state == 1;

        // XKB uses keycode = evdev_keycode + 8
        let xkb_keycode = key + 8;

        // Get XKB state
        let xkb_state = self.keyboard_state.state;
        if xkb_state.is_null() {
            // XKB not initialized yet
            self.current_window_state.keyboard_state.current_char = OptionChar::None;
            self.current_window_state
                .keyboard_state
                .current_virtual_keycode = OptionVirtualKeyCode::None;
            return;
        }

        // Get keysym (symbolic key identifier)
        let keysym = unsafe { (self.xkb.xkb_state_key_get_one_sym)(xkb_state, xkb_keycode) };

        // Translate keysym to VirtualKeyCode
        let virtual_keycode = translate_keysym_to_virtual_keycode(keysym);
        self.current_window_state
            .keyboard_state
            .current_virtual_keycode = OptionVirtualKeyCode::Some(virtual_keycode);

        // Update pressed_virtual_keycodes and pressed_scancodes lists
        if is_pressed {
            // Add key to pressed lists
            self.current_window_state
                .keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(virtual_keycode);
            self.current_window_state
                .keyboard_state
                .pressed_scancodes
                .insert_hm_item(key);
        } else {
            // Remove key from pressed lists
            self.current_window_state
                .keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&virtual_keycode);
            self.current_window_state
                .keyboard_state
                .pressed_scancodes
                .remove_hm_item(&key);
        }

        // Get UTF-8 character (if printable)
        if is_pressed {
            let mut buffer = [0i8; 32];
            let len = unsafe {
                (self.xkb.xkb_state_key_get_utf8)(
                    xkb_state,
                    xkb_keycode,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                )
            };

            if len > 0 && len < buffer.len() as i32 {
                let utf8_str = unsafe {
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        buffer.as_ptr() as *const u8,
                        len as usize,
                    ))
                };
                if let Some(ch) = utf8_str.chars().next() {
                    self.current_window_state.keyboard_state.current_char =
                        OptionChar::Some(ch as u32); // OptionChar expects u32
                } else {
                    self.current_window_state.keyboard_state.current_char = OptionChar::None;
                }
            } else {
                self.current_window_state.keyboard_state.current_char = OptionChar::None;
            }
        } else {
            self.current_window_state.keyboard_state.current_char = OptionChar::None;
        }

        self.frame_needs_regeneration = true;
    }

    /// Handle pointer motion event
    pub fn handle_pointer_motion(&mut self, x: f64, y: f64) {
        let logical_pos = LogicalPosition::new(x as f32, y as f32);
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(logical_pos);

        // Update hit test for hover effects
        self.update_hit_test(logical_pos);

        self.frame_needs_regeneration = true;
    }

    /// Handle pointer button event
    pub fn handle_pointer_button(&mut self, serial: u32, button: u32, state: u32) {
        self.pointer_state.serial = serial;

        let mouse_button = match button {
            0x110 => MouseButton::Left,   // BTN_LEFT
            0x111 => MouseButton::Right,  // BTN_RIGHT
            0x112 => MouseButton::Middle, // BTN_MIDDLE
            _ => return,
        };

        if state == 1 {
            // Button pressed
            self.current_window_state.mouse_state.left_down = mouse_button == MouseButton::Left;
            self.current_window_state.mouse_state.right_down = mouse_button == MouseButton::Right;
            self.current_window_state.mouse_state.middle_down = mouse_button == MouseButton::Middle;
            self.pointer_state.button_down = Some(mouse_button);
        } else {
            // Button released
            self.current_window_state.mouse_state.left_down = false;
            self.current_window_state.mouse_state.right_down = false;
            self.current_window_state.mouse_state.middle_down = false;
            self.pointer_state.button_down = None;
        }

        self.frame_needs_regeneration = true;
    }

    /// Handle pointer axis (scroll) event
    pub fn handle_pointer_axis(&mut self, axis: u32, value: f64) {
        use azul_css::OptionF32;

        const WL_POINTER_AXIS_VERTICAL_SCROLL: u32 = 0;
        const WL_POINTER_AXIS_HORIZONTAL_SCROLL: u32 = 1;

        match axis {
            WL_POINTER_AXIS_VERTICAL_SCROLL => {
                let current = match self.current_window_state.mouse_state.scroll_y {
                    OptionF32::Some(v) => v,
                    OptionF32::None => 0.0,
                };
                self.current_window_state.mouse_state.scroll_y =
                    OptionF32::Some(current + value as f32);
            }
            WL_POINTER_AXIS_HORIZONTAL_SCROLL => {
                let current = match self.current_window_state.mouse_state.scroll_x {
                    OptionF32::Some(v) => v,
                    OptionF32::None => 0.0,
                };
                self.current_window_state.mouse_state.scroll_x =
                    OptionF32::Some(current + value as f32);
            }
            _ => {}
        }

        self.frame_needs_regeneration = true;
    }

    /// Handle pointer enter event
    pub fn handle_pointer_enter(&mut self, serial: u32, x: f64, y: f64) {
        self.pointer_state.serial = serial;
        let logical_pos = LogicalPosition::new(x as f32, y as f32);
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(logical_pos);
        self.update_hit_test(logical_pos);
        self.frame_needs_regeneration = true;
    }

    /// Handle pointer leave event
    pub fn handle_pointer_leave(&mut self, _serial: u32) {
        // Get last known position before leaving
        let last_pos = match self.current_window_state.mouse_state.cursor_position {
            CursorPosition::InWindow(pos) => pos,
            _ => LogicalPosition::zero(),
        };
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(last_pos);
        self.current_window_state.last_hit_test = FullHitTest::empty(None);
        self.frame_needs_regeneration = true;
    }

    /// Update hit test at current cursor position
    fn update_hit_test(&mut self, position: LogicalPosition) {
        use azul_core::geom::PhysicalPosition;

        if let Some(AsyncHitTester::Resolved(ref hit_tester)) = self.hit_tester {
            let physical_pos_u32 = position.to_physical(
                self.current_window_state
                    .size
                    .get_hidpi_factor()
                    .inner
                    .get(),
            );
            let physical_pos =
                PhysicalPosition::new(physical_pos_u32.x as f32, physical_pos_u32.y as f32);

            let hit_test_result =
                hit_tester.hit_test(wr_translate2::translate_world_point(physical_pos));
            self.current_window_state.last_hit_test =
                wr_translate2::translate_hit_test_result(hit_test_result);
        }
    }

    /// Process window events (V2 wrapper for external use)
    pub fn process_window_events_v2(&mut self) -> ProcessEventResult {
        self.process_events();
        ProcessEventResult::DoNothing
    }

    /// Regenerate layout after DOM changes
    ///
    /// Wayland-specific implementation with mandatory CSD injection.
    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        use crate::desktop::csd;

        let layout_window = match &mut self.layout_window {
            Some(lw) => lw,
            None => return Err("No layout window".into()),
        };

        // 1. Call user's layout callback to get new DOM
        let layout_callback = self.current_window_state.layout_callback.clone();

        let mut callback_info = LayoutCallbackInfo::new(
            self.current_window_state.size,
            self.current_window_state.theme,
            &self.image_cache,
            &self.gl_context_ptr,
            &*self.fc_cache,
            self.resources.system_style.clone(),
        );

        let user_dom = match &layout_callback {
            azul_core::callbacks::LayoutCallback::Raw(inner) => (inner.cb)(
                &mut self.resources.app_data.borrow_mut(),
                &mut callback_info,
            ),
            azul_core::callbacks::LayoutCallback::Marshaled(marshaled) => (marshaled.cb.cb)(
                &mut marshaled.marshal_data.clone(),
                &mut self.resources.app_data.borrow_mut(),
                &mut callback_info,
            ),
        };

        // 2. Check if CSD should be injected (can be disabled by user via has_decorations flag)
        // On Wayland, there are no native decorations, but the user can disable CSD to have a
        // borderless window
        let should_inject_csd = csd::should_inject_csd(
            self.current_window_state.flags.has_decorations,
            self.current_window_state.flags.decorations,
        );
        let has_minimize = true;
        let has_maximize = true;

        let final_dom = if should_inject_csd {
            csd::wrap_user_dom_with_decorations(
                user_dom,
                &self.current_window_state.title.as_str(),
                should_inject_csd,
                has_minimize,
                has_maximize,
                &self.resources.system_style,
            )
        } else {
            user_dom
        };

        // 3. Perform layout with LayoutWindow
        layout_window
            .layout_and_generate_display_list(
                final_dom,
                &self.current_window_state,
                &self.renderer_resources,
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &mut None, // debug_messages
            )
            .map_err(|e| format!("Layout failed: {:?}", e))?;

        // 4. Calculate scrollbar states based on new layout
        layout_window.scroll_states.calculate_scrollbar_states();

        // 5. Rebuild display list and send to WebRender
        if let Some(ref mut render_api) = self.render_api {
            let dpi_factor = self.current_window_state.size.get_hidpi_factor();
            let mut txn = webrender::Transaction::new();
            crate::desktop::wr_translate2::rebuild_display_list(
                &mut txn,
                layout_window,
                render_api,
                &self.image_cache,
                Vec::new(),
                &mut self.renderer_resources,
                dpi_factor,
            );

            // 6. Synchronize scrollbar opacity with GPU cache
            let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
            for (dom_id, layout_result) in &layout_window.layout_results {
                azul_layout::LayoutWindow::synchronize_scrollbar_opacity(
                    &mut layout_window.gpu_state_manager,
                    &layout_window.scroll_states,
                    *dom_id,
                    &layout_result.layout_tree,
                    &system_callbacks,
                    azul_core::task::Duration::System(
                        azul_core::task::SystemTimeDiff::from_millis(500),
                    ), // fade_delay
                    azul_core::task::Duration::System(
                        azul_core::task::SystemTimeDiff::from_millis(200),
                    ), // fade_duration
                );
            }
        }

        // 7. Mark frame needs regeneration
        self.frame_needs_regeneration = true;

        Ok(())
    }

    /// Synchronize window state with Wayland compositor
    ///
    /// Wayland-specific state synchronization using Wayland protocols.
    pub fn sync_window_state(&mut self) {
        use azul_core::window::WindowFrame;

        // Note: Wayland state changes must be committed
        let mut needs_commit = false;

        // Sync title
        if let Some(prev) = &self.previous_window_state {
            if prev.title != self.current_window_state.title {
                let c_title = match std::ffi::CString::new(self.current_window_state.title.as_str())
                {
                    Ok(s) => s,
                    Err(_) => return,
                };
                unsafe {
                    (self.wayland.xdg_toplevel_set_title)(self.xdg_toplevel, c_title.as_ptr());
                }
                needs_commit = true;
            }

            // Window frame state changed? (Minimize/Maximize/Normal)
            if prev.flags.frame != self.current_window_state.flags.frame {
                match self.current_window_state.flags.frame {
                    WindowFrame::Minimized => {
                        // Wayland: Request minimize
                        unsafe {
                            (self.wayland.xdg_toplevel_set_minimized)(self.xdg_toplevel);
                        }
                    }
                    WindowFrame::Maximized => {
                        // Wayland: Request maximize
                        unsafe {
                            (self.wayland.xdg_toplevel_set_maximized)(self.xdg_toplevel);
                        }
                    }
                    WindowFrame::Normal | WindowFrame::Fullscreen => {
                        // Wayland: Restore (unset maximize)
                        if prev.flags.frame == WindowFrame::Maximized {
                            unsafe {
                                (self.wayland.xdg_toplevel_unset_maximized)(self.xdg_toplevel);
                            }
                        }
                    }
                }
                needs_commit = true;
            }
        }

        // Note: Wayland doesn't support direct position control
        // The compositor decides window placement

        // Sync visibility
        // TODO: Wayland visibility control via xdg_toplevel methods

        // Commit changes if needed
        if needs_commit {
            unsafe {
                (self.wayland.wl_surface_commit)(self.surface);
            }
        }
    }

    /// Generate frame if needed and reset flag
    ///
    /// This method implements the Wayland frame callback pattern for VSync:
    /// 1. Render to WebRender
    /// 2. Swap buffers (if GPU mode)
    /// 3. Set up frame callback for next frame
    pub fn generate_frame_if_needed(&mut self) {
        if !self.frame_needs_regeneration || self.frame_callback_pending {
            return;
        }

        match &mut self.render_mode {
            RenderMode::Gpu(gl_context, _) => {
                // 1. Wait for WebRender to be ready
                if let Some(renderer) = &mut self.renderer {
                    let (lock, cvar) = &*self.new_frame_ready;
                    let mut ready = lock.lock().unwrap();

                    // Non-blocking check - don't wait if not ready
                    if !*ready {
                        return;
                    }
                    *ready = false;
                    drop(ready); // Release lock before rendering

                    // 2. Update and render
                    renderer.update();
                    let physical_size = self.current_window_state.size.get_physical_size();
                    let device_size = webrender::api::units::DeviceIntSize::new(
                        physical_size.width as i32,
                        physical_size.height as i32,
                    );
                    if let Err(e) = renderer.render(device_size, 0) {
                        eprintln!("[Wayland] WebRender render failed: {:?}", e);
                        return;
                    }

                    // 3. Swap buffers
                    if let Err(e) = gl_context.swap_buffers() {
                        eprintln!("[Wayland] Swap buffers failed: {:?}", e);
                        return;
                    }
                }
            }
            RenderMode::Cpu(Some(cpu_state)) => {
                // CPU rendering - draw to shared memory buffer
                cpu_state.draw_blue();
                unsafe {
                    (self.wayland.wl_surface_attach)(self.surface, cpu_state.buffer, 0, 0);
                    (self.wayland.wl_surface_damage)(
                        self.surface,
                        0,
                        0,
                        cpu_state.width,
                        cpu_state.height,
                    );
                }
            }
            RenderMode::Cpu(None) => {
                // CPU fallback not yet initialized - initialize it now if we have shm
                if !self.shm.is_null() {
                    let width = self.current_window_state.size.dimensions.width as i32;
                    let height = self.current_window_state.size.dimensions.height as i32;
                    match CpuFallbackState::new(&self.wayland, self.shm, width, height) {
                        Ok(cpu_state) => {
                            self.render_mode = RenderMode::Cpu(Some(cpu_state));
                            eprintln!("[Wayland] CPU fallback initialized: {}x{}", width, height);
                        }
                        Err(e) => {
                            eprintln!("[Wayland] Failed to initialize CPU fallback: {:?}", e);
                        }
                    }
                }
            }
        }

        // 4. Set up frame callback for next frame (VSync)
        unsafe {
            let frame_callback = (self.wayland.wl_surface_frame)(self.surface);
            let listener = defines::wl_callback_listener {
                done: frame_done_callback,
            };
            (self.wayland.wl_callback_add_listener)(
                frame_callback,
                &listener,
                self as *mut _ as *mut _,
            );
            (self.wayland.wl_surface_commit)(self.surface);
        }

        self.frame_needs_regeneration = false;
        self.frame_callback_pending = true;
    }
}

/// Wayland frame callback - called when compositor is ready for next frame
extern "C" fn frame_done_callback(
    data: *mut std::ffi::c_void,
    _callback: *mut defines::wl_callback,
    _callback_data: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.frame_callback_pending = false;

    // If there are more changes pending, request another frame
    if window.frame_needs_regeneration {
        window.generate_frame_if_needed();
    }
}

impl Drop for WaylandWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.xdg_toplevel.is_null() {
                (self.wayland.wl_proxy_destroy)(self.xdg_toplevel as _);
            }
            if !self.xdg_surface.is_null() {
                (self.wayland.wl_proxy_destroy)(self.xdg_surface as _);
            }
            if !self.surface.is_null() {
                (self.wayland.wl_proxy_destroy)(self.surface as _);
            }
            if !self.event_queue.is_null() {
                (self.wayland.wl_event_queue_destroy)(self.event_queue);
            }
            if !self.display.is_null() {
                (self.wayland.wl_display_disconnect)(self.display);
            }
        }
    }
}

impl CpuFallbackState {
    fn new(
        wayland: &Rc<Wayland>,
        shm: *mut wl_shm,
        width: i32,
        height: i32,
    ) -> Result<Self, WindowError> {
        let stride = width * 4;
        let size = stride * height;

        let fd = unsafe {
            libc::memfd_create(CString::new("azul-fb").unwrap().as_ptr(), libc::MFD_CLOEXEC)
        };
        if fd == -1 {
            return Err(WindowError::PlatformError("memfd_create failed".into()));
        }

        if unsafe { libc::ftruncate(fd, size as libc::off_t) } == -1 {
            unsafe { libc::close(fd) };
            return Err(WindowError::PlatformError("ftruncate failed".into()));
        }

        let data = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        // The fd can be closed after mmap as it's now managed by the kernel
        unsafe { libc::close(fd) };

        if data == libc::MAP_FAILED {
            return Err(WindowError::PlatformError("mmap failed".into()));
        }

        let pool = unsafe { (wayland.wl_shm_create_pool)(shm, fd, size) };
        let buffer = unsafe {
            (wayland.wl_shm_pool_create_buffer)(
                pool,
                0,
                width,
                height,
                stride,
                WL_SHM_FORMAT_ARGB8888,
            )
        };

        Ok(Self {
            wayland: wayland.clone(),
            pool,
            buffer,
            data: data as *mut u8,
            width,
            height,
            stride,
        })
    }

    fn draw_blue(&self) {
        let size = (self.stride * self.height) as usize;
        let slice = unsafe { std::slice::from_raw_parts_mut(self.data, size) };
        for chunk in slice.chunks_exact_mut(4) {
            chunk[0] = 0xFF; // Blue
            chunk[1] = 0x00; // Green
            chunk[2] = 0x00; // Red
            chunk[3] = 0xFF; // Alpha (ARGB format)
        }
    }
}

impl Drop for CpuFallbackState {
    fn drop(&mut self) {
        unsafe {
            if !self.buffer.is_null() {
                (self.wayland.wl_buffer_destroy)(self.buffer);
            }
            if !self.pool.is_null() {
                (self.wayland.wl_shm_pool_destroy)(self.pool);
            }
            if !self.data.is_null() {
                libc::munmap(self.data as *mut _, (self.stride * self.height) as usize);
            }
        }
    }
}

// Helper methods for WaylandWindow to get display information
impl WaylandWindow {
    /// Returns the logical size of the window's surface.
    pub fn get_window_size_logical(&self) -> (i32, i32) {
        let size = self.current_window_state.size.get_logical_size();
        (size.width as i32, size.height as i32)
    }

    /// Returns the physical size of the window by applying the scale factor.
    pub fn get_window_size_physical(&self) -> (i32, i32) {
        let size = self.current_window_state.size.get_physical_size();
        (size.width as i32, size.height as i32)
    }

    /// Returns the DPI scale factor for the window.
    pub fn get_scale_factor(&self) -> f32 {
        self.current_window_state
            .size
            .get_hidpi_factor()
            .inner
            .get()
    }

    /// Get display information for Wayland
    ///
    /// Note: Wayland doesn't expose absolute positioning information to clients.
    /// This returns an approximation based on the window's size and scale.
    pub fn get_window_display_info(&self) -> Option<crate::desktop::display::DisplayInfo> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let scale_factor = self.get_scale_factor();

        // Use actual window size if available, otherwise reasonable defaults
        let (width, height) = if self.current_window_state.size.dimensions.width > 0.0
            && self.current_window_state.size.dimensions.height > 0.0
        {
            // Use window dimensions as a proxy for display size
            // This is not accurate for multi-monitor setups, but Wayland doesn't
            // provide absolute display enumeration to clients
            (
                self.current_window_state.size.dimensions.width as i32,
                self.current_window_state.size.dimensions.height as i32,
            )
        } else {
            // Fallback to environment variables or defaults
            let width = std::env::var("WAYLAND_DISPLAY_WIDTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1920);
            let height = std::env::var("WAYLAND_DISPLAY_HEIGHT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1080);
            (width, height)
        };

        let bounds = LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width as f32, height as f32),
        );

        let work_area = LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width as f32, (height as i32 - 24).max(0) as f32),
        );

        Some(crate::desktop::display::DisplayInfo {
            name: "wayland-0".to_string(),
            bounds,
            work_area,
            scale_factor,
            is_primary: true,
        })
    }
}

// ============================================================================
// WaylandPopup Implementation
// ============================================================================

impl WaylandPopup {
    /// Create a new popup window using xdg_popup protocol
    ///
    /// This creates a popup surface that is properly managed by the Wayland compositor.
    /// The popup will be positioned relative to the parent window using xdg_positioner.
    ///
    /// # Arguments
    /// * `parent` - Parent WaylandWindow
    /// * `anchor_rect` - Rectangle on parent surface where popup is anchored (logical coords)
    /// * `popup_size` - Size of popup window (logical coords)
    /// * `options` - Window creation options (for rendering setup)
    ///
    /// # Returns
    /// * `Ok(WaylandPopup)` - Successfully created popup
    /// * `Err(String)` - Error message
    pub fn new(
        parent: &WaylandWindow,
        anchor_rect: azul_core::geom::LogicalRect,
        popup_size: azul_core::geom::LogicalSize,
        options: WindowCreateOptions,
    ) -> Result<Self, String> {
        use crate::desktop::shell2::linux::wayland::defines::*;

        let wayland = parent.wayland.clone();
        let xkb = parent.xkb.clone();

        // 1. Create xdg_positioner
        let positioner = unsafe { (wayland.xdg_wm_base_create_positioner)(parent.xdg_wm_base) };

        if positioner.is_null() {
            return Err("Failed to create xdg_positioner".to_string());
        }

        // 2. Configure positioner
        unsafe {
            // Set popup size
            (wayland.xdg_positioner_set_size)(
                positioner,
                popup_size.width as i32,
                popup_size.height as i32,
            );

            // Set anchor rectangle (where popup is triggered from on parent surface)
            (wayland.xdg_positioner_set_anchor_rect)(
                positioner,
                anchor_rect.origin.x as i32,
                anchor_rect.origin.y as i32,
                anchor_rect.size.width as i32,
                anchor_rect.size.height as i32,
            );

            // Anchor to bottom-right corner of anchor rect
            (wayland.xdg_positioner_set_anchor)(positioner, XDG_POSITIONER_ANCHOR_BOTTOM_RIGHT);

            // Popup grows down and right from anchor point
            (wayland.xdg_positioner_set_gravity)(positioner, XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT);

            // Allow compositor to flip/slide if popup would overflow screen
            (wayland.xdg_positioner_set_constraint_adjustment)(
                positioner,
                XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_X
                    | XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_Y
                    | XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_X
                    | XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_Y,
            );
        }

        // 3. Create wl_surface
        let surface = unsafe { (wayland.wl_compositor_create_surface)(parent.compositor) };

        if surface.is_null() {
            unsafe {
                (wayland.xdg_positioner_destroy)(positioner);
            }
            return Err("Failed to create wl_surface for popup".to_string());
        }

        // 4. Create xdg_surface
        let xdg_surface =
            unsafe { (wayland.xdg_wm_base_get_xdg_surface)(parent.xdg_wm_base, surface) };

        if xdg_surface.is_null() {
            unsafe {
                (wayland.wl_proxy_destroy)(surface as *mut _);
                (wayland.xdg_positioner_destroy)(positioner);
            }
            return Err("Failed to create xdg_surface for popup".to_string());
        }

        // 5. Get xdg_popup role
        let xdg_popup = unsafe {
            (wayland.xdg_surface_get_popup)(
                xdg_surface,
                parent.xdg_surface, // Parent xdg_surface
                positioner,
            )
        };

        if xdg_popup.is_null() {
            unsafe {
                (wayland.wl_proxy_destroy)(xdg_surface as *mut _);
                (wayland.wl_proxy_destroy)(surface as *mut _);
                (wayland.xdg_positioner_destroy)(positioner);
            }
            return Err("Failed to create xdg_popup".to_string());
        }

        // 6. Add xdg_surface listener (configure events)
        let xdg_surface_listener = xdg_surface_listener {
            configure: popup_xdg_surface_configure,
        };

        unsafe {
            (wayland.xdg_surface_add_listener)(
                xdg_surface,
                &xdg_surface_listener,
                std::ptr::null_mut(),
            );
        }

        // 7. Add xdg_popup listener
        let popup_listener = xdg_popup_listener {
            configure: popup_configure,
            popup_done,
        };

        unsafe {
            (wayland.xdg_popup_add_listener)(xdg_popup, &popup_listener, std::ptr::null_mut());
        }

        // 8. Grab pointer for exclusive input (using parent's last serial)
        unsafe {
            (wayland.xdg_popup_grab)(xdg_popup, parent.seat, parent.pointer_state.serial);
        }

        // 9. Commit surface to make popup visible
        unsafe {
            (wayland.wl_surface_commit)(surface);
        }

        // 10. Create window state
        let current_window_state = FullWindowState {
            title: "Popup".to_string().into(),
            size: options.state.size,
            position: parent.current_window_state.position,
            flags: parent.current_window_state.flags,
            theme: parent.current_window_state.theme,
            debug_state: parent.current_window_state.debug_state,
            keyboard_state: parent.current_window_state.keyboard_state.clone(),
            mouse_state: parent.current_window_state.mouse_state.clone(),
            touch_state: parent.current_window_state.touch_state.clone(),
            ime_position: parent.current_window_state.ime_position,
            platform_specific_options: parent
                .current_window_state
                .platform_specific_options
                .clone(),
            renderer_options: parent.current_window_state.renderer_options,
            background_color: parent.current_window_state.background_color,
            layout_callback: options.state.layout_callback.clone(),
            close_callback: options.state.close_callback.clone(),
            monitor: parent.current_window_state.monitor.clone(),
            last_hit_test: FullHitTest::empty(None),
            focused_node: None,
            hovered_file: None,
            dropped_file: None,
            selections: Default::default(),
            window_focused: false,
        };

        Ok(Self {
            wayland,
            xkb,
            display: parent.display,
            parent_surface: parent.surface,
            surface,
            xdg_surface,
            xdg_popup,
            positioner,
            compositor: parent.compositor,
            seat: parent.seat,
            event_queue: parent.event_queue,
            keyboard_state: events::KeyboardState::new(),
            pointer_state: events::PointerState::new(),
            is_open: true,
            configured: false,

            layout_window: None,
            current_window_state,
            previous_window_state: None,
            render_api: None,
            renderer: None,
            hit_tester: None,
            document_id: None,
            image_cache: ImageCache::default(),
            renderer_resources: RendererResources::default(),
            gl_context_ptr: OptionGlContextPtr::None,
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            id_namespace: None,
            render_mode: RenderMode::Cpu(None),

            scrollbar_drag_state: None,
            last_hovered_node: None,
            frame_needs_regeneration: true,
            frame_callback_pending: false,

            resources: parent.resources.clone(),
            fc_cache: parent.fc_cache.clone(),
            app_data: parent.app_data.clone(),
        })
    }

    /// Close the popup window
    pub fn close(&mut self) {
        if self.is_open {
            unsafe {
                if !self.xdg_popup.is_null() {
                    (self.wayland.xdg_popup_destroy)(self.xdg_popup);
                    self.xdg_popup = std::ptr::null_mut();
                }

                if !self.xdg_surface.is_null() {
                    (self.wayland.wl_proxy_destroy)(self.xdg_surface as *mut _);
                    self.xdg_surface = std::ptr::null_mut();
                }

                if !self.surface.is_null() {
                    (self.wayland.wl_proxy_destroy)(self.surface as *mut _);
                    self.surface = std::ptr::null_mut();
                }

                if !self.positioner.is_null() {
                    (self.wayland.xdg_positioner_destroy)(self.positioner);
                    self.positioner = std::ptr::null_mut();
                }
            }

            self.is_open = false;
        }
    }
}

impl Drop for WaylandPopup {
    fn drop(&mut self) {
        self.close();
    }
}

// ============================================================================
// XDG Popup Listener Callbacks
// ============================================================================

/// xdg_surface configure callback for popup
extern "C" fn popup_xdg_surface_configure(
    _data: *mut c_void,
    xdg_surface: *mut defines::xdg_surface,
    serial: u32,
) {
    // Must acknowledge configure
    unsafe {
        // Note: We need to get wayland instance from somewhere
        // For now, use dlsym to get the function directly
        type AckFn = unsafe extern "C" fn(*mut defines::xdg_surface, u32);
        let lib = libc::dlopen(
            b"libwayland-client.so.0\0".as_ptr() as *const i8,
            libc::RTLD_LAZY,
        );
        if !lib.is_null() {
            let ack_fn = libc::dlsym(lib, b"xdg_surface_ack_configure\0".as_ptr() as *const i8);
            if !ack_fn.is_null() {
                let ack: AckFn = std::mem::transmute(ack_fn);
                ack(xdg_surface, serial);
            }
            libc::dlclose(lib);
        }
    }
}

/// xdg_popup configure callback
extern "C" fn popup_configure(
    _data: *mut c_void,
    _xdg_popup: *mut defines::xdg_popup,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    eprintln!(
        "[xdg_popup] configure: x={}, y={}, width={}, height={}",
        x, y, width, height
    );
    // Compositor has positioned the popup
    // We could resize the popup here if needed
}

/// xdg_popup done callback - popup was dismissed by compositor
extern "C" fn popup_done(_data: *mut c_void, _xdg_popup: *mut defines::xdg_popup) {
    eprintln!("[xdg_popup] popup_done: compositor dismissed popup");
    // Popup should be closed
    // TODO: Signal to application that popup was dismissed
}
