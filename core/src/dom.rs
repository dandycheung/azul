//! Defines the core Document Object Model (DOM) structures.
//!
//! This module is responsible for representing the UI as a tree of nodes,
//! similar to the HTML DOM. It includes definitions for node types, event handling,
//! accessibility, and the main `Dom` and `CompactDom` structures.

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    iter::FromIterator,
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_css::{
    css::{Css, NodeTypeTag},
    format_rust_code::GetHash,
    props::{
        basic::FontRef,
        layout::{LayoutDisplay, LayoutFloat, LayoutPosition},
        property::CssProperty,
    },
    AzString, OptionAzString,
};

// Re-export event filters from events module (moved in Phase 3.5)
pub use crate::events::{
    ApplicationEventFilter, ComponentEventFilter, EventFilter, FocusEventFilter, HoverEventFilter,
    NotEventFilter, WindowEventFilter,
};
pub use crate::id::{Node, NodeHierarchy, NodeId};
use crate::{
    callbacks::{
        CoreCallback, CoreCallbackData, CoreCallbackDataVec, CoreCallbackType, IFrameCallback,
        IFrameCallbackType,
    },
    id::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut},
    menu::Menu,
    prop_cache::{CssPropertyCache, CssPropertyCachePtr},
    refany::{OptionRefAny, RefAny},
    resources::{
        image_ref_get_hash, CoreImageCallback, ImageMask, ImageRef, ImageRefHash, RendererResources,
    },
    styled_dom::{
        CompactDom, NodeHierarchyItemId, StyleFontFamilyHash, StyledDom, StyledNode,
        StyledNodeState,
    },
    window::OptionVirtualKeyCodeCombo,
};

static TAG_ID: AtomicUsize = AtomicUsize::new(1);

/// Unique tag that is used to annotate which rectangles are relevant for hit-testing.
/// These tags are generated per-frame to identify interactable areas.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TagId(pub u64);

impl ::core::fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TagId({})", self.0)
    }
}

impl ::core::fmt::Debug for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl TagId {
    /// Creates a new, unique hit-testing tag ID.
    pub fn unique() -> Self {
        TagId(TAG_ID.fetch_add(1, Ordering::SeqCst) as u64)
    }

    /// Resets the counter (usually done after each frame) so that we can
    /// track hit-testing Tag IDs of subsequent frames.
    pub fn reset() {
        TAG_ID.swap(1, Ordering::SeqCst);
    }
}

/// Same as the `TagId`, but only for scrollable nodes.
/// This provides a typed distinction for tags associated with scrolling containers.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ScrollTagId(pub TagId);

impl ::core::fmt::Display for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ScrollTagId({})", (self.0).0)
    }
}

impl ::core::fmt::Debug for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl ScrollTagId {
    /// Creates a new, unique scroll tag ID. Note that this should not
    /// be used for identifying nodes, use the `DomNodeHash` instead.
    pub fn unique() -> ScrollTagId {
        ScrollTagId(TagId::unique())
    }
}

/// Orientation of a scrollbar.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

/// Calculated hash of a DOM node, used for identifying identical DOM
/// nodes across frames for efficient diffing and state preservation.
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DomNodeHash(pub u64);

impl ::core::fmt::Debug for DomNodeHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DomNodeHash({})", self.0)
    }
}

/// List of core DOM node types built into `azul`.
/// This enum defines the building blocks of the UI, similar to HTML tags.
#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeType {
    // Root and container elements
    /// Root element of the document.
    Body,
    /// Generic block-level container.
    Div,
    /// Paragraph.
    P,
    /// Headings.
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    /// Line break.
    Br,
    /// Horizontal rule.
    Hr,
    /// Preformatted text.
    Pre,
    /// Block quote.
    BlockQuote,
    /// Address.
    Address,

    // List elements
    /// Unordered list.
    Ul,
    /// Ordered list.
    Ol,
    /// List item.
    Li,
    /// Definition list.
    Dl,
    /// Definition term.
    Dt,
    /// Definition description.
    Dd,

    // Table elements
    /// Table container.
    Table,
    /// Table caption.
    Caption,
    /// Table header.
    THead,
    /// Table body.
    TBody,
    /// Table footer.
    TFoot,
    /// Table row.
    Tr,
    /// Table header cell.
    Th,
    /// Table data cell.
    Td,
    /// Table column group.
    ColGroup,
    /// Table column.
    Col,

    // Form elements
    /// Form container.
    Form,
    /// Form fieldset.
    FieldSet,
    /// Fieldset legend.
    Legend,
    /// Label for form controls.
    Label,
    /// Input control.
    Input,
    /// Button control.
    Button,
    /// Select dropdown.
    Select,
    /// Option group.
    OptGroup,
    /// Select option.
    SelectOption,
    /// Multiline text input.
    TextArea,

    // Inline elements
    /// Generic inline container.
    Span,
    /// Anchor/hyperlink.
    A,
    /// Emphasized text.
    Em,
    /// Strongly emphasized text.
    Strong,
    /// Bold text.
    B,
    /// Italic text.
    I,
    /// Code.
    Code,
    /// Sample output.
    Samp,
    /// Keyboard input.
    Kbd,
    /// Variable.
    Var,
    /// Citation.
    Cite,
    /// Abbreviation.
    Abbr,
    /// Acronym.
    Acronym,
    /// Quotation.
    Q,
    /// Subscript.
    Sub,
    /// Superscript.
    Sup,
    /// Small text.
    Small,
    /// Big text.
    Big,

    // Pseudo-elements (transformed into real elements)
    /// ::before pseudo-element.
    Before,
    /// ::after pseudo-element.
    After,
    /// ::marker pseudo-element.
    Marker,
    /// ::placeholder pseudo-element.
    Placeholder,

    // Special content types
    /// Text content.
    Text(AzString),
    /// Image element.
    Image(ImageRef),
    /// IFrame (embedded content).
    IFrame(IFrameNode),
}

impl NodeType {
    /// Determines the default display value for a node type according to HTML standards.
    pub fn get_default_display(&self) -> LayoutDisplay {
        match self {
            // Block-level elements
            NodeType::Body
            | NodeType::Div
            | NodeType::P
            | NodeType::H1
            | NodeType::H2
            | NodeType::H3
            | NodeType::H4
            | NodeType::H5
            | NodeType::H6
            | NodeType::Pre
            | NodeType::BlockQuote
            | NodeType::Address
            | NodeType::Hr
            | NodeType::Ul
            | NodeType::Ol
            | NodeType::Li
            | NodeType::Dl
            | NodeType::Dt
            | NodeType::Dd
            | NodeType::Form
            | NodeType::FieldSet
            | NodeType::Legend => LayoutDisplay::Block,

            // Table elements
            NodeType::Table => LayoutDisplay::Table,
            NodeType::Caption => LayoutDisplay::TableCaption,
            NodeType::THead | NodeType::TBody | NodeType::TFoot => LayoutDisplay::TableRowGroup,
            NodeType::Tr => LayoutDisplay::TableRow,
            NodeType::Th | NodeType::Td => LayoutDisplay::TableCell,
            NodeType::ColGroup => LayoutDisplay::TableColumnGroup,
            NodeType::Col => LayoutDisplay::TableColumn,

            // Inline elements
            NodeType::Text(_)
            | NodeType::Br
            | NodeType::Image(_)
            | NodeType::Span
            | NodeType::A
            | NodeType::Em
            | NodeType::Strong
            | NodeType::B
            | NodeType::I
            | NodeType::Code
            | NodeType::Samp
            | NodeType::Kbd
            | NodeType::Var
            | NodeType::Cite
            | NodeType::Abbr
            | NodeType::Acronym
            | NodeType::Q
            | NodeType::Sub
            | NodeType::Sup
            | NodeType::Small
            | NodeType::Big
            | NodeType::Label
            | NodeType::Input
            | NodeType::Button
            | NodeType::Select
            | NodeType::OptGroup
            | NodeType::SelectOption
            | NodeType::TextArea => LayoutDisplay::Inline,

            // Special cases
            NodeType::IFrame(_) => LayoutDisplay::Block,

            // Pseudo-elements
            NodeType::Before | NodeType::After => LayoutDisplay::Inline,
            NodeType::Marker => LayoutDisplay::Marker,
            NodeType::Placeholder => LayoutDisplay::Inline,
        }
    }
    /// Returns the formatting context that this node type establishes by default.
    pub fn default_formatting_context(&self) -> FormattingContext {
        use self::NodeType::*;

        match self {
            // Regular block elements
            Body | Div | P | H1 | H2 | H3 | H4 | H5 | H6 | Pre | BlockQuote | Address | Hr | Ul
            | Ol | Li | Dl | Dt | Dd | Form | FieldSet | Legend => FormattingContext::Block {
                establishes_new_context: false,
            },

            // Table elements with specific formatting contexts
            Table => FormattingContext::Table,
            Caption => FormattingContext::TableCaption,
            THead | TBody | TFoot => FormattingContext::TableRowGroup,
            Tr => FormattingContext::TableRow,
            Th | Td => FormattingContext::TableCell,
            ColGroup => FormattingContext::TableColumnGroup,
            Col => FormattingContext::TableColumnGroup,

            // Inline elements
            Span | A | Em | Strong | B | I | Code | Samp | Kbd | Var | Cite | Abbr | Acronym
            | Q | Sub | Sup | Small | Big | Label | Input | Button | Select | OptGroup
            | SelectOption | TextArea | Text(_) | Br => FormattingContext::Inline,

            // Special elements
            Image(_) => FormattingContext::Inline,
            IFrame(_) => FormattingContext::Block {
                establishes_new_context: true,
            },

            // Pseudo-elements
            Before | After | Marker | Placeholder => FormattingContext::Inline,
        }
    }

