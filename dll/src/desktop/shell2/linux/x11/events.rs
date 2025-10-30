//! X11 Event handling - Cross-platform V2 event system with state-diffing
//!
//! This module implements the same event processing architecture as Windows and macOS:
//! 1. Save previous_window_state before modifying current_window_state
//! 2. Update current_window_state based on X11 events
//! 3. Use create_events_from_states() to detect changes via state diffing
//! 4. Use dispatch_events() to determine which callbacks to invoke
//! 5. Invoke callbacks recursively with depth limit
//! 6. Process callback results (DOM regeneration, window state changes, etc.)
//!
//! Includes full IME (XIM) support for international text input.

use std::{
    ffi::{CStr, CString},
    rc::Rc,
};

use azul_core::{
    callbacks::Update,
    dom::{DomId, NodeId},
    events::{
        dispatch_events, CallbackTarget as CoreCallbackTarget, EventFilter, MouseButton,
        ProcessEventResult,
    },
    geom::{LogicalPosition, PhysicalPosition},
    hit_test::FullHitTest,
    window::{CursorPosition, VirtualKeyCode},
};
use azul_layout::callbacks::{CallCallbacksResult, CallbackInfo};

use super::{defines::*, dlopen::Xlib, X11Window};
use crate::desktop::shell2::common::event_v2::PlatformWindowV2;

// ============================================================================
// IME Support (X Input Method)
// ============================================================================

pub(super) struct ImeManager {
    xlib: Rc<Xlib>,
    xim: XIM,
    xic: XIC,
}

impl ImeManager {
    pub(super) fn new(xlib: &Rc<Xlib>, display: *mut Display, window: Window) -> Option<Self> {
        unsafe {
            // Set the locale. This is crucial for XIM to work correctly.
            let locale = CString::new("").unwrap();
            (xlib.XSetLocaleModifiers)(locale.as_ptr());

            let xim = (xlib.XOpenIM)(
                display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if xim.is_null() {
                eprintln!("[X11 IME] Could not open input method. IME will not be available.");
                return None;
            }

            let client_window_str = CString::new("clientWindow").unwrap();
            let input_style_str = CString::new("inputStyle").unwrap();

            let xic = (xlib.XCreateIC)(
                xim,
                input_style_str.as_ptr(),
                XIMPreeditNothing | XIMStatusNothing,
                client_window_str.as_ptr(),
                window,
                std::ptr::null_mut() as *const i8, // Sentinel
            );

            if xic.is_null() {
                eprintln!("[X11 IME] Could not create input context. IME will not be available.");
                (xlib.XCloseIM)(xim);
                return None;
            }

            (xlib.XSetICFocus)(xic);

            Some(Self {
                xlib: xlib.clone(),
                xim,
                xic,
            })
        }
    }

    /// Filters an event through the IME.
    /// Returns `true` if the event was consumed by the IME.
    pub(super) fn filter_event(&self, event: &mut XEvent) -> bool {
        unsafe { (self.xlib.XFilterEvent)(event, 0) != 0 }
    }

    /// Translates a key event into a character and a keysym, considering the IME.
    pub(super) fn lookup_string(&self, event: &mut XKeyEvent) -> (Option<String>, Option<KeySym>) {
        let mut keysym: KeySym = 0;
        let mut status: i32 = 0;
        let mut buffer: [i8; 32] = [0; 32];

        let count = unsafe {
            (self.xlib.XmbLookupString)(
                self.xic,
                event,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut keysym,
                &mut status,
            )
        };

        let chars = if count > 0 {
            Some(unsafe {
                CStr::from_ptr(buffer.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            })
        } else {
            None
        };

        let keysym = if keysym != 0 { Some(keysym) } else { None };

        (chars, keysym)
    }
}

impl Drop for ImeManager {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XDestroyIC)(self.xic);
            (self.xlib.XCloseIM)(self.xim);
        }
    }
}

// ============================================================================
// Event Handler - Main Implementation
// ============================================================================

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events,
    /// keys)
    RootNodes,
}

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

