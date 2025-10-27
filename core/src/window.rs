#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use core::{
    cmp::Ordering,
    ffi::c_void,
    hash::{Hash, Hasher},
    ops,
    sync::atomic::{AtomicI64, AtomicUsize, Ordering as AtomicOrdering},
};

use azul_css::{
    css::CssPath,
    props::{
        basic::{ColorU, FloatValue, LayoutPoint, LayoutRect, LayoutSize},
        property::CssProperty,
        style::StyleCursor,
    },
    AzString, LayoutDebugMessage, OptionAzString, OptionF32, OptionI32, U8Vec,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::{LayoutCallback, LayoutCallbackType, Update},
    dom::{DomId, DomNodeId, NodeHierarchy},
    geom::{LogicalPosition, LogicalSize, OptionLogicalSize, PhysicalPositionI32, PhysicalSize},
    gl::OptionGlContextPtr,
    hit_test::{ExternalScrollId, OverflowingScrollNode},
    id::{NodeDataContainer, NodeId},
    resources::{
        DpiScaleFactor, Epoch, GlTextureCache, IdNamespace, ImageCache, ImageMask, ImageRef,
        RendererResources, ResourceUpdate,
    },
    selection::SelectionState,
    styled_dom::NodeHierarchyItemId,
    task::{Instant, ThreadId, TimerId},
    FastBTreeSet, FastHashMap,
};

pub const DEFAULT_TITLE: &str = "Azul App";

static LAST_WINDOW_ID: AtomicI64 = AtomicI64::new(0);

/// Each default callback is identified by its ID (not by it's function pointer),
/// since multiple IDs could point to the same function.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
pub struct WindowId {
    pub id: i64,
}

impl WindowId {
    pub fn new() -> Self {
        WindowId {
            id: LAST_WINDOW_ID.fetch_add(1, AtomicOrdering::SeqCst),
        }
    }
}

static LAST_ICON_KEY: AtomicUsize = AtomicUsize::new(0);

/// Key that is used for checking whether a window icon has changed -
/// this way azul doesn't need to diff the actual bytes, just the icon key.
/// Use `IconKey::new()` to generate a new, unique key
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct IconKey {
    id: usize,
}

impl IconKey {
    pub fn new() -> Self {
        Self {
            id: LAST_ICON_KEY.fetch_add(1, AtomicOrdering::SeqCst),
        }
    }
}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub struct RendererOptions {
    pub vsync: Vsync,
    pub srgb: Srgb,
    pub hw_accel: HwAcceleration,
}

impl_option!(
    RendererOptions,
    OptionRendererOptions,
    [PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash]
);

impl Default for RendererOptions {
    fn default() -> Self {
        Self {
            vsync: Vsync::Enabled,
            srgb: Srgb::Disabled,
            hw_accel: HwAcceleration::Enabled,
        }
    }
}