    fn into_library_owned_nodetype(&self) -> Self {
        use self::NodeType::*;
        match self {
            Body => Body,
            Div => Div,
            P => P,
            H1 => H1,
            H2 => H2,
            H3 => H3,
            H4 => H4,
            H5 => H5,
            H6 => H6,
            Br => Br,
            Hr => Hr,
            Pre => Pre,
            BlockQuote => BlockQuote,
            Address => Address,
            Ul => Ul,
            Ol => Ol,
            Li => Li,
            Dl => Dl,
            Dt => Dt,
            Dd => Dd,
            Table => Table,
            Caption => Caption,
            THead => THead,
            TBody => TBody,
            TFoot => TFoot,
            Tr => Tr,
            Th => Th,
            Td => Td,
            ColGroup => ColGroup,
            Col => Col,
            Form => Form,
            FieldSet => FieldSet,
            Legend => Legend,
            Label => Label,
            Input => Input,
            Button => Button,
            Select => Select,
            OptGroup => OptGroup,
            SelectOption => SelectOption,
            TextArea => TextArea,
            Span => Span,
            A => A,
            Em => Em,
            Strong => Strong,
            B => B,
            I => I,
            Code => Code,
            Samp => Samp,
            Kbd => Kbd,
            Var => Var,
            Cite => Cite,
            Abbr => Abbr,
            Acronym => Acronym,
            Q => Q,
            Sub => Sub,
            Sup => Sup,
            Small => Small,
            Big => Big,
            Before => Before,
            After => After,
            Marker => Marker,
            Placeholder => Placeholder,

            Text(s) => Text(s.clone_self()),
            Image(i) => Image(i.clone()), // note: shallow clone
            IFrame(i) => IFrame(IFrameNode {
                callback: i.callback,
                data: i.data.clone(),
            }),
        }
    }

    pub(crate) fn format(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Text(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            IFrame(i) => Some(format!("iframe({:?})", i)),
            _ => None,
        }
    }

    /// Returns the NodeTypeTag for CSS selector matching.
    pub fn get_path(&self) -> NodeTypeTag {
        match self {
            Self::Body => NodeTypeTag::Body,
            Self::Div => NodeTypeTag::Div,
            Self::P => NodeTypeTag::P,
            Self::H1 => NodeTypeTag::H1,
            Self::H2 => NodeTypeTag::H2,
            Self::H3 => NodeTypeTag::H3,
            Self::H4 => NodeTypeTag::H4,
            Self::H5 => NodeTypeTag::H5,
            Self::H6 => NodeTypeTag::H6,
            Self::Br => NodeTypeTag::Br,
            Self::Hr => NodeTypeTag::Hr,
            Self::Pre => NodeTypeTag::Pre,
            Self::BlockQuote => NodeTypeTag::BlockQuote,
            Self::Address => NodeTypeTag::Address,
            Self::Ul => NodeTypeTag::Ul,
            Self::Ol => NodeTypeTag::Ol,
            Self::Li => NodeTypeTag::Li,
            Self::Dl => NodeTypeTag::Dl,
            Self::Dt => NodeTypeTag::Dt,
            Self::Dd => NodeTypeTag::Dd,
            Self::Table => NodeTypeTag::Table,
            Self::Caption => NodeTypeTag::Caption,
            Self::THead => NodeTypeTag::THead,
            Self::TBody => NodeTypeTag::TBody,
            Self::TFoot => NodeTypeTag::TFoot,
            Self::Tr => NodeTypeTag::Tr,
            Self::Th => NodeTypeTag::Th,
            Self::Td => NodeTypeTag::Td,
            Self::ColGroup => NodeTypeTag::ColGroup,
            Self::Col => NodeTypeTag::Col,
            Self::Form => NodeTypeTag::Form,
            Self::FieldSet => NodeTypeTag::FieldSet,
            Self::Legend => NodeTypeTag::Legend,
            Self::Label => NodeTypeTag::Label,
            Self::Input => NodeTypeTag::Input,
            Self::Button => NodeTypeTag::Button,
            Self::Select => NodeTypeTag::Select,
            Self::OptGroup => NodeTypeTag::OptGroup,
            Self::SelectOption => NodeTypeTag::SelectOption,
            Self::TextArea => NodeTypeTag::TextArea,
            Self::Span => NodeTypeTag::Span,
            Self::A => NodeTypeTag::A,
            Self::Em => NodeTypeTag::Em,
            Self::Strong => NodeTypeTag::Strong,
            Self::B => NodeTypeTag::B,
            Self::I => NodeTypeTag::I,
            Self::Code => NodeTypeTag::Code,
            Self::Samp => NodeTypeTag::Samp,
            Self::Kbd => NodeTypeTag::Kbd,
            Self::Var => NodeTypeTag::Var,
            Self::Cite => NodeTypeTag::Cite,
            Self::Abbr => NodeTypeTag::Abbr,
            Self::Acronym => NodeTypeTag::Acronym,
            Self::Q => NodeTypeTag::Q,
            Self::Sub => NodeTypeTag::Sub,
            Self::Sup => NodeTypeTag::Sup,
            Self::Small => NodeTypeTag::Small,
            Self::Big => NodeTypeTag::Big,
            Self::Text(_) => NodeTypeTag::Text,
            Self::Image(_) => NodeTypeTag::Img,
            Self::IFrame(_) => NodeTypeTag::IFrame,
            Self::Before => NodeTypeTag::Before,
            Self::After => NodeTypeTag::After,
            Self::Marker => NodeTypeTag::Marker,
            Self::Placeholder => NodeTypeTag::Placeholder,
        }
    }
}

/// Represents the CSS formatting context for an element
#[derive(Clone, PartialEq)]
pub enum FormattingContext {
    /// Block-level formatting context
    Block {
        /// Whether this element establishes a new block formatting context
        establishes_new_context: bool,
    },
    /// Inline-level formatting context
    Inline,
    /// Inline-block (participates in an IFC but creates a BFC)
    InlineBlock,
    /// Flex formatting context
    Flex,
    /// Float (left or right)
    Float(LayoutFloat),
    /// Absolutely positioned (out of flow)
    OutOfFlow(LayoutPosition),
    /// Table formatting context (container)
    Table,
    /// Table row group formatting context (thead, tbody, tfoot)
    TableRowGroup,
    /// Table row formatting context
    TableRow,
    /// Table cell formatting context (td, th)
    TableCell,
    /// Table column group formatting context
    TableColumnGroup,
    /// Table caption formatting context
    TableCaption,
    /// Grid formatting context
    Grid,
    /// No formatting context (display: none)
    None,
}

impl fmt::Debug for FormattingContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormattingContext::Block {
                establishes_new_context,
            } => write!(
                f,
                "Block {{ establishes_new_context: {establishes_new_context:?} }}"
            ),
            FormattingContext::Inline => write!(f, "Inline"),
            FormattingContext::InlineBlock => write!(f, "InlineBlock"),
            FormattingContext::Flex => write!(f, "Flex"),
            FormattingContext::Float(layout_float) => write!(f, "Float({layout_float:?})"),
            FormattingContext::OutOfFlow(layout_position) => {
                write!(f, "OutOfFlow({layout_position:?})")
            }
            FormattingContext::Grid => write!(f, "Grid"),
            FormattingContext::None => write!(f, "None"),
            FormattingContext::Table => write!(f, "Table"),
            FormattingContext::TableRowGroup => write!(f, "TableRowGroup"),
            FormattingContext::TableRow => write!(f, "TableRow"),
            FormattingContext::TableCell => write!(f, "TableCell"),
            FormattingContext::TableColumnGroup => write!(f, "TableColumnGroup"),
            FormattingContext::TableCaption => write!(f, "TableCaption"),
        }
    }
}

impl Default for FormattingContext {
    fn default() -> Self {
        FormattingContext::Block {
            establishes_new_context: false,
        }
    }
}

/// Defines the type of event that can trigger a callback action.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum On {
    /// Mouse cursor is hovering over the element.
    MouseOver,
    /// Mouse cursor has is over element and is pressed
    /// (not good for "click" events - use `MouseUp` instead).
    MouseDown,
    /// (Specialization of `MouseDown`). Fires only if the left mouse button
    /// has been pressed while cursor was over the element.
    LeftMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the middle mouse button
    /// has been pressed while cursor was over the element.
    MiddleMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the right mouse button
    /// has been pressed while cursor was over the element.
    RightMouseDown,
    /// Mouse button has been released while cursor was over the element.
    MouseUp,
    /// (Specialization of `MouseUp`). Fires only if the left mouse button has
    /// been released while cursor was over the element.
    LeftMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the middle mouse button has
    /// been released while cursor was over the element.
    MiddleMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the right mouse button has
    /// been released while cursor was over the element.
    RightMouseUp,
    /// Mouse cursor has entered the element.
    MouseEnter,
    /// Mouse cursor has left the element.
    MouseLeave,
    /// Mousewheel / touchpad scrolling.
    Scroll,
    /// The window received a unicode character (also respects the system locale).
    /// Check `keyboard_state.current_char` to get the current pressed character.
    TextInput,
    /// A **virtual keycode** was pressed. Note: This is only the virtual keycode,
    /// not the actual char. If you want to get the character, use `TextInput` instead.
    /// A virtual key does not have to map to a printable character.
    ///
    /// You can get all currently pressed virtual keycodes in the
    /// `keyboard_state.current_virtual_keycodes` and / or just the last keycode in the
    /// `keyboard_state.latest_virtual_keycode`.
    VirtualKeyDown,
    /// A **virtual keycode** was release. See `VirtualKeyDown` for more info.
    VirtualKeyUp,
    /// A file has been dropped on the element.
    HoveredFile,
    /// A file is being hovered on the element.
    DroppedFile,
    /// A file was hovered, but has exited the window.
    HoveredFileCancelled,
    /// Equivalent to `onfocus`.
    FocusReceived,
    /// Equivalent to `onblur`.
    FocusLost,
}

// ============================================================================
// NOTE: EventFilter types moved to core/src/events.rs (Phase 3.5)
//
// The following types are now defined in events.rs and re-exported above:
// - EventFilter
// - HoverEventFilter
// - FocusEventFilter
// - WindowEventFilter
// - NotEventFilter
// - ComponentEventFilter
// - ApplicationEventFilter
//
// This consolidates all event-related logic in one place.
// ============================================================================