impl X11Window {
    // ========================================================================
    // V2 Cross-Platform Event Processing (from macOS/Windows)
    // ========================================================================

    /// V2: Process window events using cross-platform dispatch system.
    ///
    /// V2: Process callback result and update window state.
    /// Returns the appropriate ProcessEventResult based on what changed.
    fn process_callback_result_v2(&mut self, result: &CallCallbacksResult) -> ProcessEventResult {
        let mut event_result = ProcessEventResult::DoNothing;

        // Handle window state modifications
        if let Some(ref modified_state) = result.modified_window_state {
            self.current_window_state.title = modified_state.title.clone();
            self.current_window_state.size = modified_state.size;
            self.current_window_state.position = modified_state.position;
            self.current_window_state.flags = modified_state.flags;
            self.current_window_state.background_color = modified_state.background_color;

            // Check if window should close
            if modified_state.flags.close_requested {
                self.is_open = false;
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle focus changes
        if let Some(new_focus) = result.update_focused_node {
            self.current_window_state.focused_node = new_focus;
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle image updates
        if result.images_changed.is_some() || result.image_masks_changed.is_some() {
            event_result =
                event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
        }

        // Handle timers and threads
        if result.timers.is_some()
            || result.timers_removed.is_some()
            || result.threads.is_some()
            || result.threads_removed.is_some()
        {
            // TODO: Implement timer/thread management for X11
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Process Update screen command
        match result.callbacks_update_screen {
            Update::RefreshDom => {
                if let Err(e) = self.regenerate_layout() {
                    eprintln!("Layout regeneration error: {}", e);
                }
                event_result =
                    event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
            Update::RefreshDomAllWindows => {
                if let Err(e) = self.regenerate_layout() {
                    eprintln!("Layout regeneration error: {}", e);
                }
                event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            }
            Update::DoNothing => {}
        }

        event_result
    }

    // ========================================================================
    // Event Handlers (State-Diffing Pattern)
    // ========================================================================

    /// Handle mouse button press/release events
    pub fn handle_mouse_button(&mut self, event: &XButtonEvent) -> ProcessEventResult {
        let is_down = event.type_ == ButtonPress;
        let position = LogicalPosition::new(event.x as f32, event.y as f32);

        // Map X11 button to MouseButton
        let button = match event.button {
            1 => MouseButton::Left,
            2 => MouseButton::Middle,
            3 => MouseButton::Right,
            4 if is_down => {
                // Scroll up - handle separately
                return self.handle_scroll(0.0, 1.0, position);
            }
            5 if is_down => {
                // Scroll down - handle separately
                return self.handle_scroll(0.0, -1.0, position);
            }
            _ => MouseButton::Other(event.button as u8),
        };

        // Check for scrollbar hit FIRST (before state changes)
        if is_down {
            if let Some(scrollbar_hit_id) = self.perform_scrollbar_hit_test(position) {
                return self.handle_scrollbar_click(scrollbar_hit_id, position);
            }
        } else {
            // End scrollbar drag if active
            if self.scrollbar_drag_state.is_some() {
                self.scrollbar_drag_state = None;
                return ProcessEventResult::ShouldReRenderCurrentWindow;
            }
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = is_down,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = is_down,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = is_down,
            _ => {}
        }

        // Update hit test
        self.update_hit_test(position);

        // Check for right-click context menu (before event processing)
        if !is_down && button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                if self.try_show_context_menu(hit_node, position) {
                    return ProcessEventResult::DoNothing;
                }
            }
        }

        // V2 system will automatically detect MouseDown/MouseUp and dispatch callbacks
        self.process_window_events_recursive_v2(0)
    }

    /// Handle mouse motion events
    pub fn handle_mouse_move(&mut self, event: &XMotionEvent) -> ProcessEventResult {
        let position = LogicalPosition::new(event.x as f32, event.y as f32);

        // Handle active scrollbar drag (special case - not part of normal event system)
        if self.scrollbar_drag_state.is_some() {
            return self.handle_scrollbar_drag(position);
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Update hit test
        self.update_hit_test(position);

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        self.process_window_events_recursive_v2(0)
    }

    /// Handle mouse entering/leaving window
    pub fn handle_mouse_crossing(&mut self, event: &XCrossingEvent) -> ProcessEventResult {
        let position = LogicalPosition::new(event.x as f32, event.y as f32);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state based on enter/leave
        if event.type_ == EnterNotify {
            self.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(position);
            self.update_hit_test(position);
        } else if event.type_ == LeaveNotify {
            self.current_window_state.mouse_state.cursor_position =
                CursorPosition::OutOfWindow(position);
            // Clear hit test since mouse is out
            self.current_window_state.last_hit_test = FullHitTest::empty(None);
        }

        // V2 system will detect MouseEnter/MouseLeave from state diff
        self.process_window_events_recursive_v2(0)
    }

    /// Handle scroll wheel events (X11 button 4/5)
    fn handle_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update scroll state
        use azul_css::OptionF32;
        let current_x = self
            .current_window_state
            .mouse_state
            .scroll_x
            .into_option()
            .unwrap_or(0.0);
        let current_y = self
            .current_window_state
            .mouse_state
            .scroll_y
            .into_option()
            .unwrap_or(0.0);

        self.current_window_state.mouse_state.scroll_x = OptionF32::Some(current_x + delta_x);
        self.current_window_state.mouse_state.scroll_y = OptionF32::Some(current_y + delta_y);

        // Update hit test
        self.update_hit_test(position);

        // GPU scroll for visible scrollbars (if delta is significant)
        if delta_x.abs() > 0.01 || delta_y.abs() > 0.01 {
            if let Some(hit_node) = self.get_first_hovered_node() {
                let _ = self.gpu_scroll(
                    hit_node.dom_id,
                    hit_node.node_id,
                    -delta_x * 20.0, // Scale for pixel scrolling
                    -delta_y * 20.0,
                );
            }
        }

        // V2 system will detect Scroll event from state diff
        self.process_window_events_recursive_v2(0)
    }

    /// Handle keyboard events (key press/release)
    pub fn handle_keyboard(&mut self, event: &mut XKeyEvent) -> ProcessEventResult {
        let is_down = event.type_ == KeyPress;

        // Use IME for character translation
        let (char_str, keysym) = if let Some(ime) = &self.ime_manager {
            ime.lookup_string(event)
        } else {
            // Fallback for when IME is not available
            let mut keysym: KeySym = 0;
            let mut buffer = [0; 32];
            let count = unsafe {
                (self.xlib.XLookupString)(
                    event,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                    &mut keysym,
                    std::ptr::null_mut(),
                )
            };
            let chars = if count > 0 {
                unsafe {
                    CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                }
            } else {
                String::new()
            };
            (Some(chars), Some(keysym))
        };

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update keyboard state with virtual key and scancode
        if let Some(vk) = keysym.and_then(keysym_to_virtual_keycode) {
            if is_down {
                self.current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .insert_hm_item(vk);
                self.current_window_state
                    .keyboard_state
                    .current_virtual_keycode = Some(vk).into();

                // Track scancode (X11 keycode is the scancode)
                self.current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .insert_hm_item(event.keycode as u32);
            } else {
                self.current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .remove_hm_item(&vk);
                self.current_window_state
                    .keyboard_state
                    .current_virtual_keycode = None.into();

                // Remove scancode
                self.current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .remove_hm_item(&(event.keycode as u32));
            }
        }

        // Update keyboard state with character (for text input)
        if is_down {
            if let Some(s) = char_str {
                if let Some(c) = s.chars().next() {
                    self.current_window_state.keyboard_state.current_char = Some(c as u32).into();
                }
            }
        } else {
            self.current_window_state.keyboard_state.current_char = None.into();
        }

        // V2 system will detect VirtualKeyDown/VirtualKeyUp/TextInput from state diff
        self.process_window_events_recursive_v2(0)
    }

    // ========================================================================
    // Helper Functions for V2 Event System
    // ========================================================================

    /// Update hit test at given position and store in current_window_state
    fn update_hit_test(&mut self, position: LogicalPosition) {
        if let Some(layout_window) = self.layout_window.as_ref() {
            let cursor_position = CursorPosition::InWindow(position);
            let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
                &*self.hit_tester.as_mut().unwrap().resolve(),
                self.document_id.unwrap(),
                self.current_window_state.focused_node,
                &layout_window.layout_results,
                &cursor_position,
                self.current_window_state.size.get_hidpi_factor(),
            );
            self.current_window_state.last_hit_test = hit_test;
        }
    }

    /// Get the first hovered node from current hit test
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        self.current_window_state
            .last_hit_test
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes
                    .keys()
                    .next()
                    .map(|node_id| HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
            })
            .next()
    }