impl RendererOptions {
    pub const fn new(vsync: Vsync, srgb: Srgb, hw_accel: HwAcceleration) -> Self {
        Self {
            vsync,
            srgb,
            hw_accel,
        }
    }
}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub enum Vsync {
    Enabled,
    Disabled,
    DontCare,
}
impl Vsync {
    pub const fn is_enabled(&self) -> bool {
        match self {
            Vsync::Enabled => true,
            _ => false,
        }
    }
}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub enum Srgb {
    Enabled,
    Disabled,
    DontCare,
}
impl Srgb {
    pub const fn is_enabled(&self) -> bool {
        match self {
            Srgb::Enabled => true,
            _ => false,
        }
    }
}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub enum HwAcceleration {
    Enabled,
    Disabled,
    DontCare,
}
impl HwAcceleration {
    pub const fn is_enabled(&self) -> bool {
        match self {
            HwAcceleration::Enabled => true,
            _ => false,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum RawWindowHandle {
    IOS(IOSHandle),
    MacOS(MacOSHandle),
    Xlib(XlibHandle),
    Xcb(XcbHandle),
    Wayland(WaylandHandle),
    Windows(WindowsHandle),
    Web(WebHandle),
    Android(AndroidHandle),
    Unsupported,
}

unsafe impl Send for RawWindowHandle {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IOSHandle {
    pub ui_window: *mut c_void,
    pub ui_view: *mut c_void,
    pub ui_view_controller: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct MacOSHandle {
    pub ns_window: *mut c_void,
    pub ns_view: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XlibHandle {
    /// An Xlib Window
    pub window: u64,
    pub display: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XcbHandle {
    /// An X11 xcb_window_t.
    pub window: u32,
    /// A pointer to an X server xcb_connection_t.
    pub connection: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WaylandHandle {
    /// A pointer to a wl_surface
    pub surface: *mut c_void,
    /// A pointer to a wl_display.
    pub display: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WindowsHandle {
    /// A Win32 HWND handle.
    pub hwnd: *mut c_void,
    /// The HINSTANCE associated with this type's HWND.
    pub hinstance: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WebHandle {
    /// An ID value inserted into the data attributes of the canvas element as 'raw-handle'
    ///
    /// When accessing from JS, the attribute will automatically be called rawHandle. Each canvas
    /// created by the windowing system should be assigned their own unique ID.
    /// 0 should be reserved for invalid / null IDs.
    pub id: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AndroidHandle {
    /// A pointer to an ANativeWindow.
    pub a_native_window: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum MouseCursorType {
    Default,
    Crosshair,
    Hand,
    Arrow,
    Move,
    Text,
    Wait,
    Help,
    Progress,
    NotAllowed,
    ContextMenu,
    Cell,
    VerticalText,
    Alias,
    Copy,
    NoDrop,
    Grab,
    Grabbing,
    AllScroll,
    ZoomIn,
    ZoomOut,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
}

impl Default for MouseCursorType {
    fn default() -> Self {
        MouseCursorType::Default
    }
}

/// Hardware-dependent keyboard scan code.
pub type ScanCode = u32;

/// Determines which keys are pressed currently (modifiers, etc.)
#[derive(Default, Debug, Clone, PartialEq)]
#[repr(C)]
pub struct KeyboardState {
    /// Currently pressed key, already converted to a `char` - (READONLY)
    pub current_char: OptionChar,
    /// Same as `current_char`, but .
    ///
    /// **DO NOT USE THIS FOR TEXT INPUT, USE `current_char` and `On::TextInput` instead.**
    /// For example entering `à` will fire a `VirtualKeyCode::Grave`, then `VirtualKeyCode::A`,
    /// so to correctly combine characters, use the `current_char` field.
    pub current_virtual_keycode: OptionVirtualKeyCode,
    /// Currently pressed virtual keycodes (READONLY) - it can happen that more t
    ///
    /// This is essentially an "extension" of `current_scancodes` - `current_keys` stores the
    /// characters, but what if the pressed key is not a character (such as `ArrowRight` or
    /// `PgUp`)?
    ///
    /// Note that this can have an overlap, so pressing "a" on the keyboard will insert
    /// both a `VirtualKeyCode::A` into `current_virtual_keycodes` and an `"a"` as a char into
    /// `current_keys`.
    pub pressed_virtual_keycodes: VirtualKeyCodeVec,
    /// Same as `current_virtual_keycodes`, but the scancode identifies the physical key pressed,
    /// independent of the keyboard layout. The scancode does not change if the user adjusts the
    /// host's keyboard map. Use when the physical location of the key is more important than
    /// the key's host GUI semantics, such as for movement controls in a first-person game
    /// (German keyboard: Z key, UK keyboard: Y key, etc.)
    pub pressed_scancodes: ScanCodeVec,
}

impl KeyboardState {
    pub fn shift_down(&self) -> bool {
        self.is_key_down(VirtualKeyCode::LShift) || self.is_key_down(VirtualKeyCode::RShift)
    }
    pub fn ctrl_down(&self) -> bool {
        self.is_key_down(VirtualKeyCode::LControl) || self.is_key_down(VirtualKeyCode::RControl)
    }
    pub fn alt_down(&self) -> bool {
        self.is_key_down(VirtualKeyCode::LAlt) || self.is_key_down(VirtualKeyCode::RAlt)
    }
    pub fn super_down(&self) -> bool {
        self.is_key_down(VirtualKeyCode::LWin) || self.is_key_down(VirtualKeyCode::RWin)
    }
    pub fn is_key_down(&self, key: VirtualKeyCode) -> bool {
        self.pressed_virtual_keycodes.iter().any(|k| *k == key)
    }
}

impl_option!(
    KeyboardState,
    OptionKeyboardState,
    copy = false,
    [Debug, Clone, PartialEq]
);

// char is not ABI-stable, use u32 instead
impl_option!(
    u32,
    OptionChar,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    VirtualKeyCode,
    OptionVirtualKeyCode,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    VirtualKeyCode,
    VirtualKeyCodeVec,
    VirtualKeyCodeVecDestructor
);
impl_vec_debug!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_partialord!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_ord!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_clone!(
    VirtualKeyCode,
    VirtualKeyCodeVec,
    VirtualKeyCodeVecDestructor
);
impl_vec_partialeq!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_eq!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_hash!(VirtualKeyCode, VirtualKeyCodeVec);

impl_vec_as_hashmap!(VirtualKeyCode, VirtualKeyCodeVec);

impl_vec!(ScanCode, ScanCodeVec, ScanCodeVecDestructor);
impl_vec_debug!(ScanCode, ScanCodeVec);
impl_vec_partialord!(ScanCode, ScanCodeVec);
impl_vec_ord!(ScanCode, ScanCodeVec);
impl_vec_clone!(ScanCode, ScanCodeVec, ScanCodeVecDestructor);
impl_vec_partialeq!(ScanCode, ScanCodeVec);
impl_vec_eq!(ScanCode, ScanCodeVec);
impl_vec_hash!(ScanCode, ScanCodeVec);

impl_vec_as_hashmap!(ScanCode, ScanCodeVec);

/// Mouse position, cursor type, user scroll input, etc.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct MouseState {
    /// Current mouse cursor type, set to `None` if the cursor is hidden. (READWRITE)
    pub mouse_cursor_type: OptionMouseCursorType,
    /// Where is the mouse cursor currently? Set to `None` if the window is not focused.
    /// (READWRITE)
    pub cursor_position: CursorPosition,
    /// Is the mouse cursor locked to the current window (important for applications like games)?
    /// (READWRITE)
    pub is_cursor_locked: bool,
    /// Is the left mouse button down? (READONLY)
    pub left_down: bool,
    /// Is the right mouse button down? (READONLY)
    pub right_down: bool,
    /// Is the middle mouse button down? (READONLY)
    pub middle_down: bool,
    /// Scroll amount in pixels in the horizontal direction. Gets reset to 0 after every frame
    /// (READONLY)
    pub scroll_x: OptionF32,
    /// Scroll amount in pixels in the vertical direction. Gets reset to 0 after every frame
    /// (READONLY)
    pub scroll_y: OptionF32,
}

impl MouseState {
    pub fn matches(&self, context: &ContextMenuMouseButton) -> bool {
        use self::ContextMenuMouseButton::*;
        match context {
            Left => self.left_down,
            Right => self.right_down,
            Middle => self.middle_down,
        }
    }
}

impl_option!(
    MouseState,
    OptionMouseState,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl_option!(
    MouseCursorType,
    OptionMouseCursorType,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl Default for MouseState {
    fn default() -> Self {
        Self {
            mouse_cursor_type: Some(MouseCursorType::Default).into(),
            cursor_position: CursorPosition::default(),
            is_cursor_locked: false,
            left_down: false,
            right_down: false,
            middle_down: false,
            scroll_x: None.into(),
            scroll_y: None.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct VirtualKeyCodeCombo {
    pub keys: VirtualKeyCodeVec,
}

impl_option!(
    VirtualKeyCodeCombo,
    OptionVirtualKeyCodeCombo,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum ContextMenuMouseButton {
    Right,
    Middle,
    Left,
}

impl Default for ContextMenuMouseButton {
    fn default() -> Self {
        ContextMenuMouseButton::Right
    }
}

impl MouseState {
    /// Returns whether any mouse button (left, right or center) is currently held down
    pub fn mouse_down(&self) -> bool {
        self.right_down || self.left_down || self.middle_down
    }

    pub fn get_scroll_x(&self) -> f32 {
        self.scroll_x.as_option().copied().unwrap_or(0.0)
    }

    pub fn get_scroll_y(&self) -> f32 {
        self.scroll_y.as_option().copied().unwrap_or(0.0)
    }

    pub fn get_scroll(&self) -> (f32, f32) {
        (self.get_scroll_x(), self.get_scroll_y())
    }

    pub fn get_scroll_amount(&self) -> Option<(f32, f32)> {
        const SCROLL_THRESHOLD: f32 = 0.5; // px

        if self.scroll_x.is_none() && self.scroll_y.is_none() {
            return None;
        }

        let scroll_x = self.get_scroll_x();
        let scroll_y = self.get_scroll_y();

        if libm::fabsf(scroll_x) < SCROLL_THRESHOLD && libm::fabsf(scroll_y) < SCROLL_THRESHOLD {
            return None;
        }

        Some((scroll_x, scroll_y))
    }

    /// Function reset the `scroll_x` and `scroll_y` to `None` to clear the scroll amount
    pub fn reset_scroll_to_zero(&mut self) {
        self.scroll_x = OptionF32::None;
        self.scroll_y = OptionF32::None;
    }
}

// TODO: returned by process_system_scroll
#[derive(Debug)]
pub struct ScrollResult {}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum CursorPosition {
    OutOfWindow(LogicalPosition),
    Uninitialized,
    InWindow(LogicalPosition),
}

impl Default for CursorPosition {
    fn default() -> CursorPosition {
        CursorPosition::Uninitialized
    }
}

impl CursorPosition {
    pub fn get_position(&self) -> Option<LogicalPosition> {
        match self {
            CursorPosition::InWindow(logical_pos) => Some(*logical_pos),
            CursorPosition::OutOfWindow(_) | CursorPosition::Uninitialized => None,
        }
    }

    pub fn is_inside_window(&self) -> bool {
        self.get_position().is_some()
    }
}

/// Toggles webrender debug flags (will make stuff appear on
/// the screen that you might not want to - used for debugging purposes)
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct DebugState {
    pub profiler_dbg: bool,
    pub render_target_dbg: bool,
    pub texture_cache_dbg: bool,
    pub gpu_time_queries: bool,
    pub gpu_sample_queries: bool,
    pub disable_batching: bool,
    pub epochs: bool,
    pub echo_driver_messages: bool,
    pub show_overdraw: bool,
    pub gpu_cache_dbg: bool,
    pub texture_cache_dbg_clear_evicted: bool,
    pub picture_caching_dbg: bool,
    pub primitive_dbg: bool,
    pub zoom_dbg: bool,
    pub small_screen: bool,
    pub disable_opaque_pass: bool,
    pub disable_alpha_pass: bool,
    pub disable_clip_masks: bool,
    pub disable_text_prims: bool,
    pub disable_gradient_prims: bool,
    pub obscure_images: bool,
    pub glyph_flashing: bool,
    pub smart_profiler: bool,
    pub invalidation_dbg: bool,
    pub tile_cache_logging_dbg: bool,
    pub profiler_capture: bool,
    pub force_picture_invalidation: bool,
}

fn translate_cursor(cursor: StyleCursor) -> MouseCursorType {
    use azul_css::props::style::effects::StyleCursor;
    match cursor {
        StyleCursor::Default => MouseCursorType::Default,
        StyleCursor::Crosshair => MouseCursorType::Crosshair,
        StyleCursor::Pointer => MouseCursorType::Hand,
        StyleCursor::Move => MouseCursorType::Move,
        StyleCursor::Text => MouseCursorType::Text,
        StyleCursor::Wait => MouseCursorType::Wait,
        StyleCursor::Help => MouseCursorType::Help,
        StyleCursor::Progress => MouseCursorType::Progress,
        StyleCursor::ContextMenu => MouseCursorType::ContextMenu,
        StyleCursor::Cell => MouseCursorType::Cell,
        StyleCursor::VerticalText => MouseCursorType::VerticalText,
        StyleCursor::Alias => MouseCursorType::Alias,
        StyleCursor::Copy => MouseCursorType::Copy,
        StyleCursor::Grab => MouseCursorType::Grab,
        StyleCursor::Grabbing => MouseCursorType::Grabbing,
        StyleCursor::AllScroll => MouseCursorType::AllScroll,
        StyleCursor::ZoomIn => MouseCursorType::ZoomIn,
        StyleCursor::ZoomOut => MouseCursorType::ZoomOut,
        StyleCursor::EResize => MouseCursorType::EResize,
        StyleCursor::NResize => MouseCursorType::NResize,
        StyleCursor::SResize => MouseCursorType::SResize,
        StyleCursor::SeResize => MouseCursorType::SeResize,
        StyleCursor::WResize => MouseCursorType::WResize,
        StyleCursor::EwResize => MouseCursorType::EwResize,
        StyleCursor::NsResize => MouseCursorType::NsResize,
        StyleCursor::NeswResize => MouseCursorType::NeswResize,
        StyleCursor::NwseResize => MouseCursorType::NwseResize,
        StyleCursor::ColResize => MouseCursorType::ColResize,
        StyleCursor::RowResize => MouseCursorType::RowResize,
        StyleCursor::Unset => MouseCursorType::Default,
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
#[repr(C)]
pub struct TouchState {
    /// TODO: not yet implemented
    pub unimplemented: u8,
}

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
#[repr(C)]
pub enum WindowTheme {
    DarkMode,
    LightMode,
}

impl Default for WindowTheme {
    fn default() -> WindowTheme {
        WindowTheme::LightMode // sorry!
    }
}

impl_option!(
    WindowTheme,
    OptionWindowTheme,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]
);

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Monitor {
    pub id: usize,
    pub name: OptionAzString,
    pub size: LayoutSize,
    pub position: LayoutPoint,
    pub scale_factor: f64,
    pub video_modes: VideoModeVec,
    pub is_primary_monitor: bool,
}

impl_vec!(Monitor, MonitorVec, MonitorVecDestructor);
impl_vec_debug!(Monitor, MonitorVec);
impl_vec_clone!(Monitor, MonitorVec, MonitorVecDestructor);
impl_vec_partialeq!(Monitor, MonitorVec);
impl_vec_partialord!(Monitor, MonitorVec);

impl core::hash::Hash for Monitor {
    fn hash<H>(&self, state: &mut H)
    where
        H: core::hash::Hasher,
    {
        self.id.hash(state)
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor {
            id: 0,
            name: OptionAzString::None,
            size: LayoutSize::zero(),
            position: LayoutPoint::zero(),
            scale_factor: 1.0,
            video_modes: Vec::new().into(),
            is_primary_monitor: false,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct VideoMode {
    pub size: LayoutSize,
    pub bit_depth: u16,
    pub refresh_rate: u16,
}

impl_vec!(VideoMode, VideoModeVec, VideoModeVecDestructor);
impl_vec_clone!(VideoMode, VideoModeVec, VideoModeVecDestructor);
impl_vec_debug!(VideoMode, VideoModeVec);
impl_vec_partialeq!(VideoMode, VideoModeVec);
impl_vec_partialord!(VideoMode, VideoModeVec);

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C, u8)]
pub enum WindowPosition {
    Uninitialized,
    Initialized(PhysicalPositionI32),
}

impl Default for WindowPosition {
    fn default() -> WindowPosition {
        WindowPosition::Uninitialized
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ImePosition {
    Uninitialized,
    Initialized(LogicalPosition),
}

impl Default for ImePosition {
    fn default() -> ImePosition {
        ImePosition::Uninitialized
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct WindowFlags {
    /// Is the window currently maximized, minimized or fullscreen
    pub frame: WindowFrame,
    /// User clicked the close button (set by WindowDelegate, checked by event loop)
    /// The close_callback can set this to false to prevent closing
    pub close_requested: bool,
    /// Window decoration style (title bar, native controls)
    pub decorations: WindowDecorations,
    /// Is the window currently visible?
    pub is_visible: bool,
    /// Is the window always on top?
    pub is_always_on_top: bool,
    /// Whether the window is resizable
    pub is_resizable: bool,
    /// Whether the window has focus or not (mutating this will request user attention)
    pub has_focus: bool,
    /// Compositor blur/transparency effect material
    pub background_material: WindowBackgroundMaterial,
    /// Is smooth scrolling enabled for this window?
    pub smooth_scroll_enabled: bool,
    /// Is automatic TAB switching supported?
    pub autotab_enabled: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub enum WindowFrame {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
}

/// Window decoration style
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub enum WindowDecorations {
    /// Full decorations: title bar with controls
    Normal,
    /// No title text but controls visible (extended frame)
    NoTitle,
    /// No controls visible but title bar area present
    NoControls,
    /// No decorations at all (borderless)
    None,
}

impl Default for WindowDecorations {
    fn default() -> Self {
        Self::Normal
    }
}

/// Compositor blur/transparency effects for window background
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub enum WindowBackgroundMaterial {
    /// No transparency or blur
    Opaque,
    /// Transparent without blur
    Transparent,
    /// macOS: Sidebar material, Windows: Acrylic light
    Sidebar,
    /// macOS: Menu material, Windows: Acrylic
    Menu,
    /// macOS: HUD material, Windows: Acrylic dark
    HUD,
    /// macOS: Titlebar material, Windows: Mica
    Titlebar,
    /// Windows: Mica Alt material
    MicaAlt,
}

impl Default for WindowBackgroundMaterial {
    fn default() -> Self {
        Self::Opaque
    }
}

impl Default for WindowFlags {
    fn default() -> Self {
        Self {
            frame: WindowFrame::Normal,
            close_requested: false,
            decorations: WindowDecorations::Normal,
            is_visible: true,
            is_always_on_top: false,
            is_resizable: true,
            has_focus: true,
            background_material: WindowBackgroundMaterial::Opaque,
            smooth_scroll_enabled: true,
            autotab_enabled: true,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PlatformSpecificOptions {
    pub windows_options: WindowsWindowOptions,
    pub linux_options: LinuxWindowOptions,
    pub mac_options: MacWindowOptions,
    pub wasm_options: WasmWindowOptions,
}

unsafe impl Sync for PlatformSpecificOptions {}
unsafe impl Send for PlatformSpecificOptions {}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct WindowsWindowOptions {
    /// STARTUP ONLY: Whether the window should allow drag + drop operations (default: true)
    pub allow_drag_and_drop: bool,
    /// STARTUP ONLY: Sets `WS_EX_NOREDIRECTIONBITMAP`
    pub no_redirection_bitmap: bool,
    /// STARTUP ONLY: Window icon (decoded bytes), appears at the top right corner of the window
    pub window_icon: OptionWindowIcon,
    /// READWRITE: Taskbar icon (decoded bytes), usually 256x256x4 bytes large (`ICON_BIG`).
    ///
    /// Can be changed in callbacks / at runtime.
    pub taskbar_icon: OptionTaskBarIcon,
    /// STARTUP ONLY: Pointer (casted to void pointer) to a HWND handle
    pub parent_window: OptionHwndHandle,
}

impl Default for WindowsWindowOptions {
    fn default() -> WindowsWindowOptions {
        WindowsWindowOptions {
            allow_drag_and_drop: true,
            no_redirection_bitmap: false,
            window_icon: OptionWindowIcon::None,
            taskbar_icon: OptionTaskBarIcon::None,
            parent_window: OptionHwndHandle::None,
        }
    }
}

/// Note: this should be a *mut HWND
type HwndHandle = *mut c_void;

impl_option!(
    HwndHandle,
    OptionHwndHandle,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// X window type. Maps directly to
/// [`_NET_WM_WINDOW_TYPE`](https://specifications.freedesktop.org/wm-spec/wm-spec-1.5.html).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum XWindowType {
    /// A desktop feature. This can include a single window containing desktop icons with the same
    /// dimensions as the screen, allowing the desktop environment to have full control of the
    /// desktop, without the need for proxying root window clicks.
    Desktop,
    /// A dock or panel feature. Typically a Window Manager would keep such windows on top of all
    /// other windows.
    Dock,
    /// Toolbar windows. "Torn off" from the main application.
    Toolbar,
    /// Pinnable menu windows. "Torn off" from the main application.
    Menu,
    /// A small persistent utility window, such as a palette or toolbox.
    Utility,
    /// The window is a splash screen displayed as an application is starting up.
    Splash,
    /// This is a dialog window.
    Dialog,
    /// A dropdown menu that usually appears when the user clicks on an item in a menu bar.
    /// This property is typically used on override-redirect windows.
    DropdownMenu,
    /// A popup menu that usually appears when the user right clicks on an object.
    /// This property is typically used on override-redirect windows.
    PopupMenu,
    /// A tooltip window. Usually used to show additional information when hovering over an object
    /// with the cursor. This property is typically used on override-redirect windows.
    Tooltip,
    /// The window is a notification.
    /// This property is typically used on override-redirect windows.
    Notification,
    /// This should be used on the windows that are popped up by combo boxes.
    /// This property is typically used on override-redirect windows.
    Combo,
    /// This indicates the the window is being dragged.
    /// This property is typically used on override-redirect windows.
    Dnd,
    /// This is a normal, top-level window.
    Normal,
}

impl Default for XWindowType {
    fn default() -> Self {
        XWindowType::Normal
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub enum UserAttentionType {
    None,
    Critical,
    Informational,
}

impl Default for UserAttentionType {
    fn default() -> UserAttentionType {
        UserAttentionType::None
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LinuxWindowOptions {
    /// (Unimplemented) - Can only be set at window creation, can't be changed in callbacks.
    pub x11_visual: OptionX11Visual,
    /// (Unimplemented) - Can only be set at window creation, can't be changed in callbacks.
    pub x11_screen: OptionI32,
    /// Build window with `WM_CLASS` hint; defaults to the name of the binary. Only relevant on
    /// X11. Can only be set at window creation, can't be changed in callbacks.
    pub x11_wm_classes: StringPairVec,
    /// Build window with override-redirect flag; defaults to false. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_override_redirect: bool,
    /// Build window with `_NET_WM_WINDOW_TYPE` hint; defaults to `Normal`. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_window_types: XWindowTypeVec,
    /// Build window with `_GTK_THEME_VARIANT` hint set to the specified value. Currently only
    /// relevant on X11. Can only be set at window creation, can't be changed in callbacks.
    pub x11_gtk_theme_variant: OptionAzString,
    /// Build window with resize increment hint. Only implemented on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_resize_increments: OptionLogicalSize,
    /// Build window with base size hint. Only implemented on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_base_size: OptionLogicalSize,
    /// Build window with a given application ID. It should match the `.desktop` file distributed
    /// with your program. Only relevant on Wayland.
    /// Can only be set at window creation, can't be changed in callbacks.
    ///
    /// For details about application ID conventions, see the
    /// [Desktop Entry Spec](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html#desktop-file-id)
    pub wayland_app_id: OptionAzString,
    pub wayland_theme: OptionWaylandTheme,
    pub request_user_attention: UserAttentionType,
    pub window_icon: OptionWindowIcon,
}

type X11Visual = *const c_void;
impl_option!(
    X11Visual,
    OptionX11Visual,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct AzStringPair {
    pub key: AzString,
    pub value: AzString,
}

impl_vec!(AzStringPair, StringPairVec, StringPairVecDestructor);
impl_vec_mut!(AzStringPair, StringPairVec);
impl_vec_debug!(AzStringPair, StringPairVec);
impl_vec_partialord!(AzStringPair, StringPairVec);
impl_vec_ord!(AzStringPair, StringPairVec);
impl_vec_clone!(AzStringPair, StringPairVec, StringPairVecDestructor);
impl_vec_partialeq!(AzStringPair, StringPairVec);
impl_vec_eq!(AzStringPair, StringPairVec);
impl_vec_hash!(AzStringPair, StringPairVec);

impl_option!(
    StringPairVec,
    OptionStringPairVec,
    copy = false,
    [Debug, Clone, PartialOrd, PartialEq, Ord, Eq, Hash]
);

impl StringPairVec {
    pub fn get_key(&self, search_key: &str) -> Option<&AzString> {
        self.as_ref().iter().find_map(|v| {
            if v.key.as_str() == search_key {
                Some(&v.value)
            } else {
                None
            }
        })
    }
    pub fn get_key_mut(&mut self, search_key: &str) -> Option<&mut AzStringPair> {
        self.as_mut()
            .iter_mut()
            .find(|v| v.key.as_str() == search_key)
    }
    pub fn insert_kv<I: Into<AzString>>(&mut self, key: I, value: I) {
        let key = key.into();
        let value = value.into();
        match self.get_key_mut(key.as_str()) {
            None => {}
            Some(s) => {
                s.value = value;
                return;
            }
        }
        self.push(AzStringPair { key, value });
    }
}

impl_vec!(XWindowType, XWindowTypeVec, XWindowTypeVecDestructor);
impl_vec_debug!(XWindowType, XWindowTypeVec);
impl_vec_partialord!(XWindowType, XWindowTypeVec);
impl_vec_ord!(XWindowType, XWindowTypeVec);
impl_vec_clone!(XWindowType, XWindowTypeVec, XWindowTypeVecDestructor);
impl_vec_partialeq!(XWindowType, XWindowTypeVec);
impl_vec_eq!(XWindowType, XWindowTypeVec);
impl_vec_hash!(XWindowType, XWindowTypeVec);

impl_option!(
    WaylandTheme,
    OptionWaylandTheme,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct MacWindowOptions {
    // empty for now, single field must be present for ABI compat - always set to 0
    pub _reserved: u8,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct WasmWindowOptions {
    // empty for now, single field must be present for ABI compat - always set to 0
    pub _reserved: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum FullScreenMode {
    /// - macOS: If the window is in windowed mode, transitions it slowly to fullscreen mode
    /// - other: Does the same as `FastFullScreen`.
    SlowFullScreen,
    /// Window should immediately go into fullscreen mode (on macOS this is not the default
    /// behaviour).
    FastFullScreen,
    /// - macOS: If the window is in fullscreen mode, transitions slowly back to windowed state.
    /// - other: Does the same as `FastWindowed`.
    SlowWindowed,
    /// If the window is in fullscreen mode, will immediately go back to windowed mode (on macOS
    /// this is not the default behaviour).
    FastWindowed,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
// Translation type because in winit 24.0 the WinitWaylandTheme is a trait instead
// of a struct, which makes things more complicated
pub struct WaylandTheme {
    pub title_bar_active_background_color: [u8; 4],
    pub title_bar_active_separator_color: [u8; 4],
    pub title_bar_active_text_color: [u8; 4],
    pub title_bar_inactive_background_color: [u8; 4],
    pub title_bar_inactive_separator_color: [u8; 4],
    pub title_bar_inactive_text_color: [u8; 4],
    pub maximize_idle_foreground_inactive_color: [u8; 4],
    pub minimize_idle_foreground_inactive_color: [u8; 4],
    pub close_idle_foreground_inactive_color: [u8; 4],
    pub maximize_hovered_foreground_inactive_color: [u8; 4],
    pub minimize_hovered_foreground_inactive_color: [u8; 4],
    pub close_hovered_foreground_inactive_color: [u8; 4],
    pub maximize_disabled_foreground_inactive_color: [u8; 4],
    pub minimize_disabled_foreground_inactive_color: [u8; 4],
    pub close_disabled_foreground_inactive_color: [u8; 4],
    pub maximize_idle_background_inactive_color: [u8; 4],
    pub minimize_idle_background_inactive_color: [u8; 4],
    pub close_idle_background_inactive_color: [u8; 4],
    pub maximize_hovered_background_inactive_color: [u8; 4],
    pub minimize_hovered_background_inactive_color: [u8; 4],
    pub close_hovered_background_inactive_color: [u8; 4],
    pub maximize_disabled_background_inactive_color: [u8; 4],
    pub minimize_disabled_background_inactive_color: [u8; 4],
    pub close_disabled_background_inactive_color: [u8; 4],
    pub maximize_idle_foreground_active_color: [u8; 4],
    pub minimize_idle_foreground_active_color: [u8; 4],
    pub close_idle_foreground_active_color: [u8; 4],
    pub maximize_hovered_foreground_active_color: [u8; 4],
    pub minimize_hovered_foreground_active_color: [u8; 4],
    pub close_hovered_foreground_active_color: [u8; 4],
    pub maximize_disabled_foreground_active_color: [u8; 4],
    pub minimize_disabled_foreground_active_color: [u8; 4],
    pub close_disabled_foreground_active_color: [u8; 4],
    pub maximize_idle_background_active_color: [u8; 4],
    pub minimize_idle_background_active_color: [u8; 4],
    pub close_idle_background_active_color: [u8; 4],
    pub maximize_hovered_background_active_color: [u8; 4],
    pub minimize_hovered_background_active_color: [u8; 4],
    pub close_hovered_background_active_color: [u8; 4],
    pub maximize_disabled_background_active_color: [u8; 4],
    pub minimize_disabled_background_active_color: [u8; 4],
    pub close_disabled_background_active_color: [u8; 4],
    pub title_bar_font: AzString,
    pub title_bar_font_size: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct WindowSize {
    /// Width and height of the window, in logical
    /// units (may not correspond to the physical on-screen size)
    pub dimensions: LogicalSize,
    /// Actual DPI value (default: 96)
    pub dpi: u32,
    /// Minimum dimensions of the window
    pub min_dimensions: OptionLogicalSize,
    /// Maximum dimensions of the window
    pub max_dimensions: OptionLogicalSize,
}

impl WindowSize {
    pub fn get_layout_size(&self) -> LayoutSize {
        LayoutSize::new(
            libm::roundf(self.dimensions.width) as isize,
            libm::roundf(self.dimensions.height) as isize,
        )
    }

    /// Get the actual logical size
    pub fn get_logical_size(&self) -> LogicalSize {
        self.dimensions
    }

    pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.dimensions
            .to_physical(self.get_hidpi_factor().inner.get())
    }

    pub fn get_hidpi_factor(&self) -> DpiScaleFactor {
        DpiScaleFactor {
            inner: FloatValue::new(self.dpi as f32 / 96.0),
        }
    }
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            #[cfg(not(feature = "glow"))]
            dimensions: LogicalSize::new(640.0, 480.0),
            dpi: 96,
            min_dimensions: None.into(),
            max_dimensions: None.into(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum RendererType {
    /// Force hardware rendering
    Hardware,
    /// Force software rendering
    Software,
}

impl_option!(
    RendererType,
    OptionRendererType,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]
);

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum UpdateFocusWarning {
    FocusInvalidDomId(DomId),
    FocusInvalidNodeId(NodeHierarchyItemId),
    CouldNotFindFocusNode(CssPath),
}

impl ::core::fmt::Display for UpdateFocusWarning {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use self::UpdateFocusWarning::*;
        match self {
            FocusInvalidDomId(dom_id) => write!(f, "Focusing on DOM with invalid ID: {:?}", dom_id),
            FocusInvalidNodeId(node_id) => {
                write!(f, "Focusing on node with invalid ID: {}", node_id)
            }
            CouldNotFindFocusNode(css_path) => {
                write!(f, "Could not find focus node for path: {}", css_path)
            }
        }
    }
}

/// Utility function for easier creation of a keymap - i.e. `[vec![Ctrl, S], my_function]`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum AcceleratorKey {
    Ctrl,
    Alt,
    Shift,
    Key(VirtualKeyCode),
}

impl AcceleratorKey {
    /// Checks if the current keyboard state contains the given char or modifier,
    /// i.e. if the keyboard state currently has the shift key pressed and the
    /// accelerator key is `Shift`, evaluates to true, otherwise to false.
    pub fn matches(&self, keyboard_state: &KeyboardState) -> bool {
        use self::AcceleratorKey::*;
        match self {
            Ctrl => keyboard_state.ctrl_down(),
            Alt => keyboard_state.alt_down(),
            Shift => keyboard_state.shift_down(),
            Key(k) => keyboard_state.is_key_down(*k),
        }
    }
}

/// Symbolic name for a keyboard key, does NOT take the keyboard locale into account
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum VirtualKeyCode {
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Caret,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadDivide,
    NumpadDecimal,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    NumpadMultiply,
    NumpadSubtract,
    AbntC1,
    AbntC2,
    Apostrophe,
    Apps,
    Asterisk,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    OEM102,
    Period,
    PlayPause,
    Plus,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Copy,
    Paste,
    Cut,
}

impl VirtualKeyCode {
    pub fn get_lowercase(&self) -> Option<char> {
        use self::VirtualKeyCode::*;
        match self {
            A => Some('a'),
            B => Some('b'),
            C => Some('c'),
            D => Some('d'),
            E => Some('e'),
            F => Some('f'),
            G => Some('g'),
            H => Some('h'),
            I => Some('i'),
            J => Some('j'),
            K => Some('k'),
            L => Some('l'),
            M => Some('m'),
            N => Some('n'),
            O => Some('o'),
            P => Some('p'),
            Q => Some('q'),
            R => Some('r'),
            S => Some('s'),
            T => Some('t'),
            U => Some('u'),
            V => Some('v'),
            W => Some('w'),
            X => Some('x'),
            Y => Some('y'),
            Z => Some('z'),
            Key0 | Numpad0 => Some('0'),
            Key1 | Numpad1 => Some('1'),
            Key2 | Numpad2 => Some('2'),
            Key3 | Numpad3 => Some('3'),
            Key4 | Numpad4 => Some('4'),
            Key5 | Numpad5 => Some('5'),
            Key6 | Numpad6 => Some('6'),
            Key7 | Numpad7 => Some('7'),
            Key8 | Numpad8 => Some('8'),
            Key9 | Numpad9 => Some('9'),
            Minus => Some('-'),
            Asterisk => Some('´'),
            At => Some('@'),
            Period => Some('.'),
            Semicolon => Some(';'),
            Slash => Some('/'),
            Caret => Some('^'),
            _ => None,
        }
    }
}

/// 16x16x4 bytes icon
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SmallWindowIconBytes {
    pub key: IconKey,
    pub rgba_bytes: U8Vec,
}

/// 16x16x4 bytes icon
#[derive(Debug, Clone)]
#[repr(C)]
pub struct LargeWindowIconBytes {
    pub key: IconKey,
    pub rgba_bytes: U8Vec,
}

// Window icon that usually appears in the top-left corner of the window
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum WindowIcon {
    Small(SmallWindowIconBytes),
    /// 32x32x4 bytes icon
    Large(LargeWindowIconBytes),
}

impl_option!(
    WindowIcon,
    OptionWindowIcon,
    copy = false,
    [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]
);

impl WindowIcon {
    pub fn get_key(&self) -> IconKey {
        match &self {
            WindowIcon::Small(SmallWindowIconBytes { key, .. }) => *key,
            WindowIcon::Large(LargeWindowIconBytes { key, .. }) => *key,
        }
    }
}
// -- Only compare the IconKey (for WindowIcon and TaskBarIcon)

impl PartialEq for WindowIcon {
    fn eq(&self, rhs: &Self) -> bool {
        self.get_key() == rhs.get_key()
    }
}

impl PartialOrd for WindowIcon {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some((self.get_key()).cmp(&rhs.get_key()))
    }
}

impl Eq for WindowIcon {}

impl Ord for WindowIcon {
    fn cmp(&self, rhs: &Self) -> Ordering {
        (self.get_key()).cmp(&rhs.get_key())
    }
}

impl Hash for WindowIcon {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.get_key().hash(state);
    }
}

/// 256x256x4 bytes window icon
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TaskBarIcon {
    pub key: IconKey,
    pub rgba_bytes: U8Vec,
}

impl_option!(
    TaskBarIcon,
    OptionTaskBarIcon,
    copy = false,
    [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]
);

impl PartialEq for TaskBarIcon {
    fn eq(&self, rhs: &Self) -> bool {
        self.key == rhs.key
    }
}

impl PartialOrd for TaskBarIcon {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some((self.key).cmp(&rhs.key))
    }
}

impl Eq for TaskBarIcon {}

impl Ord for TaskBarIcon {
    fn cmp(&self, rhs: &Self) -> Ordering {
        (self.key).cmp(&rhs.key)
    }
}

impl Hash for TaskBarIcon {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.key.hash(state);
    }
}