/// Contains the necessary information to render an embedded `IFrame` node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IFrameNode {
    /// The callback function that returns the DOM for the iframe's content.
    pub callback: IFrameCallback,
    /// The application data passed to the iframe's layout callback.
    pub data: RefAny,
}

/// An enum that holds either a CSS ID or a class name as a string.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IdOrClass {
    Id(AzString),
    Class(AzString),
}

impl_vec!(IdOrClass, IdOrClassVec, IdOrClassVecDestructor);
impl_vec_debug!(IdOrClass, IdOrClassVec);
impl_vec_partialord!(IdOrClass, IdOrClassVec);
impl_vec_ord!(IdOrClass, IdOrClassVec);
impl_vec_clone!(IdOrClass, IdOrClassVec, IdOrClassVecDestructor);
impl_vec_partialeq!(IdOrClass, IdOrClassVec);
impl_vec_eq!(IdOrClass, IdOrClassVec);
impl_vec_hash!(IdOrClass, IdOrClassVec);

impl IdOrClass {
    pub fn as_id(&self) -> Option<&str> {
        match self {
            IdOrClass::Id(s) => Some(s.as_str()),
            IdOrClass::Class(_) => None,
        }
    }
    pub fn as_class(&self) -> Option<&str> {
        match self {
            IdOrClass::Class(s) => Some(s.as_str()),
            IdOrClass::Id(_) => None,
        }
    }
}

// memory optimization: store all inline-normal / inline-hover / inline-* attributes
// as one Vec instad of 4 separate Vecs

/// Represents an inline CSS property attached to a node for a specific interaction state.
/// This allows defining styles for `:hover`, `:focus`, etc., directly on a DOM node.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeDataInlineCssProperty {
    /// A standard, non-interactive style property.
    /// - **CSS Equivalent**: ` ` (no pseudo-class)
    Normal(CssProperty),

    /// A style property that applies when the element is active (e.g., being clicked).
    /// - **CSS Equivalent**: `:active`
    Active(CssProperty),

    /// A style property that applies when the element has focus.
    /// - **CSS Equivalent**: `:focus`
    Focus(CssProperty),

    /// A style property that applies when the element is being hovered by the mouse.
    /// - **CSS Equivalent**: `:hover`
    Hover(CssProperty),

    /// A style property that applies when the element is disabled and cannot be interacted with.
    /// - **CSS Equivalent**: `:disabled`
    Disabled(CssProperty),

    /// A style property that applies when the element is checked (e.g., a checkbox or radio
    /// button).
    /// - **CSS Equivalent**: `:checked`
    Checked(CssProperty),

    /// A style property that applies when the element or one of its descendants has focus.
    /// - **CSS Equivalent**: `:focus-within`
    FocusWithin(CssProperty),

    /// A style property that applies to a link that has been visited.
    /// - **CSS Equivalent**: `:visited`
    Visited(CssProperty),
}

macro_rules! parse_from_str {
    ($s:expr, $prop_type:ident) => {{
        use azul_css::{css::CssDeclaration, parser2::ErrorLocation, props::property::CssKeyMap};

        let s = $s.trim();
        let css_key_map = CssKeyMap::get();

        let v = s
            .split(";")
            .filter_map(|kv| {
                let mut kv_iter = kv.split(":");
                let key = kv_iter.next()?;
                let value = kv_iter.next()?;
                let mut declarations = Vec::new();
                let mut warnings = Vec::new();

                azul_css::parser2::parse_css_declaration(
                    key,
                    value,
                    (ErrorLocation::default(), ErrorLocation::default()),
                    &css_key_map,
                    &mut warnings,
                    &mut declarations,
                )
                .ok()?;

                let declarations = declarations
                    .iter()
                    .filter_map(|c| match c {
                        CssDeclaration::Static(d) => {
                            Some(NodeDataInlineCssProperty::$prop_type(d.clone()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();

                if declarations.is_empty() {
                    None
                } else {
                    Some(declarations)
                }
            })
            .collect::<Vec<Vec<NodeDataInlineCssProperty>>>();

        v.into_iter()
            .flat_map(|k| k.into_iter())
            .collect::<Vec<_>>()
            .into()
    }};
}

impl NodeDataInlineCssPropertyVec {
    // given "flex-direction: row", returns
    // vec![NodeDataInlineCssProperty::Normal(FlexDirection::Row)]
    pub fn parse_normal(s: &str) -> Self {
        return parse_from_str!(s, Normal);
    }

    // given "flex-direction: row", returns
    // vec![NodeDataInlineCssProperty::Hover(FlexDirection::Row)]
    pub fn parse_hover(s: &str) -> Self {
        return parse_from_str!(s, Hover);
    }

    // given "flex-direction: row", returns
    // vec![NodeDataInlineCssProperty::Active(FlexDirection::Row)]
    pub fn parse_active(s: &str) -> Self {
        return parse_from_str!(s, Active);
    }

    // given "flex-direction: row", returns
    // vec![NodeDataInlineCssProperty::Focus(FlexDirection::Row)]
    pub fn parse_focus(s: &str) -> Self {
        return parse_from_str!(s, Focus);
    }

    // appends two NodeDataInlineCssPropertyVec, even if both are &'static arrays
    pub fn with_append(&self, mut other: Self) -> Self {
        let mut m = self.clone().into_library_owned_vec();
        m.append(&mut other.into_library_owned_vec());
        m.into()
    }
}

impl fmt::Debug for NodeDataInlineCssProperty {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::NodeDataInlineCssProperty::*;
        match self {
            Normal(p) => write!(f, "Normal({}: {})", p.key(), p.value()),
            Active(p) => write!(f, "Active({}: {})", p.key(), p.value()),
            Focus(p) => write!(f, "Focus({}: {})", p.key(), p.value()),
            Hover(p) => write!(f, "Hover({}: {})", p.key(), p.value()),
            Disabled(p) => write!(f, "Disabled({}: {})", p.key(), p.value()),
            Checked(p) => write!(f, "Checked({}: {})", p.key(), p.value()),
            FocusWithin(p) => write!(f, "FocusWithin({}: {})", p.key(), p.value()),
            Visited(p) => write!(f, "Visited({}: {})", p.key(), p.value()),
        }
    }
}

impl_vec!(
    NodeDataInlineCssProperty,
    NodeDataInlineCssPropertyVec,
    NodeDataInlineCssPropertyVecDestructor
);
impl_vec_debug!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_partialord!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_ord!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_clone!(
    NodeDataInlineCssProperty,
    NodeDataInlineCssPropertyVec,
    NodeDataInlineCssPropertyVecDestructor
);
impl_vec_partialeq!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_eq!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_hash!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);

/// Represents all data associated with a single DOM node, such as its type,
/// classes, IDs, callbacks, and inline styles.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeData {
    /// `div`, `p`, `img`, etc.
    pub(crate) node_type: NodeType,
    /// `data-*` attributes for this node, useful to store UI-related data on the node itself.
    pub(crate) dataset: OptionRefAny,
    /// Stores all ids and classes as one vec - size optimization since
    /// most nodes don't have any classes or IDs.
    pub(crate) ids_and_classes: IdOrClassVec,
    /// Callbacks attached to this node:
    ///
    /// `On::MouseUp` -> `Callback(my_button_click_handler)`
    pub(crate) callbacks: CoreCallbackDataVec,
    /// Stores the inline CSS properties, same as in HTML.
    pub(crate) inline_css_props: NodeDataInlineCssPropertyVec,
    /// Tab index (commonly used property).
    pub(crate) tab_index: OptionTabIndex,
    /// Stores "extra", not commonly used data of the node: accessibility, clip-mask, tab-index,
    /// etc.
    ///
    /// SHOULD NOT EXPOSED IN THE API - necessary to retroactively add functionality
    /// to the node without breaking the ABI.
    extra: Option<Box<NodeDataExt>>,
}

impl Hash for NodeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.dataset.hash(state);
        self.ids_and_classes.as_ref().hash(state);

        // NOTE: callbacks are NOT hashed regularly, otherwise
        // they'd cause inconsistencies because of the scroll callback
        for callback in self.callbacks.as_ref().iter() {
            callback.event.hash(state);
            callback.callback.hash(state);
            callback.data.get_type_id().hash(state);
        }

        self.inline_css_props.as_ref().hash(state);
        if let Some(ext) = self.extra.as_ref() {
            if let Some(c) = ext.clip_mask.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.accessibility.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.menu_bar.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.context_menu.as_ref() {
                c.hash(state);
            }
        }
    }
}

/// NOTE: NOT EXPOSED IN THE API! Stores extra,
/// not commonly used information for the NodeData.
/// This helps keep the primary `NodeData` struct smaller for common cases.
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct NodeDataExt {
    /// Optional clip mask for this DOM node.
    pub(crate) clip_mask: Option<ImageMask>,
    /// Optional extra accessibility information about this DOM node (MSAA, AT-SPI, UA).
    pub(crate) accessibility: Option<Box<AccessibilityInfo>>,
    /// Menu bar that should be displayed at the top of this nodes rect.
    pub(crate) menu_bar: Option<Box<Menu>>,
    /// Context menu that should be opened when the item is left-clicked.
    pub(crate) context_menu: Option<Box<Menu>>,
    // ... insert further API extensions here...
}

/// Holds information about a UI element for accessibility purposes (e.g., screen readers).
/// This is a wrapper for platform-specific accessibility APIs like MSAA.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct AccessibilityInfo {
    /// Get the "name" of the `IAccessible`, for example the
    /// name of a button, checkbox or menu item. Try to use unique names
    /// for each item in a dialog so that voice dictation software doesn't
    /// have to deal with extra ambiguity.
    pub name: OptionAzString,
    /// Get the "value" of the `IAccessible`, for example a number in a slider,
    /// a URL for a link, the text a user entered in a field.
    pub value: OptionAzString,
    /// Get an enumerated value representing what this IAccessible is used for,
    /// for example is it a link, static text, editable text, a checkbox, or a table cell, etc.
    pub role: AccessibilityRole,
    /// Possible on/off states, such as focused, focusable, selected, selectable,
    /// visible, protected (for passwords), checked, etc.
    pub states: AccessibilityStateVec,
    /// Optional keyboard accelerator.
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional "default action" description. Only used when there is at least
    /// one `ComponentEventFilter::DefaultAction` callback present on this node.
    pub default_action: OptionAzString,
}