    /// Get raw window handle for callbacks
    fn get_raw_window_handle(&self) -> azul_core::window::RawWindowHandle {
        azul_core::window::RawWindowHandle::Xlib(azul_core::window::XlibHandle {
            window: self.window as u64,
            display: self.display as *mut std::ffi::c_void,
        })
    }

    // ========================================================================
    // Scrollbar Handling (from Windows/macOS)
    // ========================================================================

    /// Query WebRender hit-tester for scrollbar hits at given position
    fn perform_scrollbar_hit_test(
        &mut self,
        position: LogicalPosition,
    ) -> Option<azul_core::hit_test::ScrollbarHitId> {
        use webrender::api::units::WorldPoint;

        let hit_tester = &*self.hit_tester.as_mut()?.resolve();
        let world_point = WorldPoint::new(position.x, position.y);
        let hit_result = hit_tester.hit_test(world_point);

        // Check each hit item for scrollbar tag
        for item in &hit_result.items {
            if let Some(scrollbar_id) =
                crate::desktop::wr_translate2::translate_item_tag_to_scrollbar_hit_id(item.tag)
            {
                return Some(scrollbar_id);
            }
        }

        None
    }

    /// Handle scrollbar click (thumb or track)
    fn handle_scrollbar_click(
        &mut self,
        hit_id: azul_core::hit_test::ScrollbarHitId,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::hit_test::ScrollbarHitId;

        match hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id)
            | ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                // Start drag
                let layout_window = match self.layout_window.as_ref() {
                    Some(lw) => lw,
                    None => return ProcessEventResult::DoNothing,
                };