/// Defines the element's purpose for accessibility APIs, informing assistive technologies
/// like screen readers about the function of a UI element. Each variant corresponds to a
/// standard control type or UI structure.
///
/// For more details, see the [MSDN Role Constants page](https://docs.microsoft.com/en-us/windows/winauto/object-roles).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AccessibilityRole {
    /// Represents the title or caption bar of a window.
    /// - **Purpose**: To identify the title bar containing the window title and system commands.
    /// - **When to use**: This role is typically inserted by the operating system for standard
    ///   windows.
    /// - **Example**: The bar at the top of an application window displaying its name and the
    ///   minimize, maximize, and close buttons.
    TitleBar,

    /// Represents a menu bar at the top of a window.
    /// - **Purpose**: To contain a set of top-level menus for an application.
    /// - **When to use**: For the main menu bar of an application, such as one containing "File,"
    ///   "Edit," and "View."
    /// - **Example**: The "File", "Edit", "View" menu bar at the top of a text editor.
    MenuBar,

    /// Represents a vertical or horizontal scroll bar.
    /// - **Purpose**: To enable scrolling through content that is larger than the visible area.
    /// - **When to use**: For any scrollable region of content.
    /// - **Example**: The bar on the side of a web page that allows the user to scroll up and
    ///   down.
    ScrollBar,

    /// Represents a handle or grip used for moving or resizing.
    /// - **Purpose**: To provide a user interface element for manipulating another element's size
    ///   or position.
    /// - **When to use**: For handles that allow resizing of windows, panes, or other objects.
    /// - **Example**: The small textured area in the bottom-right corner of a window that can be
    ///   dragged to resize it.
    Grip,

    /// Represents a system sound indicating an event.
    /// - **Purpose**: To associate a sound with a UI event, providing an auditory cue.
    /// - **When to use**: When a sound is the primary representation of an event.
    /// - **Example**: A system notification sound that plays when a new message arrives.
    Sound,

    /// Represents the system's mouse pointer or other pointing device.
    /// - **Purpose**: To indicate the screen position of the user's pointing device.
    /// - **When to use**: This role is managed by the operating system.
    /// - **Example**: The arrow that moves on the screen as you move the mouse.
    Cursor,

    /// Represents the text insertion point indicator.
    /// - **Purpose**: To show the current text entry or editing position.
    /// - **When to use**: This role is typically managed by the operating system for text input
    ///   fields.
    /// - **Example**: The blinking vertical line in a text box that shows where the next character
    ///   will be typed.
    Caret,

    /// Represents an alert or notification.
    /// - **Purpose**: To convey an important, non-modal message to the user.
    /// - **When to use**: For non-intrusive notifications that do not require immediate user
    ///   interaction.
    /// - **Example**: A small, temporary "toast" notification that appears to confirm an action,
    ///   like "Email sent."
    Alert,

    /// Represents a window frame.
    /// - **Purpose**: To serve as the container for other objects like a title bar and client
    ///   area.
    /// - **When to use**: This is a fundamental role, typically managed by the windowing system.
    /// - **Example**: The main window of any application, which contains all other UI elements.
    Window,

    /// Represents a window's client area, where the main content is displayed.
    /// - **Purpose**: To define the primary content area of a window.
    /// - **When to use**: For the main content region of a window. It's often the default role for
    ///   a custom control container.
    /// - **Example**: The area of a web browser where the web page content is rendered.
    Client,

    /// Represents a pop-up menu.
    /// - **Purpose**: To display a list of `MenuItem` objects that appears when a user performs an
    ///   action.
    /// - **When to use**: For context menus (right-click menus) or drop-down menus.
    /// - **Example**: The menu that appears when you right-click on a file in a file explorer.
    MenuPopup,

    /// Represents an individual item within a menu.
    /// - **Purpose**: To represent a single command, option, or separator within a menu.
    /// - **When to use**: For individual options inside a `MenuBar` or `MenuPopup`.
    /// - **Example**: The "Save" option within the "File" menu.
    MenuItem,

    /// Represents a small pop-up window that provides information.
    /// - **Purpose**: To offer brief, contextual help or information about a UI element.
    /// - **When to use**: For informational pop-ups that appear on mouse hover.
    /// - **Example**: The small box of text that appears when you hover over a button in a
    ///   toolbar.
    Tooltip,

    /// Represents the main window of an application.
    /// - **Purpose**: To identify the top-level window of an application.
    /// - **When to use**: For the primary window that represents the application itself.
    /// - **Example**: The main window of a calculator or notepad application.
    Application,

    /// Represents a document window within an application.
    /// - **Purpose**: To represent a contained document, typically in a Multiple Document
    ///   Interface (MDI) application.
    /// - **When to use**: For individual document windows inside a larger application shell.
    /// - **Example**: In a photo editor that allows multiple images to be open in separate
    ///   windows, each image window would be a `Document`.
    Document,

    /// Represents a pane or a distinct section of a window.
    /// - **Purpose**: To divide a window into visually and functionally distinct areas.
    /// - **When to use**: For sub-regions of a window, like a navigation pane, preview pane, or
    ///   sidebar.
    /// - **Example**: The preview pane in an email client that shows the content of the selected
    ///   email.
    Pane,

    /// Represents a graphical chart or graph.
    /// - **Purpose**: To display data visually in a chart format.
    /// - **When to use**: For any type of chart, such as a bar chart, line chart, or pie chart.
    /// - **Example**: A bar chart displaying monthly sales figures.
    Chart,

    /// Represents a dialog box or message box.
    /// - **Purpose**: To create a secondary window that requires user interaction before returning
    ///   to the main application.
    /// - **When to use**: For modal or non-modal windows that prompt the user for information or a
    ///   response.
    /// - **Example**: The "Open File" or "Print" dialog in most applications.
    Dialog,

    /// Represents a window's border.
    /// - **Purpose**: To identify the border of a window, which is often used for resizing.
    /// - **When to use**: This role is typically managed by the windowing system.
    /// - **Example**: The decorative and functional frame around a window.
    Border,

    /// Represents a group of related controls.
    /// - **Purpose**: To logically group other objects that share a common purpose.
    /// - **When to use**: For grouping controls like a set of radio buttons or a fieldset with a
    ///   legend.
    /// - **Example**: A "Settings" group box in a dialog that contains several related checkboxes.
    Grouping,

    /// Represents a visual separator.
    /// - **Purpose**: To visually divide a space or a group of controls.
    /// - **When to use**: For visual separators in menus, toolbars, or between panes.
    /// - **Example**: The horizontal line in a menu that separates groups of related menu items.
    Separator,

    /// Represents a toolbar containing a group of controls.
    /// - **Purpose**: To group controls, typically buttons, for quick access to frequently used
    ///   functions.
    /// - **When to use**: For a bar of buttons or other controls, usually at the top of a window
    ///   or pane.
    /// - **Example**: The toolbar at the top of a word processor with buttons for "Bold,"
    ///   "Italic," and "Underline."
    Toolbar,

    /// Represents a status bar for displaying information.
    /// - **Purpose**: To display status information about the current state of the application.
    /// - **When to use**: For a bar, typically at the bottom of a window, that displays messages.
    /// - **Example**: The bar at the bottom of a web browser that shows the loading status of a
    ///   page.
    StatusBar,

    /// Represents a data table.
    /// - **Purpose**: To present data in a two-dimensional grid of rows and columns.
    /// - **When to use**: For grid-like data presentation.
    /// - **Example**: A spreadsheet or a table of data in a database application.
    Table,

    /// Represents a column header in a table.
    /// - **Purpose**: To provide a label for a column of data.
    /// - **When to use**: For the headers of columns in a `Table`.
    /// - **Example**: The header row in a spreadsheet with labels like "Name," "Date," and
    ///   "Amount."
    ColumnHeader,

    /// Represents a row header in a table.
    /// - **Purpose**: To provide a label for a row of data.
    /// - **When to use**: For the headers of rows in a `Table`.
    /// - **Example**: The numbered rows on the left side of a spreadsheet.
    RowHeader,

    /// Represents a full column of cells in a table.
    /// - **Purpose**: To represent an entire column as a single accessible object.
    /// - **When to use**: When it is useful to interact with a column as a whole.
    /// - **Example**: The "Amount" column in a financial data table.
    Column,

    /// Represents a full row of cells in a table.
    /// - **Purpose**: To represent an entire row as a single accessible object.
    /// - **When to use**: When it is useful to interact with a row as a whole.
    /// - **Example**: A row representing a single customer's information in a customer list.
    Row,

    /// Represents a single cell within a table.
    /// - **Purpose**: To represent a single data point or control within a `Table`.
    /// - **When to use**: For individual cells in a grid or table.
    /// - **Example**: A single cell in a spreadsheet containing a specific value.
    Cell,

    /// Represents a hyperlink to a resource.
    /// - **Purpose**: To provide a navigational link to another document or location.
    /// - **When to use**: For text or images that, when clicked, navigate to another resource.
    /// - **Example**: A clickable link on a web page.
    Link,

    /// Represents a help balloon or pop-up.
    /// - **Purpose**: To provide more detailed help information than a standard tooltip.
    /// - **When to use**: For a pop-up that offers extended help text, often initiated by a help
    ///   button.
    /// - **Example**: A pop-up balloon with a paragraph of help text that appears when a user
    ///   clicks a help icon.
    HelpBalloon,

    /// Represents an animated, character-like graphic object.
    /// - **Purpose**: To provide an animated agent for user assistance or entertainment.
    /// - **When to use**: For animated characters or avatars that provide help or guidance.
    /// - **Example**: An animated paperclip that offers tips in a word processor (e.g.,
    ///   Microsoft's Clippy).
    Character,

    /// Represents a list of items.
    /// - **Purpose**: To contain a set of `ListItem` objects.
    /// - **When to use**: For list boxes or similar controls that present a list of selectable
    ///   items.
    /// - **Example**: The list of files in a file selection dialog.
    List,

    /// Represents an individual item within a list.
    /// - **Purpose**: To represent a single, selectable item within a `List`.
    /// - **When to use**: For each individual item in a list box or combo box.
    /// - **Example**: A single file name in a list of files.
    ListItem,

    /// Represents an outline or tree structure.
    /// - **Purpose**: To display a hierarchical view of data.
    /// - **When to use**: For tree-view controls that show nested items.
    /// - **Example**: A file explorer's folder tree view.
    Outline,

    /// Represents an individual item within an outline or tree.
    /// - **Purpose**: To represent a single node (which can be a leaf or a branch) in an
    ///   `Outline`.
    /// - **When to use**: For each node in a tree view.
    /// - **Example**: A single folder in a file explorer's tree view.
    OutlineItem,

    /// Represents a single tab in a tabbed interface.
    /// - **Purpose**: To provide a control for switching between different `PropertyPage` views.
    /// - **When to use**: For the individual tabs that the user can click to switch pages.
    /// - **Example**: The "General" and "Security" tabs in a file properties dialog.
    PageTab,

    /// Represents the content of a page in a property sheet.
    /// - **Purpose**: To serve as a container for the controls displayed when a `PageTab` is
    ///   selected.
    /// - **When to use**: For the content area associated with a specific tab.
    /// - **Example**: The set of options displayed when the "Security" tab is active.
    PropertyPage,

    /// Represents a visual indicator, like a slider thumb.
    /// - **Purpose**: To visually indicate the current value or position of another control.
    /// - **When to use**: For a sub-element that indicates status, like the thumb of a scrollbar.
    /// - **Example**: The draggable thumb of a scrollbar that indicates the current scroll
    ///   position.
    Indicator,

    /// Represents a picture or graphical image.
    /// - **Purpose**: To display a non-interactive image.
    /// - **When to use**: For images and icons that are purely decorative or informational.
    /// - **Example**: A company logo displayed in an application's "About" dialog.
    Graphic,

    /// Represents read-only text.
    /// - **Purpose**: To provide a non-editable text label for another control or for displaying
    ///   information.
    /// - **When to use**: For text that the user cannot edit.
    /// - **Example**: The label "Username:" next to a text input field.
    StaticText,

    /// Represents editable text or a text area.
    /// - **Purpose**: To allow for user text input or selection.
    /// - **When to use**: For text input fields where the user can type.
    /// - **Example**: A text box for entering a username or password.
    Text,

    /// Represents a standard push button.
    /// - **Purpose**: To initiate an immediate action.
    /// - **When to use**: For standard buttons that perform an action when clicked.
    /// - **Example**: An "OK" or "Cancel" button in a dialog.
    PushButton,

    /// Represents a check box control.
    /// - **Purpose**: To allow the user to make a binary choice (checked or unchecked).
    /// - **When to use**: For options that can be toggled on or off independently.
    /// - **Example**: A "Remember me" checkbox on a login form.
    CheckButton,

    /// Represents a radio button.
    /// - **Purpose**: To allow the user to select one option from a mutually exclusive group.
    /// - **When to use**: For a choice where only one option from a `Grouping` can be selected.
    /// - **Example**: "Male" and "Female" radio buttons for selecting gender.
    RadioButton,

    /// Represents a combination of a text field and a drop-down list.
    /// - **Purpose**: To allow the user to either type a value or select one from a list.
    /// - **When to use**: For controls that offer a list of suggestions but also allow custom
    ///   input.
    /// - **Example**: A font selector that allows you to type a font name or choose one from a
    ///   list.
    ComboBox,

    /// Represents a drop-down list box.
    /// - **Purpose**: To allow the user to select an item from a non-editable list that drops
    ///   down.
    /// - **When to use**: For selecting a single item from a predefined list of options.
    /// - **Example**: A country selection drop-down menu.
    DropList,

    /// Represents a progress bar.
    /// - **Purpose**: To indicate the progress of a lengthy operation.
    /// - **When to use**: To provide feedback for tasks like file downloads or installations.
    /// - **Example**: The bar that fills up to show the progress of a file copy operation.
    ProgressBar,

    /// Represents a dial or knob.
    /// - **Purpose**: To allow selecting a value from a continuous or discrete range, often
    ///   circularly.
    /// - **When to use**: For controls that resemble real-world dials, like a volume knob.
    /// - **Example**: A volume control knob in a media player application.
    Dial,

    /// Represents a control for entering a keyboard shortcut.
    /// - **Purpose**: To capture a key combination from the user.
    /// - **When to use**: In settings where users can define their own keyboard shortcuts.
    /// - **Example**: A text field in a settings dialog where a user can press a key combination
    ///   to assign it to a command.
    HotkeyField,

    /// Represents a slider for selecting a value within a range.
    /// - **Purpose**: To allow the user to adjust a setting along a continuous or discrete range.
    /// - **When to use**: For adjusting values like volume, brightness, or zoom level.
    /// - **Example**: A slider to control the volume of a video.
    Slider,

    /// Represents a spin button (up/down arrows) for incrementing or decrementing a value.
    /// - **Purpose**: To provide fine-tuned adjustment of a value, typically numeric.
    /// - **When to use**: For controls that allow stepping through a range of values.
    /// - **Example**: The up and down arrows next to a number input for setting the font size.
    SpinButton,

    /// Represents a diagram or flowchart.
    /// - **Purpose**: To represent data or relationships in a schematic form.
    /// - **When to use**: For visual representations of structures that are not charts, like a
    ///   database schema diagram.
    /// - **Example**: A flowchart illustrating a business process.
    Diagram,

    /// Represents an animation control.
    /// - **Purpose**: To display a sequence of images or indicate an ongoing process.
    /// - **When to use**: For animations that show that an operation is in progress.
    /// - **Example**: The animation that plays while files are being copied.
    Animation,

    /// Represents a mathematical equation.
    /// - **Purpose**: To display a mathematical formula in the correct format.
    /// - **When to use**: For displaying mathematical equations.
    /// - **Example**: A rendered mathematical equation in a scientific document editor.
    Equation,

    /// Represents a button that drops down a list of items.
    /// - **Purpose**: To combine a default action button with a list of alternative actions.
    /// - **When to use**: For buttons that have a primary action and a secondary list of options.
    /// - **Example**: A "Send" button with a dropdown arrow that reveals "Send and Archive."
    ButtonDropdown,

    /// Represents a button that drops down a full menu.
    /// - **Purpose**: To provide a button that opens a menu of choices rather than performing a
    ///   single action.
    /// - **When to use**: When a button's primary purpose is to reveal a menu.
    /// - **Example**: A "Tools" button that opens a menu with various tool options.
    ButtonMenu,

    /// Represents a button that drops down a grid for selection.
    /// - **Purpose**: To allow selection from a two-dimensional grid of options.
    /// - **When to use**: For buttons that open a grid-based selection UI.
    /// - **Example**: A color picker button that opens a grid of color swatches.
    ButtonDropdownGrid,

    /// Represents blank space between other objects.
    /// - **Purpose**: To represent significant empty areas in a UI that are part of the layout.
    /// - **When to use**: Sparingly, to signify that a large area is intentionally blank.
    /// - **Example**: A large empty panel in a complex layout might use this role.
    Whitespace,

    /// Represents the container for a set of tabs.
    /// - **Purpose**: To group a set of `PageTab` elements.
    /// - **When to use**: To act as the parent container for a row or column of tabs.
    /// - **Example**: The entire row of tabs at the top of a properties dialog.
    PageTabList,

    /// Represents a clock control.
    /// - **Purpose**: To display the current time.
    /// - **When to use**: For any UI element that displays time.
    /// - **Example**: The clock in the system tray of the operating system.
    Clock,

    /// Represents a button with two parts: a default action and a dropdown.
    /// - **Purpose**: To combine a frequently used action with a set of related, less-used
    ///   actions.
    /// - **When to use**: When a button has a default action and other related actions available
    ///   in a dropdown.
    /// - **Example**: A "Save" split button where the primary part saves, and the dropdown offers
    ///   "Save As."
    SplitButton,

    /// Represents a control for entering an IP address.
    /// - **Purpose**: To provide a specialized input field for IP addresses, often with formatting
    ///   and validation.
    /// - **When to use**: For dedicated IP address input fields.
    /// - **Example**: A network configuration dialog with a field for entering a static IP
    ///   address.
    IpAddress,

    /// Represents an element with no specific role.
    /// - **Purpose**: To indicate an element that has no semantic meaning for accessibility.
    /// - **When to use**: Should be used sparingly for purely decorative elements that should be
    ///   ignored by assistive technologies.
    /// - **Example**: A decorative graphical flourish that has no function or information to
    ///   convey.
    Nothing,
}

/// Defines the current state of an element for accessibility APIs (e.g., focused, checked).
/// These states provide dynamic information to assistive technologies about the element's
/// condition.
///
/// See the [MSDN State Constants page](https://docs.microsoft.com/en-us/windows/win32/winauto/object-state-constants) for more details.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum AccessibilityState {
    /// The element is unavailable and cannot be interacted with.
    /// - **Purpose**: To indicate that a control is disabled or grayed out.
    /// - **When to use**: For disabled buttons, non-interactive menu items, or any control that is
    ///   temporarily non-functional.
    /// - **Example**: A "Save" button that is disabled until the user makes changes to a document.
    Unavailable,

    /// The element is selected.
    /// - **Purpose**: To indicate that an item is currently chosen or highlighted. This is
    ///   distinct from having focus.
    /// - **When to use**: For selected items in a list, highlighted text, or the currently active
    ///   tab in a tab list.
    /// - **Example**: A file highlighted in a file explorer, or multiple selected emails in an
    ///   inbox.
    Selected,

    /// The element has the keyboard focus.
    /// - **Purpose**: To identify the single element that will receive keyboard input.
    /// - **When to use**: For the control that is currently active and ready to be manipulated by
    ///   the keyboard.
    /// - **Example**: A text box with a blinking cursor, or a button with a dotted outline around
    ///   it.
    Focused,

    /// The element is checked, toggled, or in a mixed state.
    /// - **Purpose**: To represent the state of controls like checkboxes, radio buttons, and
    ///   toggle buttons.
    /// - **When to use**: For checkboxes that are ticked, selected radio buttons, or toggle
    ///   buttons that are "on."
    /// - **Example**: A checked "I agree" checkbox, a selected "Yes" radio button, or an active
    ///   "Bold" button in a toolbar.
    Checked,

    /// The element's content cannot be edited by the user.
    /// - **Purpose**: To indicate that the element's value can be viewed and copied, but not
    ///   modified.
    /// - **When to use**: For display-only text fields or documents.
    /// - **Example**: A text box displaying a license agreement that the user can scroll through
    ///   but cannot edit.
    Readonly,

    /// The element is the default action in a dialog or form.
    /// - **Purpose**: To identify the button that will be activated if the user presses the Enter
    ///   key.
    /// - **When to use**: For the primary confirmation button in a dialog.
    /// - **Example**: The "OK" button in a dialog box, which often has a thicker or colored
    ///   border.
    Default,

    /// The element is expanded, showing its child items.
    /// - **Purpose**: To indicate that a collapsible element is currently open and its contents
    ///   are visible.
    /// - **When to use**: For tree view nodes, combo boxes with their lists open, or expanded
    ///   accordion panels.
    /// - **Example**: A folder in a file explorer's tree view that has been clicked to show its
    ///   subfolders.
    Expanded,

    /// The element is collapsed, hiding its child items.
    /// - **Purpose**: To indicate that a collapsible element is closed and its contents are
    ///   hidden.
    /// - **When to use**: The counterpart to `Expanded` for any collapsible UI element.
    /// - **Example**: A closed folder in a file explorer's tree view, hiding its contents.
    Collapsed,

    /// The element is busy and cannot respond to user interaction.
    /// - **Purpose**: To indicate that the element or application is performing an operation and
    ///   is temporarily unresponsive.
    /// - **When to use**: When an application is loading, processing data, or otherwise occupied.
    /// - **Example**: A window that is grayed out and shows a spinning cursor while saving a large
    ///   file.
    Busy,

    /// The element is not currently visible on the screen.
    /// - **Purpose**: To indicate that an element exists but is currently scrolled out of the
    ///   visible area.
    /// - **When to use**: For items in a long list or a large document that are not within the
    ///   current viewport.
    /// - **Example**: A list item in a long dropdown that you would have to scroll down to see.
    Offscreen,

    /// The element can accept keyboard focus.
    /// - **Purpose**: To indicate that the user can navigate to this element using the keyboard
    ///   (e.g., with the Tab key).
    /// - **When to use**: On all interactive elements like buttons, links, and input fields,
    ///   whether they currently have focus or not.
    /// - **Example**: A button that can receive focus, even if it is not the currently focused
    ///   element.
    Focusable,

    /// The element is a container whose children can be selected.
    /// - **Purpose**: To indicate that the element contains items that can be chosen.
    /// - **When to use**: On container controls like list boxes, tree views, or text spans where
    ///   text can be highlighted.
    /// - **Example**: A list box control is `Selectable`, while its individual list items have the
    ///   `Selected` state when chosen.
    Selectable,

    /// The element is a hyperlink.
    /// - **Purpose**: To identify an object that navigates to another resource or location when
    ///   activated.
    /// - **When to use**: On any object that functions as a hyperlink.
    /// - **Example**: Text or an image that, when clicked, opens a web page.
    Linked,

    /// The element is a hyperlink that has been visited.
    /// - **Purpose**: To indicate that a hyperlink has already been followed by the user.
    /// - **When to use**: On a `Linked` object that the user has previously activated.
    /// - **Example**: A hyperlink on a web page that has changed color to show it has been
    ///   visited.
    Traversed,

    /// The element allows multiple of its children to be selected at once.
    /// - **Purpose**: To indicate that a container control supports multi-selection.
    /// - **When to use**: On container controls like list boxes or file explorers that support
    ///   multiple selections (e.g., with Ctrl-click).
    /// - **Example**: A file list that allows the user to select several files at once for a copy
    ///   operation.
    Multiselectable,

    /// The element contains protected content that should not be read aloud.
    /// - **Purpose**: To prevent assistive technologies from speaking the content of a sensitive
    ///   field.
    /// - **When to use**: Primarily for password input fields.
    /// - **Example**: A password text box where typed characters are masked with asterisks or
    ///   dots.
    Protected,
}