                let scroll_offset = layout_window
                    .scroll_states
                    .get_current_offset(dom_id, node_id)
                    .unwrap_or_default();

                self.scrollbar_drag_state = Some(azul_layout::ScrollbarDragState {
                    hit_id,
                    initial_mouse_pos: position,
                    initial_scroll_offset: scroll_offset,
                });

                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            ScrollbarHitId::VerticalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, true)
            }

            ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, false)
            }
        }
    }

    /// Handle track click - jump scroll to clicked position
    fn handle_track_click(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
        click_position: LogicalPosition,
        is_vertical: bool,
    ) -> ProcessEventResult {
        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Get scrollbar state
        let scrollbar_state = if is_vertical {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return ProcessEventResult::DoNothing,
        };

        let scroll_state = match layout_window
            .scroll_states
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate click ratio (0.0 = top/left, 1.0 = bottom/right)
        let click_ratio = if is_vertical {
            let track_top = scrollbar_state.track_rect.origin.y;
            let track_height = scrollbar_state.track_rect.size.height;
            ((click_position.y - track_top) / track_height).clamp(0.0, 1.0)
        } else {
            let track_left = scrollbar_state.track_rect.origin.x;
            let track_width = scrollbar_state.track_rect.size.width;
            ((click_position.x - track_left) / track_width).clamp(0.0, 1.0)
        };

        // Calculate target scroll position
        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);
        let target_scroll = click_ratio * max_scroll;

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let scroll_delta = target_scroll - current_scroll;

        // Apply scroll using gpu_scroll
        if let Err(e) = self.gpu_scroll(
            dom_id.inner as u64,
            node_id.index() as u64,
            if is_vertical { 0.0 } else { scroll_delta },
            if is_vertical { scroll_delta } else { 0.0 },
        ) {
            eprintln!("Track click scroll failed: {}", e);
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    /// Handle scrollbar drag (continuous thumb movement)
    fn handle_scrollbar_drag(&mut self, current_pos: LogicalPosition) -> ProcessEventResult {
        let drag_state = match &self.scrollbar_drag_state {
            Some(ds) => ds.clone(),
            None => return ProcessEventResult::DoNothing,
        };

        use azul_core::hit_test::ScrollbarHitId;
        let (dom_id, node_id, is_vertical) = match drag_state.hit_id {
            ScrollbarHitId::VerticalThumb(d, n) | ScrollbarHitId::VerticalTrack(d, n) => {
                (d, n, true)
            }
            ScrollbarHitId::HorizontalThumb(d, n) | ScrollbarHitId::HorizontalTrack(d, n) => {
                (d, n, false)
            }
        };

        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        let scrollbar_state = if is_vertical {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return ProcessEventResult::DoNothing,
        };

        let scroll_state = match layout_window
            .scroll_states
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate mouse delta in pixels
        let pixel_delta = if is_vertical {
            current_pos.y - drag_state.initial_mouse_pos.y
        } else {
            current_pos.x - drag_state.initial_mouse_pos.x
        };

        // Convert pixel delta to scroll delta
        let track_size = if is_vertical {
            scrollbar_state.track_rect.size.height
        } else {
            scrollbar_state.track_rect.size.width
        };

        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);

        // Account for thumb size
        let thumb_size = scrollbar_state.thumb_size_ratio * track_size;
        let usable_track_size = (track_size - thumb_size).max(1.0);

        // Calculate scroll delta
        let scroll_delta = if usable_track_size > 0.0 {
            (pixel_delta / usable_track_size) * max_scroll
        } else {
            0.0
        };

        // Calculate target scroll position
        let target_scroll = if is_vertical {
            drag_state.initial_scroll_offset.y + scroll_delta
        } else {
            drag_state.initial_scroll_offset.x + scroll_delta
        };

        let target_scroll = target_scroll.clamp(0.0, max_scroll);

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let delta_from_current = target_scroll - current_scroll;

        // Apply scroll
        if let Err(e) = self.gpu_scroll(
            dom_id.inner as u64,
            node_id.index() as u64,
            if is_vertical { 0.0 } else { delta_from_current },
            if is_vertical { delta_from_current } else { 0.0 },
        ) {
            eprintln!("Scrollbar drag failed: {}", e);
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    /// GPU scroll implementation
    pub fn gpu_scroll(
        &mut self,
        dom_id: u64,
        node_id: u64,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return Err("No layout window".into()),
        };

        use azul_core::{
            dom::{DomId, NodeId},
            geom::LogicalPosition,
        };

        let dom_id_typed = DomId {
            inner: dom_id as usize,
        };
        let node_id_typed = match NodeId::from_usize(node_id as usize) {
            Some(nid) => nid,
            None => return Err("Invalid node ID".into()),
        };

        // Apply scroll delta
        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
        layout_window.scroll_states.scroll_by(
            dom_id_typed,
            node_id_typed,
            LogicalPosition::new(delta_x, delta_y),
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff {
                secs: 0,
                nanos: 0,
            }),
            azul_core::events::EasingFunction::Linear,
            (external.get_system_time_fn.cb)(),
        );

        // Recalculate scrollbar states after scroll update
        layout_window.scroll_states.calculate_scrollbar_states();

        // Update WebRender scroll layers and GPU transforms
        if let (Some(render_api), Some(document_id)) = (&mut self.render_api, self.document_id) {
            let mut txn = crate::desktop::wr_translate2::WrTransaction::new();

            // Scroll all nodes to WebRender
            crate::desktop::wr_translate2::scroll_all_nodes(layout_window, &mut txn);

            // Synchronize GPU-animated values
            crate::desktop::wr_translate2::synchronize_gpu_values(layout_window, &mut txn);

            // Generate frame
            crate::desktop::wr_translate2::generate_frame(
                &mut txn,
                layout_window,
                render_api,
                false, // Display list not rebuilt
            );

            // Send transaction
            render_api.send_transaction(
                crate::desktop::wr_translate2::wr_translate_document_id(document_id),
                txn,
            );
        }

        Ok(())
    }

    // ========================================================================
    // Context Menu Support
    // ========================================================================

    /// Try to show context menu for the given node at position
    ///
    /// Uses the unified menu system (crate::desktop::menu::show_menu) which is identical
    /// to how menu bar menus work, but spawns at cursor position instead of below a trigger rect.
    /// Returns true if a menu was shown
    fn try_show_context_menu(&mut self, node: HitTestNode, position: LogicalPosition) -> bool {
        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return false,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return false,
        };

        // Check if this node has a context menu
        let node_id = match azul_core::id::NodeId::from_usize(node.node_id as usize) {
            Some(nid) => nid,
            None => return false,
        };

        let binding = layout_result.styled_dom.node_data.as_container();
        let node_data = match binding.get(node_id) {
            Some(nd) => nd,
            None => return false,
        };

        // Context menus are stored directly on NodeData
        let context_menu = match node_data.get_context_menu() {
            Some(menu) => menu,
            None => return false,
        };

        eprintln!(
            "[Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Get system style from resources
        let system_style = self.resources.system_style.clone();

        // Get parent window position
        let parent_pos = match self.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let menu_options = crate::desktop::menu::show_menu(
            (**context_menu).clone(), // Dereference Box<Menu>
            system_style,
            parent_pos,
            None,           // No trigger rect for context menus (they spawn at cursor)
            Some(position), // Cursor position for menu positioning
            None,           // No parent menu
        );

        // Create the menu window and register it in the window registry
        // X11 supports full multi-window management via the registry system
        match super::X11Window::new_with_resources(menu_options, self.resources.clone()) {
            Ok(menu_window) => {
                // Register as owned menu window to prevent drop
                // The window will be managed by the registry and event loop
                super::super::registry::register_owned_menu_window(Box::new(menu_window));
                eprintln!("[Context Menu] Menu window created and registered successfully");
                true
            }
            Err(e) => {
                eprintln!("[Context Menu] Failed to create menu window: {:?}", e);
                false
            }
        }
    }
}