impl_vec!(
    AccessibilityState,
    AccessibilityStateVec,
    AccessibilityStateVecDestructor
);
impl_vec_clone!(
    AccessibilityState,
    AccessibilityStateVec,
    AccessibilityStateVecDestructor
);
impl_vec_debug!(AccessibilityState, AccessibilityStateVec);
impl_vec_partialeq!(AccessibilityState, AccessibilityStateVec);
impl_vec_partialord!(AccessibilityState, AccessibilityStateVec);
impl_vec_eq!(AccessibilityState, AccessibilityStateVec);
impl_vec_ord!(AccessibilityState, AccessibilityStateVec);
impl_vec_hash!(AccessibilityState, AccessibilityStateVec);

impl Clone for NodeData {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            node_type: self.node_type.into_library_owned_nodetype(),
            dataset: match &self.dataset {
                OptionRefAny::None => OptionRefAny::None,
                OptionRefAny::Some(s) => OptionRefAny::Some(s.clone()),
            },
            ids_and_classes: self.ids_and_classes.clone(), /* do not clone the IDs and classes if
                                                            * they are &'static */
            inline_css_props: self.inline_css_props.clone(), /* do not clone the inline CSS props
                                                              * if they are &'static */
            callbacks: self.callbacks.clone(),
            tab_index: self.tab_index,
            extra: self.extra.clone(),
        }
    }
}

// Clone, PartialEq, Eq, Hash, PartialOrd, Ord
impl_vec!(NodeData, NodeDataVec, NodeDataVecDestructor);
impl_vec_clone!(NodeData, NodeDataVec, NodeDataVecDestructor);
impl_vec_mut!(NodeData, NodeDataVec);
impl_vec_debug!(NodeData, NodeDataVec);
impl_vec_partialord!(NodeData, NodeDataVec);
impl_vec_ord!(NodeData, NodeDataVec);
impl_vec_partialeq!(NodeData, NodeDataVec);
impl_vec_eq!(NodeData, NodeDataVec);
impl_vec_hash!(NodeData, NodeDataVec);

impl NodeDataVec {
    #[inline]
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeData> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    #[inline]
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeData> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

unsafe impl Send for NodeData {}

/// Determines the behavior of an element in sequential focus navigation
// (e.g., using the Tab key).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum TabIndex {
    /// Automatic tab index, similar to simply setting `focusable = "true"` or `tabindex = 0`
    /// (both have the effect of making the element focusable).
    ///
    /// Sidenote: See https://www.w3.org/TR/html5/editing.html#sequential-focus-navigation-and-the-tabindex-attribute
    /// for interesting notes on tabindex and accessibility
    Auto,
    /// Set the tab index in relation to its parent element. I.e. if you have a list of elements,
    /// the focusing order is restricted to the current parent.
    ///
    /// Ex. a div might have:
    ///
    /// ```no_run,ignore
    /// div (Auto)
    /// |- element1 (OverrideInParent 0) <- current focus
    /// |- element2 (OverrideInParent 5)
    /// |- element3 (OverrideInParent 2)
    /// |- element4 (Global 5)
    /// ```
    ///
    /// When pressing tab repeatedly, the focusing order will be
    /// "element3, element2, element4, div", since OverrideInParent elements
    /// take precedence among global order.
    OverrideInParent(u32),
    /// Elements can be focused in callbacks, but are not accessible via
    /// keyboard / tab navigation (-1).
    NoKeyboardFocus,
}

impl_option!(
    TabIndex,
    OptionTabIndex,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl TabIndex {
    /// Returns the HTML-compatible number of the `tabindex` element.
    pub fn get_index(&self) -> isize {
        use self::TabIndex::*;
        match self {
            Auto => 0,
            OverrideInParent(x) => *x as isize,
            NoKeyboardFocus => -1,
        }
    }
}

impl Default for TabIndex {
    fn default() -> Self {
        TabIndex::Auto
    }
}

impl Default for NodeData {
    fn default() -> Self {
        NodeData::new(NodeType::Div)
    }
}

impl fmt::Display for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);

        match self.node_type.format() {
            Some(content) => write!(
                f,
                "<{}{}>{}</{}>",
                html_type, attributes_string, content, html_type
            ),
            None => write!(f, "<{}{}/>", html_type, attributes_string),
        }
    }
}

fn node_data_to_string(node_data: &NodeData) -> String {
    let mut id_string = String::new();
    let ids = node_data
        .ids_and_classes
        .as_ref()
        .iter()
        .filter_map(|s| s.as_id())
        .collect::<Vec<_>>()
        .join(" ");

    if !ids.is_empty() {
        id_string = format!(" id=\"{}\" ", ids);
    }

    let mut class_string = String::new();
    let classes = node_data
        .ids_and_classes
        .as_ref()
        .iter()
        .filter_map(|s| s.as_class())
        .collect::<Vec<_>>()
        .join(" ");

    if !classes.is_empty() {
        class_string = format!(" class=\"{}\" ", classes);
    }

    let mut tabindex_string = String::new();
    if let Some(tab_index) = node_data.get_tab_index() {
        tabindex_string = format!(" tabindex=\"{}\" ", tab_index.get_index());
    };

    format!("{}{}{}", id_string, class_string, tabindex_string)
}

impl NodeData {
    /// Creates a new `NodeData` instance from a given `NodeType`.
    #[inline]
    pub const fn new(node_type: NodeType) -> Self {
        Self {
            node_type,
            dataset: OptionRefAny::None,
            ids_and_classes: IdOrClassVec::from_const_slice(&[]),
            callbacks: CoreCallbackDataVec::from_const_slice(&[]),
            inline_css_props: NodeDataInlineCssPropertyVec::from_const_slice(&[]),
            tab_index: OptionTabIndex::None,
            extra: None,
        }
    }

    /// Shorthand for `NodeData::new(NodeType::Body)`.
    #[inline(always)]
    pub const fn body() -> Self {
        Self::new(NodeType::Body)
    }

    /// Shorthand for `NodeData::new(NodeType::Div)`.
    #[inline(always)]
    pub const fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `NodeData::new(NodeType::Br)`.
    #[inline(always)]
    pub const fn br() -> Self {
        Self::new(NodeType::Br)
    }

    /// Shorthand for `NodeData::new(NodeType::Text(value.into()))`.
    #[inline(always)]
    pub fn text<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Text(value.into()))
    }

    /// Shorthand for `NodeData::new(NodeType::Image(image_id))`.
    #[inline(always)]
    pub fn image(image: ImageRef) -> Self {
        Self::new(NodeType::Image(image))
    }

    #[inline(always)]
    pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self {
        Self::new(NodeType::IFrame(IFrameNode {
            callback: IFrameCallback { cb: callback },
            data,
        }))
    }

    /// Checks whether this node is of the given node type (div, image, text).
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType) -> bool {
        self.node_type == searched_type
    }

    /// Checks whether this node has the searched ID attached.
    pub fn has_id(&self, id: &str) -> bool {
        self.ids_and_classes
            .iter()
            .any(|id_or_class| id_or_class.as_id() == Some(id))
    }

    /// Checks whether this node has the searched class attached.
    pub fn has_class(&self, class: &str) -> bool {
        self.ids_and_classes
            .iter()
            .any(|id_or_class| id_or_class.as_class() == Some(class))
    }

    pub fn has_context_menu(&self) -> bool {
        self.extra
            .as_ref()
            .map(|m| m.context_menu.is_some())
            .unwrap_or(false)
    }

    pub fn is_text_node(&self) -> bool {
        match self.node_type {
            NodeType::Text(_) => true,
            _ => false,
        }
    }

    pub fn is_iframe_node(&self) -> bool {
        match self.node_type {
            NodeType::IFrame(_) => true,
            _ => false,
        }
    }

    /// Returns the default CSS display value for this node type.
    /// This is used by the layout engine to determine the initial display mode
    /// before CSS rules are applied.
    pub fn get_default_display(&self) -> azul_css::props::layout::LayoutDisplay {
        use azul_css::props::layout::LayoutDisplay;
        match self.node_type {
            NodeType::Text(_) => LayoutDisplay::Inline,
            NodeType::Body => LayoutDisplay::Block,
            NodeType::Table => LayoutDisplay::Table,
            NodeType::Tr => LayoutDisplay::TableRow,
            NodeType::Td | NodeType::Th => LayoutDisplay::TableCell,
            NodeType::TBody | NodeType::THead | NodeType::TFoot => LayoutDisplay::TableRowGroup,
            // IFrame nodes are replaced elements - display: block by default
            // They fill available space with width/height: 100% (see get_default_width/height)
            NodeType::IFrame(_) => LayoutDisplay::Block,
            NodeType::Div
            | NodeType::P
            | NodeType::H1
            | NodeType::H2
            | NodeType::H3
            | NodeType::H4
            | NodeType::H5
            | NodeType::H6 => LayoutDisplay::Block,
            _ => LayoutDisplay::Inline,
        }
    }

    /// Returns the default CSS width value for this node type.
    /// This is used when no explicit width is set via CSS.
    pub fn get_default_width(&self) -> Option<azul_css::props::layout::LayoutWidth> {
        use azul_css::props::{basic::PixelValue, layout::LayoutWidth};
        match self.node_type {
            // Body and IFrame fill their parent container by default
            NodeType::Body | NodeType::IFrame(_) => {
                Some(LayoutWidth::Px(PixelValue::const_percent(100)))
            }
            _ => None,
        }
    }

    /// Returns the default CSS height value for this node type.
    /// This is used when no explicit height is set via CSS.
    pub fn get_default_height(&self) -> Option<azul_css::props::layout::LayoutHeight> {
        use azul_css::props::{basic::PixelValue, layout::LayoutHeight};
        match self.node_type {
            // Body and IFrame fill their parent container by default
            NodeType::Body | NodeType::IFrame(_) => {
                Some(LayoutHeight::Px(PixelValue::const_percent(100)))
            }
            _ => None,
        }
    }

    // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
    // in the future (which is why the fields are all private).

    #[inline(always)]
    pub const fn get_node_type(&self) -> &NodeType {
        &self.node_type
    }
    #[inline(always)]
    pub fn get_dataset_mut(&mut self) -> &mut OptionRefAny {
        &mut self.dataset
    }
    #[inline(always)]
    pub const fn get_dataset(&self) -> &OptionRefAny {
        &self.dataset
    }
    #[inline(always)]
    pub const fn get_ids_and_classes(&self) -> &IdOrClassVec {
        &self.ids_and_classes
    }
    #[inline(always)]
    pub const fn get_callbacks(&self) -> &CoreCallbackDataVec {
        &self.callbacks
    }
    #[inline(always)]
    pub const fn get_inline_css_props(&self) -> &NodeDataInlineCssPropertyVec {
        &self.inline_css_props
    }

    #[inline]
    pub fn get_clip_mask(&self) -> Option<&ImageMask> {
        self.extra.as_ref().and_then(|e| e.clip_mask.as_ref())
    }
    #[inline]
    pub fn get_tab_index(&self) -> Option<&TabIndex> {
        self.tab_index.as_ref()
    }
    #[inline]
    pub fn get_accessibility_info(&self) -> Option<&Box<AccessibilityInfo>> {
        self.extra.as_ref().and_then(|e| e.accessibility.as_ref())
    }
    #[inline]
    pub fn get_menu_bar(&self) -> Option<&Box<Menu>> {
        self.extra.as_ref().and_then(|e| e.menu_bar.as_ref())
    }
    #[inline]
    pub fn get_context_menu(&self) -> Option<&Box<Menu>> {
        self.extra.as_ref().and_then(|e| e.context_menu.as_ref())
    }

    #[inline(always)]
    pub fn set_node_type(&mut self, node_type: NodeType) {
        self.node_type = node_type;
    }
    #[inline(always)]
    pub fn set_dataset(&mut self, data: OptionRefAny) {
        self.dataset = data;
    }
    #[inline(always)]
    pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec) {
        self.ids_and_classes = ids_and_classes;
    }
    #[inline(always)]
    pub fn set_callbacks(&mut self, callbacks: CoreCallbackDataVec) {
        self.callbacks = callbacks;
    }
    #[inline(always)]
    pub fn set_inline_css_props(&mut self, inline_css_props: NodeDataInlineCssPropertyVec) {
        self.inline_css_props = inline_css_props;
    }
    #[inline]
    pub fn set_clip_mask(&mut self, clip_mask: ImageMask) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .clip_mask = Some(clip_mask);
    }
    #[inline]
    pub fn set_tab_index(&mut self, tab_index: TabIndex) {
        self.tab_index = Some(tab_index).into();
    }
    #[inline]
    pub fn set_accessibility_info(&mut self, accessibility_info: AccessibilityInfo) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .accessibility = Some(Box::new(accessibility_info));
    }
    #[inline]
    pub fn set_menu_bar(&mut self, menu_bar: Menu) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .menu_bar = Some(Box::new(menu_bar));
    }
    #[inline]
    pub fn set_context_menu(&mut self, context_menu: Menu) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .context_menu = Some(Box::new(context_menu));
    }

    #[inline]
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_context_menu(context_menu);
        self
    }

    #[inline]
    pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: CoreCallbackType) {
        let mut v: CoreCallbackDataVec = Vec::new().into();
        mem::swap(&mut v, &mut self.callbacks);
        let mut v = v.into_library_owned_vec();
        v.push(CoreCallbackData {
            event,
            data,
            callback: CoreCallback { cb: callback },
        });
        self.callbacks = v.into();
    }
    #[inline]
    pub fn add_id(&mut self, s: AzString) {
        let mut v: IdOrClassVec = Vec::new().into();
        mem::swap(&mut v, &mut self.ids_and_classes);
        let mut v = v.into_library_owned_vec();
        v.push(IdOrClass::Id(s));
        self.ids_and_classes = v.into();
    }
    #[inline]
    pub fn add_class(&mut self, s: AzString) {
        let mut v: IdOrClassVec = Vec::new().into();
        mem::swap(&mut v, &mut self.ids_and_classes);
        let mut v = v.into_library_owned_vec();
        v.push(IdOrClass::Class(s));
        self.ids_and_classes = v.into();
    }
    #[inline]
    pub fn add_normal_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Normal(p));
        self.inline_css_props = v.into();
    }
    #[inline]
    pub fn add_hover_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Hover(p));
        self.inline_css_props = v.into();
    }
    #[inline]
    pub fn add_active_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Active(p));
        self.inline_css_props = v.into();
    }
    #[inline]
    pub fn add_focus_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Focus(p));
        self.inline_css_props = v.into();
    }

    /// Calculates a deterministic node hash for this node.
    pub fn calculate_node_data_hash(&self) -> DomNodeHash {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        self.hash(&mut hasher);
        let h = hasher.finalize64();
        DomNodeHash(h)
    }

    #[inline(always)]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.set_tab_index(tab_index);
        self
    }
    #[inline(always)]
    pub fn with_dataset(mut self, data: OptionRefAny) -> Self {
        self.dataset = data;
        self
    }
    #[inline(always)]
    pub fn with_ids_and_classes(mut self, ids_and_classes: IdOrClassVec) -> Self {
        self.ids_and_classes = ids_and_classes;
        self
    }
    #[inline(always)]
    pub fn with_callbacks(mut self, callbacks: CoreCallbackDataVec) -> Self {
        self.callbacks = callbacks;
        self
    }
    #[inline(always)]
    pub fn with_inline_css_props(mut self, inline_css_props: NodeDataInlineCssPropertyVec) -> Self {
        self.inline_css_props = inline_css_props;
        self
    }

    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = NodeData::div();
        mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn copy_special(&self) -> Self {
        Self {
            node_type: self.node_type.into_library_owned_nodetype(),
            dataset: match &self.dataset {
                OptionRefAny::None => OptionRefAny::None,
                OptionRefAny::Some(s) => OptionRefAny::Some(s.clone()),
            },
            ids_and_classes: self.ids_and_classes.clone(), /* do not clone the IDs and classes if
                                                            * they are &'static */
            inline_css_props: self.inline_css_props.clone(), /* do not clone the inline CSS props
                                                              * if they are &'static */
            callbacks: self.callbacks.clone(),
            tab_index: self.tab_index,
            extra: self.extra.clone(),
        }
    }

    pub fn is_focusable(&self) -> bool {
        // TODO: do some better analysis of next / first / item
        self.get_tab_index().is_some()
            || self
                .get_callbacks()
                .iter()
                .any(|cb| cb.event.is_focus_callback())
    }

    pub fn get_iframe_node(&mut self) -> Option<&mut IFrameNode> {
        match &mut self.node_type {
            NodeType::IFrame(i) => Some(i),
            _ => None,
        }
    }

    pub fn get_render_image_callback_node<'a>(
        &'a mut self,
    ) -> Option<(&'a mut CoreImageCallback, ImageRefHash)> {
        match &mut self.node_type {
            NodeType::Image(img) => {
                let hash = image_ref_get_hash(&img);
                img.get_image_callback_mut().map(|r| (r, hash))
            }
            _ => None,
        }
    }

    pub fn debug_print_start(
        &self,
        css_cache: &CssPropertyCache,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> String {
        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);
        let style = css_cache.get_computed_css_style_string(&self, node_id, node_state);
        format!(
            "<{} data-az-node-id=\"{}\" {} {style}>",
            html_type,
            node_id.index(),
            attributes_string,
            style = if style.trim().is_empty() {
                String::new()
            } else {
                format!("style=\"{style}\"")
            }
        )
    }

    pub fn debug_print_end(&self) -> String {
        let html_type = self.node_type.get_path();
        format!("</{}>", html_type)
    }
}