// ============================================================================
// Extension Trait for Callback Conversion
// ============================================================================

trait CallbackExt {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self;
}

impl CallbackExt for azul_layout::callbacks::Callback {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self {
        // Use the existing safe wrapper method from Callback
        azul_layout::callbacks::Callback::from_core(core_callback)
    }
}

// ============================================================================
// Keycode Conversion
// ============================================================================

pub fn keysym_to_virtual_keycode(keysym: KeySym) -> Option<VirtualKeyCode> {
    // This is a partial mapping based on X11/keysymdef.h
    match keysym as u32 {
        XK_BackSpace => Some(VirtualKeyCode::Back),
        XK_Tab => Some(VirtualKeyCode::Tab),
        XK_Return => Some(VirtualKeyCode::Return),
        XK_Pause => Some(VirtualKeyCode::Pause),
        XK_Scroll_Lock => Some(VirtualKeyCode::Scroll),
        XK_Escape => Some(VirtualKeyCode::Escape),
        XK_Home => Some(VirtualKeyCode::Home),
        XK_Left => Some(VirtualKeyCode::Left),
        XK_Up => Some(VirtualKeyCode::Up),
        XK_Right => Some(VirtualKeyCode::Right),
        XK_Down => Some(VirtualKeyCode::Down),
        XK_Page_Up => Some(VirtualKeyCode::PageUp),
        XK_Page_Down => Some(VirtualKeyCode::PageDown),
        XK_End => Some(VirtualKeyCode::End),
        XK_Insert => Some(VirtualKeyCode::Insert),
        XK_Delete => Some(VirtualKeyCode::Delete),
        XK_space => Some(VirtualKeyCode::Space),
        XK_0 => Some(VirtualKeyCode::Key0),
        XK_1 => Some(VirtualKeyCode::Key1),
        XK_2 => Some(VirtualKeyCode::Key2),
        XK_3 => Some(VirtualKeyCode::Key3),
        XK_4 => Some(VirtualKeyCode::Key4),
        XK_5 => Some(VirtualKeyCode::Key5),
        XK_6 => Some(VirtualKeyCode::Key6),
        XK_7 => Some(VirtualKeyCode::Key7),
        XK_8 => Some(VirtualKeyCode::Key8),
        XK_9 => Some(VirtualKeyCode::Key9),
        XK_a | XK_A => Some(VirtualKeyCode::A),
        XK_b | XK_B => Some(VirtualKeyCode::B),
        XK_c | XK_C => Some(VirtualKeyCode::C),
        XK_d | XK_D => Some(VirtualKeyCode::D),
        XK_e | XK_E => Some(VirtualKeyCode::E),
        XK_f | XK_F => Some(VirtualKeyCode::F),
        XK_g | XK_G => Some(VirtualKeyCode::G),
        XK_h | XK_H => Some(VirtualKeyCode::H),
        XK_i | XK_I => Some(VirtualKeyCode::I),
        XK_j | XK_J => Some(VirtualKeyCode::J),
        XK_k | XK_K => Some(VirtualKeyCode::K),
        XK_l | XK_L => Some(VirtualKeyCode::L),
        XK_m | XK_M => Some(VirtualKeyCode::M),
        XK_n | XK_N => Some(VirtualKeyCode::N),
        XK_o | XK_O => Some(VirtualKeyCode::O),
        XK_p | XK_P => Some(VirtualKeyCode::P),
        XK_q | XK_Q => Some(VirtualKeyCode::Q),
        XK_r | XK_R => Some(VirtualKeyCode::R),
        XK_s | XK_S => Some(VirtualKeyCode::S),
        XK_t | XK_T => Some(VirtualKeyCode::T),
        XK_u | XK_U => Some(VirtualKeyCode::U),
        XK_v | XK_V => Some(VirtualKeyCode::V),
        XK_w | XK_W => Some(VirtualKeyCode::W),
        XK_x | XK_X => Some(VirtualKeyCode::X),
        XK_y | XK_Y => Some(VirtualKeyCode::Y),
        XK_z | XK_Z => Some(VirtualKeyCode::Z),
        XK_F1 => Some(VirtualKeyCode::F1),
        XK_F2 => Some(VirtualKeyCode::F2),
        XK_F3 => Some(VirtualKeyCode::F3),
        XK_F4 => Some(VirtualKeyCode::F4),
        XK_F5 => Some(VirtualKeyCode::F5),
        XK_F6 => Some(VirtualKeyCode::F6),
        XK_F7 => Some(VirtualKeyCode::F7),
        XK_F8 => Some(VirtualKeyCode::F8),
        XK_F9 => Some(VirtualKeyCode::F9),
        XK_F10 => Some(VirtualKeyCode::F10),
        XK_F11 => Some(VirtualKeyCode::F11),
        XK_F12 => Some(VirtualKeyCode::F12),
        XK_Shift_L => Some(VirtualKeyCode::LShift),
        XK_Shift_R => Some(VirtualKeyCode::RShift),
        XK_Control_L => Some(VirtualKeyCode::LControl),
        XK_Control_R => Some(VirtualKeyCode::RControl),
        XK_Alt_L => Some(VirtualKeyCode::LAlt),
        XK_Alt_R => Some(VirtualKeyCode::RAlt),
        XK_Super_L => Some(VirtualKeyCode::LWin),
        XK_Super_R => Some(VirtualKeyCode::RWin),
        _ => None,
    }
}