/// A unique, runtime-generated identifier for a single `Dom` instance.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct DomId {
    pub inner: usize,
}

impl fmt::Display for DomId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl DomId {
    pub const ROOT_ID: DomId = DomId { inner: 0 };
}

impl Default for DomId {
    fn default() -> DomId {
        DomId::ROOT_ID
    }
}

impl_option!(
    DomId,
    OptionDomId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// A UUID for a DOM node within a `LayoutWindow`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DomNodeId {
    /// The ID of the `Dom` this node belongs to.
    pub dom: DomId,
    /// The hierarchical ID of the node within its `Dom`.
    pub node: NodeHierarchyItemId,
}

impl_option!(
    DomNodeId,
    OptionDomNodeId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl DomNodeId {
    pub const ROOT: DomNodeId = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::NONE,
    };
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read
/// anything back from it. It's designed for ease of construction.
#[repr(C)]
#[derive(PartialEq, Clone, Eq, Hash, PartialOrd, Ord)]
pub struct Dom {
    /// The data for the root node of this DOM (or sub-DOM).
    pub root: NodeData,
    /// The children of this DOM node.
    pub children: DomVec,
    // Tracks the number of sub-children of the current children, so that
    // the `Dom` can be converted into a `CompactDom`.
    pub estimated_total_children: usize,
}

impl_option!(
    Dom,
    OptionDom,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(Dom, DomVec, DomVecDestructor);
impl_vec_clone!(Dom, DomVec, DomVecDestructor);
impl_vec_mut!(Dom, DomVec);
impl_vec_debug!(Dom, DomVec);
impl_vec_partialord!(Dom, DomVec);
impl_vec_ord!(Dom, DomVec);
impl_vec_partialeq!(Dom, DomVec);
impl_vec_eq!(Dom, DomVec);
impl_vec_hash!(Dom, DomVec);

impl Dom {
    // ----- DOM CONSTRUCTORS

    /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
    /// doesn't allocate, it only allocates once you add at least one child node.
    #[inline(always)]
    pub fn new(node_type: NodeType) -> Self {
        Self {
            root: NodeData::new(node_type),
            children: Vec::new().into(),
            estimated_total_children: 0,
        }
    }
    #[inline(always)]
    pub fn from_data(node_data: NodeData) -> Self {
        Self {
            root: node_data,
            children: Vec::new().into(),
            estimated_total_children: 0,
        }
    }
    #[inline(always)]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }
    #[inline(always)]
    pub fn body() -> Self {
        Self::new(NodeType::Body)
    }
    #[inline(always)]
    pub fn br() -> Self {
        Self::new(NodeType::Br)
    }
    #[inline(always)]
    pub fn text<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Text(value.into()))
    }
    #[inline(always)]
    pub fn image(image: ImageRef) -> Self {
        Self::new(NodeType::Image(image))
    }
    #[inline(always)]
    pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self {
        Self::new(NodeType::IFrame(IFrameNode {
            callback: IFrameCallback { cb: callback },
            data,
        }))
    }

    /// Parse XML/XHTML string into a DOM
    ///
    /// This is a simple wrapper that parses XML and converts it to a DOM.
    /// For now, it just creates a text node with the content since full XML parsing
    /// requires the xml feature and more complex parsing logic.
    #[cfg(feature = "xml")]
    pub fn from_xml<S: AsRef<str>>(xml_str: S) -> Self {
        // TODO: Implement full XML parsing
        // For now, just create a text node showing that XML was loaded
        Self::text(format!(
            "XML content loaded ({} bytes)",
            xml_str.as_ref().len()
        ))
    }

    /// Parse XML/XHTML string into a DOM (fallback without xml feature)
    #[cfg(not(feature = "xml"))]
    pub fn from_xml<S: AsRef<str>>(xml_str: S) -> Self {
        Self::text(format!(
            "XML parsing requires 'xml' feature ({} bytes)",
            xml_str.as_ref().len()
        ))
    }

    // Swaps `self` with a default DOM, necessary for builder methods
    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self {
            root: NodeData::div(),
            children: DomVec::from_const_slice(&[]),
            estimated_total_children: 0,
        };
        mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn add_child(&mut self, child: Dom) {
        let mut v: DomVec = Vec::new().into();
        mem::swap(&mut v, &mut self.children);
        let mut v = v.into_library_owned_vec();
        v.push(child);
        self.children = v.into();
        self.estimated_total_children += 1;
    }

    #[inline(always)]
    pub fn set_children(&mut self, children: DomVec) {
        let children_estimated = children
            .iter()
            .map(|s| s.estimated_total_children + 1)
            .sum();
        self.children = children;
        self.estimated_total_children = children_estimated;
    }

    pub fn copy_except_for_root(&mut self) -> Self {
        Self {
            root: self.root.copy_special(),
            children: self.children.clone(),
            estimated_total_children: self.estimated_total_children,
        }
    }
    pub fn node_count(&self) -> usize {
        self.estimated_total_children + 1
    }

    pub fn style(&mut self, css: azul_css::parser2::CssApiWrapper) -> StyledDom {
        StyledDom::new(self, css)
    }
    #[inline(always)]
    pub fn with_children(mut self, children: DomVec) -> Self {
        self.children = children;
        self
    }
    #[inline(always)]
    pub fn with_child(&mut self, child: Self) -> Self {
        let mut dom = self.swap_with_default();
        dom.add_child(child);
        dom
    }
    #[inline(always)]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.root.set_tab_index(tab_index);
        self
    }
    #[inline(always)]
    pub fn with_dataset(mut self, data: OptionRefAny) -> Self {
        self.root.dataset = data;
        self
    }
    #[inline(always)]
    pub fn with_ids_and_classes(mut self, ids_and_classes: IdOrClassVec) -> Self {
        self.root.ids_and_classes = ids_and_classes;
        self
    }
    #[inline(always)]
    pub fn with_callbacks(mut self, callbacks: CoreCallbackDataVec) -> Self {
        self.root.callbacks = callbacks;
        self
    }
    #[inline(always)]
    pub fn with_inline_css_props(mut self, inline_css_props: NodeDataInlineCssPropertyVec) -> Self {
        self.root.inline_css_props = inline_css_props;
        self
    }

    pub fn set_inline_style(&mut self, style: &str) {
        self.root.set_inline_css_props(
            self.root
                .get_inline_css_props()
                .with_append(NodeDataInlineCssPropertyVec::parse_normal(style)),
        )
    }

    pub fn with_inline_style(mut self, style: &str) -> Self {
        self.set_inline_style(style);
        self
    }

    #[inline]
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.root.set_context_menu(context_menu);
        self
    }

    pub fn fixup_children_estimated(&mut self) -> usize {
        if self.children.is_empty() {
            self.estimated_total_children = 0;
        } else {
            self.estimated_total_children = self
                .children
                .iter_mut()
                .map(|s| s.fixup_children_estimated() + 1)
                .sum();
        }
        return self.estimated_total_children;
    }
}

impl core::iter::FromIterator<Dom> for Dom {
    fn from_iter<I: IntoIterator<Item = Dom>>(iter: I) -> Self {
        let mut estimated_total_children = 0;
        let children = iter
            .into_iter()
            .map(|c| {
                estimated_total_children += c.estimated_total_children + 1;
                c
            })
            .collect::<Vec<Dom>>();

        Dom {
            root: NodeData::div(),
            children: children.into(),
            estimated_total_children,
        }
    }
}

impl fmt::Debug for Dom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn print_dom(d: &Dom, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "Dom {{\r\n")?;
            write!(f, "\troot: {:#?}\r\n", d.root)?;
            write!(
                f,
                "\testimated_total_children: {:#?}\r\n",
                d.estimated_total_children
            )?;
            write!(f, "\tchildren: [\r\n")?;
            for c in d.children.iter() {
                print_dom(c, f)?;
            }
            write!(f, "\t]\r\n")?;
            write!(f, "}}\r\n")?;
            Ok(())
        }

        print_dom(self, f)
    }
}
