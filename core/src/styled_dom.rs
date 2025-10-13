use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
};

use azul_css::{
    css::{Css, CssPath},
    parser2::CssApiWrapper,
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, ColumnCountValue, ColumnFillValue, ColumnRuleColorValue,
            ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue, ColumnWidthValue,
            ContentValue, CounterIncrementValue, CounterResetValue, CssProperty, CssPropertyType,
            FlowFromValue, FlowIntoValue, LayoutAlignContentValue, LayoutAlignItemsValue,
            LayoutAlignSelfValue, LayoutBorderBottomWidthValue, LayoutBorderLeftWidthValue,
            LayoutBorderRightWidthValue, LayoutBorderTopWidthValue, LayoutBottomValue,
            LayoutBoxSizingValue, LayoutClearValue, LayoutColumnGapValue, LayoutDisplayValue,
            LayoutFlexBasisValue, LayoutFlexDirectionValue, LayoutFlexGrowValue,
            LayoutFlexShrinkValue, LayoutFlexWrapValue, LayoutFloatValue, LayoutGapValue,
            LayoutGridAutoColumnsValue, LayoutGridAutoFlowValue, LayoutGridAutoRowsValue,
            LayoutGridColumnValue, LayoutGridRowValue, LayoutGridTemplateColumnsValue,
            LayoutGridTemplateRowsValue, LayoutHeightValue, LayoutJustifyContentValue,
            LayoutJustifyItemsValue, LayoutJustifySelfValue, LayoutLeftValue,
            LayoutMarginBottomValue, LayoutMarginLeftValue, LayoutMarginRightValue,
            LayoutMarginTopValue, LayoutMaxHeightValue, LayoutMaxWidthValue, LayoutMinHeightValue,
            LayoutMinWidthValue, LayoutOverflowValue, LayoutPaddingBottomValue,
            LayoutPaddingLeftValue, LayoutPaddingRightValue, LayoutPaddingTopValue,
            LayoutPositionValue, LayoutRightValue, LayoutRowGapValue, LayoutScrollbarWidthValue,
            LayoutTextJustifyValue, LayoutTopValue, LayoutWidthValue, LayoutWritingModeValue,
            LayoutZIndexValue, OrphansValue, PageBreakValue, ScrollbarStyleValue,
            SelectionBackgroundColorValue, SelectionColorValue, ShapeImageThresholdValue,
            ShapeMarginValue, ShapeOutsideValue, StringSetValue, StyleBackfaceVisibilityValue,
            StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
            StyleBackgroundRepeatVecValue, StyleBackgroundSizeVecValue,
            StyleBorderBottomColorValue, StyleBorderBottomLeftRadiusValue,
            StyleBorderBottomRightRadiusValue, StyleBorderBottomStyleValue,
            StyleBorderLeftColorValue, StyleBorderLeftStyleValue, StyleBorderRightColorValue,
            StyleBorderRightStyleValue, StyleBorderTopColorValue, StyleBorderTopLeftRadiusValue,
            StyleBorderTopRightRadiusValue, StyleBorderTopStyleValue, StyleBoxShadowValue,
            StyleCursorValue, StyleDirectionValue, StyleFilterVecValue, StyleFontFamilyVecValue,
            StyleFontSizeValue, StyleFontValue, StyleHyphensValue, StyleLetterSpacingValue,
            StyleLineHeightValue, StyleMixBlendModeValue, StyleOpacityValue,
            StylePerspectiveOriginValue, StyleScrollbarColorValue, StyleTabWidthValue,
            StyleTextAlignValue, StyleTextColorValue, StyleTransformOriginValue,
            StyleTransformVecValue, StyleVisibilityValue, StyleWhiteSpaceValue,
            StyleWordSpacingValue, WidowsValue,
        },
        style::StyleTextColor,
    },
    AzString,
};

use crate::{
    resources::{Au, ImageCache, ImageRef, ImmediateFontId, RendererResources},
    callbacks::{CallbackInfo, RefAny, Update},
    dom::{
        CompactDom, Dom, NodeData, NodeDataInlineCssProperty, NodeDataVec, OptionTabIndex,
        TabIndex, TagId,
    },
    id_tree::{Node, NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    style::{
        construct_html_cascade_tree, matches_html_element, rule_ends_with, CascadeInfo,
        CascadeInfoVec,
    },
    window::Menu,
    FastBTreeSet, FastHashMap,
};

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct ChangedCssProperty {
    pub previous_state: StyledNodeState,
    pub previous_prop: CssProperty,
    pub current_state: StyledNodeState,
    pub current_prop: CssProperty,
}

impl_vec!(
    ChangedCssProperty,
    ChangedCssPropertyVec,
    ChangedCssPropertyVecDestructor
);
impl_vec_debug!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_partialord!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_clone!(
    ChangedCssProperty,
    ChangedCssPropertyVec,
    ChangedCssPropertyVecDestructor
);
impl_vec_partialeq!(ChangedCssProperty, ChangedCssPropertyVec);

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub enum CssPropertySource {
    Css(CssPath),
    Inline,
}

/// NOTE: multiple states can be active at
///
/// TODO: use bitflags here!
#[repr(C)]
#[derive(Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct StyledNodeState {
    pub normal: bool,
    pub hover: bool,
    pub active: bool,
    pub focused: bool,
}

impl core::fmt::Debug for StyledNodeState {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let mut v = Vec::new();
        if self.normal {
            v.push("normal");
        }
        if self.hover {
            v.push("hover");
        }
        if self.active {
            v.push("active");
        }
        if self.focused {
            v.push("focused");
        }
        write!(f, "{:?}", v)
    }
}

impl Default for StyledNodeState {
    fn default() -> StyledNodeState {
        Self::new()
    }
}

impl StyledNodeState {
    pub const fn new() -> Self {
        StyledNodeState {
            normal: true,
            hover: false,
            active: false,
            focused: false,
        }
    }
}

/// A styled Dom node
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct StyledNode {
    /// Current state of this styled node (used later for caching the style / layout)
    pub state: StyledNodeState,
    /// Optional tag ID
    ///
    /// NOTE: The tag ID has to be adjusted after the layout is done (due to scroll tags)
    pub tag_id: OptionTagId,
}

impl_vec!(StyledNode, StyledNodeVec, StyledNodeVecDestructor);
impl_vec_mut!(StyledNode, StyledNodeVec);
impl_vec_debug!(StyledNode, StyledNodeVec);
impl_vec_partialord!(StyledNode, StyledNodeVec);
impl_vec_clone!(StyledNode, StyledNodeVec, StyledNodeVecDestructor);
impl_vec_partialeq!(StyledNode, StyledNodeVec);

impl StyledNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, StyledNode> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, StyledNode> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Clone)]
pub struct CssPropertyCachePtr {
    pub ptr: Box<CssPropertyCache>,
    pub run_destructor: bool,
}

impl CssPropertyCachePtr {
    pub fn new(cache: CssPropertyCache) -> Self {
        Self {
            ptr: Box::new(cache),
            run_destructor: true,
        }
    }
    fn downcast_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        &mut *self.ptr
    }
}

impl Drop for CssPropertyCachePtr {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[test]
fn test_it() {
    let s = "
        html, body, p {
            margin: 0;
            padding: 0;
        }
        #div1 {
            border: solid black;
            height: 2in;
            position: absolute;
            top: 1in;
            width: 3in;
        }
        div div {
            background: blue;
            height: 1in;
            position: fixed;
            width: 1in;
        }
    ";

    let css = azul_css::parser2::new_from_str(s);
    let dom = Dom::body()
        .with_children(
            vec![Dom::div()
                .with_ids_and_classes(
                    vec![crate::dom::IdOrClass::Id("div1".to_string().into())].into(),
                )
                .with_children(vec![Dom::div()].into())]
            .into(),
        )
        .style(CssApiWrapper { css: css.0 });
    println!(
        "styled dom: {:#?}",
        dom.get_html_string("", "", false)
            .lines()
            .collect::<Vec<_>>()
    );
}

// NOTE: To avoid large memory allocations, this is a "cache" that stores all the CSS properties
// found in the DOM. This cache exists on a per-DOM basis, so it scales independent of how many
// nodes are in the DOM.
//
// If each node would carry its own CSS properties, that would unnecessarily consume memory
// because most nodes use the default properties or override only one or two properties.
//
// The cache can compute the property of any node at any given time, given the current node
// state (hover, active, focused, normal). This way we don't have to duplicate the CSS properties
// onto every single node and exchange them when the style changes. Two caches can be appended
// to each other by simply merging their NodeIds.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CssPropertyCache {
    // number of nodes in the current DOM
    pub node_count: usize,

    // properties that were overridden in callbacks (not specific to any node state)
    pub user_overridden_properties: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,

    // non-default CSS properties that were cascaded from the parent
    pub cascaded_normal_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub cascaded_hover_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub cascaded_active_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub cascaded_focus_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,

    // non-default CSS properties that were set via a CSS file
    pub css_normal_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub css_hover_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub css_active_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub css_focus_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
}

impl CssPropertyCache {
    /// Restyles the CSS property cache with a new CSS file
    #[must_use]
    pub fn restyle(
        &mut self,
        css: &mut Css,
        node_data: &NodeDataContainerRef<NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
        html_tree: &NodeDataContainerRef<CascadeInfo>,
    ) -> Vec<TagIdToNodeIdMapping> {
        use azul_css::{
            css::{CssDeclaration, CssPathPseudoSelector::*},
            props::layout::LayoutDisplay,
        };

        let css_is_empty = css.is_empty();

        if !css_is_empty {
            css.sort_by_specificity();

            macro_rules! filter_rules {($expected_pseudo_selector:expr, $node_id:expr) => {{
                css
                .rules() // can not be parallelized due to specificity order matching
                .filter(|rule_block| rule_ends_with(&rule_block.path, $expected_pseudo_selector))
                .filter(|rule_block| matches_html_element(
                    &rule_block.path,
                    $node_id,
                    &node_hierarchy.as_container(),
                    &node_data,
                    &html_tree,
                    $expected_pseudo_selector
                ))
                // rule matched, now copy all the styles of this rule
                .flat_map(|matched_rule| {
                    matched_rule.declarations
                    .iter()
                    .filter_map(move |declaration| {
                        match declaration {
                            CssDeclaration::Static(s) => Some(s),
                            CssDeclaration::Dynamic(_d) => None, // TODO: No variable support yet!
                        }
                    })
                })
                .map(|prop| prop.clone())
                .collect::<Vec<CssProperty>>()
            }};}

            // NOTE: This is wrong, but fast
            //
            // Get all nodes that end with `:hover`, `:focus` or `:active`
            // and copy the respective styles to the `hover_css_constraints`, etc. respectively
            //
            // NOTE: This won't work correctly for paths with `.blah:hover > #thing`
            // but that can be fixed later

            // go through each HTML node (in parallel) and see which CSS rules match
            let css_normal_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(None, node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_hover_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Hover), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_active_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Active), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_focus_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Focus), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            self.css_normal_props = css_normal_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();

            self.css_hover_props = css_hover_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();

            self.css_active_props = css_active_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();

            self.css_focus_props = css_focus_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();
        }

        // Inheritance: Inherit all values of the parent to the children, but
        // only if the property is inheritable and isn't yet set
        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes.iter() {
            let parent_id = match node_id.into_crate_internal() {
                Some(s) => s,
                None => continue,
            };

            // Inherit CSS properties from map A -> map B
            // map B will be populated with all inherited CSS properties
            macro_rules! inherit_props {
                ($from_inherit_map:expr, $to_inherit_map:expr) => {
                    let parent_inheritable_css_props =
                        $from_inherit_map.get(&parent_id).and_then(|map| {
                            let parent_inherit_props = map
                                .iter()
                                .filter(|(css_prop_type, _)| css_prop_type.is_inheritable())
                                .map(|(css_prop_type, css_prop)| (*css_prop_type, css_prop.clone()))
                                .collect::<Vec<(CssPropertyType, CssProperty)>>();
                            if parent_inherit_props.is_empty() {
                                None
                            } else {
                                Some(parent_inherit_props)
                            }
                        });

                    match parent_inheritable_css_props {
                        Some(pi) => {
                            // only override the rule if the child does not already have an
                            // inherited rule
                            for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                                let child_map = $to_inherit_map
                                    .entry(child_id)
                                    .or_insert_with(|| BTreeMap::new());

                                for (inherited_rule_type, inherited_rule_value) in pi.iter() {
                                    let _ = child_map
                                        .entry(*inherited_rule_type)
                                        .or_insert_with(|| inherited_rule_value.clone());
                                }
                            }
                        }
                        None => {}
                    }
                };
            }

            // Same as inherit_props, but filters along the inline node data instead
            macro_rules! inherit_inline_css_props {($filter_type:ident, $to_inherit_map:expr) => {
                let parent_inheritable_css_props = &node_data[parent_id]
                .inline_css_props
                .iter()
                 // test whether the property is a [normal, hover, focus, active] property
                .filter_map(|css_prop| if let NodeDataInlineCssProperty::$filter_type(p) = css_prop { Some(p) } else { None })
                // test whether the property is inheritable
                .filter(|css_prop| css_prop.get_type().is_inheritable())
                .cloned()
                .collect::<Vec<CssProperty>>();

                if !parent_inheritable_css_props.is_empty() {
                    // only override the rule if the child does not already have an inherited rule
                    for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                        let child_map = $to_inherit_map.entry(child_id).or_insert_with(|| BTreeMap::new());
                        for inherited_rule in parent_inheritable_css_props.iter() {
                            let _ = child_map
                            .entry(inherited_rule.get_type())
                            .or_insert_with(|| inherited_rule.clone());
                        }
                    }
                }

            };}

            // strongest inheritance first

            // Inherit inline CSS properties
            inherit_inline_css_props!(Normal, self.cascaded_normal_props);
            inherit_inline_css_props!(Hover, self.cascaded_hover_props);
            inherit_inline_css_props!(Active, self.cascaded_active_props);
            inherit_inline_css_props!(Focus, self.cascaded_focus_props);

            // Inherit the CSS properties from the CSS file
            if !css_is_empty {
                inherit_props!(self.css_normal_props, self.cascaded_normal_props);
                inherit_props!(self.css_hover_props, self.cascaded_hover_props);
                inherit_props!(self.css_active_props, self.cascaded_active_props);
                inherit_props!(self.css_focus_props, self.cascaded_focus_props);
            }

            // Inherit properties that were inherited in a previous iteration of the loop
            inherit_props!(self.cascaded_normal_props, self.cascaded_normal_props);
            inherit_props!(self.cascaded_hover_props, self.cascaded_hover_props);
            inherit_props!(self.cascaded_active_props, self.cascaded_active_props);
            inherit_props!(self.cascaded_focus_props, self.cascaded_focus_props);
        }

        // When restyling, the tag / node ID mappings may change, regenerate them
        // See if the node should have a hit-testing tag ID
        let default_node_state = StyledNodeState::default();

        // In order to hit-test `:hover` and `:active` selectors,
        // we need to insert "tag IDs" for all rectangles
        // that have a non-normal path ending, for example if we have
        // `#thing:hover`, then all nodes selected by `#thing`
        // need to get a TagId, otherwise, they can't be hit-tested.

        // NOTE: restyling a DOM may change the :hover nodes, which is
        // why the tag IDs have to be re-generated on every .restyle() call!
        node_data
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);

                let should_auto_insert_tabindex = node_data
                    .get_callbacks()
                    .iter()
                    .any(|cb| cb.event.is_focus_callback());

                let tab_index = match node_data.get_tab_index() {
                    Some(s) => Some(*s),
                    None => {
                        if should_auto_insert_tabindex {
                            Some(TabIndex::Auto)
                        } else {
                            None
                        }
                    }
                };

                let mut node_should_have_tag = false;

                // workaround for "goto end" - early break if
                // one of the conditions is true
                loop {
                    // check for display: none
                    let display = self
                        .get_display(&node_data, &node_id, &default_node_state)
                        .and_then(|p| p.get_property_or_default())
                        .unwrap_or_default();

                    if display == LayoutDisplay::None {
                        node_should_have_tag = false;
                        break;
                    }

                    if node_data.has_context_menu() {
                        node_should_have_tag = true;
                        break;
                    }

                    if tab_index.is_some() {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for context menu
                    if node_data.get_context_menu().is_some() {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :hover
                    let node_has_hover_props =
                        node_data.inline_css_props.as_ref().iter().any(|p| match p {
                            NodeDataInlineCssProperty::Hover(_) => true,
                            _ => false,
                        }) || self.css_hover_props.get(&node_id).is_some()
                            || self.cascaded_hover_props.get(&node_id).is_some();

                    if node_has_hover_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :active
                    let node_has_active_props =
                        node_data.inline_css_props.as_ref().iter().any(|p| match p {
                            NodeDataInlineCssProperty::Active(_) => true,
                            _ => false,
                        }) || self.css_active_props.get(&node_id).is_some()
                            || self.cascaded_active_props.get(&node_id).is_some();

                    if node_has_active_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :focus
                    let node_has_focus_props =
                        node_data.inline_css_props.as_ref().iter().any(|p| match p {
                            NodeDataInlineCssProperty::Focus(_) => true,
                            _ => false,
                        }) || self.css_focus_props.get(&node_id).is_some()
                            || self.cascaded_focus_props.get(&node_id).is_some();

                    if node_has_focus_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check whether any Hover(), Active() or Focus() callbacks are present
                    let node_only_window_callbacks = node_data.get_callbacks().is_empty()
                        || node_data
                            .get_callbacks()
                            .iter()
                            .all(|cb| cb.event.is_window_callback());

                    if !node_only_window_callbacks {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for non-default cursor: property - needed for hit-testing cursor
                    let node_has_non_default_cursor = self
                        .get_cursor(&node_data, &node_id, &default_node_state)
                        .is_some();

                    if node_has_non_default_cursor {
                        node_should_have_tag = true;
                        break;
                    }

                    break;
                }

                if !node_should_have_tag {
                    None
                } else {
                    Some(TagIdToNodeIdMapping {
                        tag_id: AzTagId::from_crate_internal(TagId::unique()),
                        node_id: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                        tab_index: tab_index.into(),
                        parent_node_ids: {
                            let mut parents = Vec::new();
                            let mut cur_parent = node_hierarchy.as_container()[node_id].parent_id();
                            while let Some(c) = cur_parent.clone() {
                                parents.push(NodeHierarchyItemId::from_crate_internal(Some(c)));
                                cur_parent = node_hierarchy.as_container()[c].parent_id();
                            }
                            parents.reverse(); // parents sorted in depth-increasing order
                            parents.into()
                        },
                    })
                }
            })
            .collect()
    }

    pub fn get_computed_css_style_string(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> String {
        let mut s = String::new();
        if let Some(p) = self.get_background_content(&node_data, node_id, node_state) {
            s.push_str(&format!("background: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_position(&node_data, node_id, node_state) {
            s.push_str(&format!("background-position: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_size(&node_data, node_id, node_state) {
            s.push_str(&format!("background-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_repeat(&node_data, node_id, node_state) {
            s.push_str(&format!("background-repeat: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_font_size(&node_data, node_id, node_state) {
            s.push_str(&format!("font-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_font_family(&node_data, node_id, node_state) {
            s.push_str(&format!("font-family: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_color(&node_data, node_id, node_state) {
            s.push_str(&format!("color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_align(&node_data, node_id, node_state) {
            s.push_str(&format!("text-align: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_line_height(&node_data, node_id, node_state) {
            s.push_str(&format!("line-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_letter_spacing(&node_data, node_id, node_state) {
            s.push_str(&format!("letter-spacing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_word_spacing(&node_data, node_id, node_state) {
            s.push_str(&format!("word-spacing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_tab_width(&node_data, node_id, node_state) {
            s.push_str(&format!("tab-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_cursor(&node_data, node_id, node_state) {
            s.push_str(&format!("cursor: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_shadow_left(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-left: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_box_shadow_right(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-right: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_box_shadow_top(&node_data, node_id, node_state) {
            s.push_str(&format!("-azul-box-shadow-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_shadow_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-bottom: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_top_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_left_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-top-left-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_top_right_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-top-right-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_bottom_left_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-bottom-left-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_bottom_right_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-bottom-right-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_opacity(&node_data, node_id, node_state) {
            s.push_str(&format!("opacity: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_transform(&node_data, node_id, node_state) {
            s.push_str(&format!("transform: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_transform_origin(&node_data, node_id, node_state) {
            s.push_str(&format!("transform-origin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_perspective_origin(&node_data, node_id, node_state) {
            s.push_str(&format!("perspective-origin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_backface_visibility(&node_data, node_id, node_state) {
            s.push_str(&format!("backface-visibility: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_hyphens(&node_data, node_id, node_state) {
            s.push_str(&format!("hyphens: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_direction(&node_data, node_id, node_state) {
            s.push_str(&format!("direction: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_white_space(&node_data, node_id, node_state) {
            s.push_str(&format!("white-space: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_display(&node_data, node_id, node_state) {
            s.push_str(&format!("display: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_float(&node_data, node_id, node_state) {
            s.push_str(&format!("float: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_sizing(&node_data, node_id, node_state) {
            s.push_str(&format!("box-sizing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_width(&node_data, node_id, node_state) {
            s.push_str(&format!("width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_height(&node_data, node_id, node_state) {
            s.push_str(&format!("height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_min_width(&node_data, node_id, node_state) {
            s.push_str(&format!("min-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_min_height(&node_data, node_id, node_state) {
            s.push_str(&format!("min-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_max_width(&node_data, node_id, node_state) {
            s.push_str(&format!("max-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_max_height(&node_data, node_id, node_state) {
            s.push_str(&format!("max-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_position(&node_data, node_id, node_state) {
            s.push_str(&format!("position: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_top(&node_data, node_id, node_state) {
            s.push_str(&format!("top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_right(&node_data, node_id, node_state) {
            s.push_str(&format!("right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_left(&node_data, node_id, node_state) {
            s.push_str(&format!("left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_top(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_left(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_right(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_top(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_left(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_right(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_x(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-x: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_y(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-y: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_direction(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-direction: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_wrap(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-wrap: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_grow(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-grow: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_shrink(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-shrink: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_justify_content(&node_data, node_id, node_state) {
            s.push_str(&format!("justify-content: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_align_items(&node_data, node_id, node_state) {
            s.push_str(&format!("align-items: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_align_content(&node_data, node_id, node_state) {
            s.push_str(&format!("align-content: {};", p.get_css_value_fmt()));
        }
        s
    }
}

/// Calculated hash of a font-family
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct StyleFontFamilyHash(pub u64);

impl ::core::fmt::Debug for StyleFontFamilyHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StyleFontFamilyHash({})", self.0)
    }
}

impl StyleFontFamilyHash {
    pub fn new(family: &StyleFontFamily) -> Self {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        family.hash(&mut hasher);
        Self(hasher.finalize64())
    }
}

/// Calculated hash of a font-family
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct StyleFontFamiliesHash(pub u64);

impl ::core::fmt::Debug for StyleFontFamiliesHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StyleFontFamiliesHash({})", self.0)
    }
}

impl StyleFontFamiliesHash {
    pub fn new(families: &[StyleFontFamily]) -> Self {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        for f in families.iter() {
            f.hash(&mut hasher);
        }
        Self(hasher.finalize64())
    }
}

impl CssPropertyCache {
    pub fn empty(node_count: usize) -> Self {
        Self {
            node_count,
            user_overridden_properties: BTreeMap::new(),

            cascaded_normal_props: BTreeMap::new(),
            cascaded_hover_props: BTreeMap::new(),
            cascaded_active_props: BTreeMap::new(),
            cascaded_focus_props: BTreeMap::new(),

            css_normal_props: BTreeMap::new(),
            css_hover_props: BTreeMap::new(),
            css_active_props: BTreeMap::new(),
            css_focus_props: BTreeMap::new(),
        }
    }

    pub fn append(&mut self, other: &mut Self) {
        macro_rules! append_css_property_vec {
            ($field_name:ident) => {{
                let mut s = BTreeMap::new();
                core::mem::swap(&mut s, &mut other.$field_name);
                for (node_id, property_map) in s.into_iter() {
                    self.$field_name
                        .insert(node_id + self.node_count, property_map);
                }
            }};
        }

        append_css_property_vec!(user_overridden_properties);
        append_css_property_vec!(cascaded_normal_props);
        append_css_property_vec!(cascaded_hover_props);
        append_css_property_vec!(cascaded_active_props);
        append_css_property_vec!(cascaded_focus_props);
        append_css_property_vec!(css_normal_props);
        append_css_property_vec!(css_hover_props);
        append_css_property_vec!(css_active_props);
        append_css_property_vec!(css_focus_props);

        self.node_count += other.node_count;
    }

    pub fn is_horizontal_overflow_visible(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_x(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_visible()
    }

    pub fn is_vertical_overflow_visible(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_y(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_visible()
    }

    pub fn is_horizontal_overflow_hidden(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_x(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_hidden()
    }

    pub fn is_vertical_overflow_hidden(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_y(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_hidden()
    }

    pub fn get_text_color_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleTextColor {
        use crate::ui_solver::DEFAULT_TEXT_COLOR;
        self.get_text_color(node_data, node_id, node_state)
            .and_then(|fs| fs.get_property().cloned())
            .unwrap_or(DEFAULT_TEXT_COLOR)
    }

    /// Returns the font ID of the
    pub fn get_font_id_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleFontFamilyVec {
        use crate::ui_solver::DEFAULT_FONT_ID;
        let default_font_id = vec![StyleFontFamily::System(AzString::from_const_str(
            DEFAULT_FONT_ID,
        ))]
        .into();
        let font_family_opt = self.get_font_family(node_data, node_id, node_state);

        font_family_opt
            .as_ref()
            .and_then(|family| Some(family.get_property()?.clone()))
            .unwrap_or(default_font_id)
    }

    pub fn get_font_size_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleFontSize {
        use crate::ui_solver::DEFAULT_FONT_SIZE;
        self.get_font_size(node_data, node_id, node_state)
            .and_then(|fs| fs.get_property().cloned())
            .unwrap_or(DEFAULT_FONT_SIZE)
    }

    pub fn has_border(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_border_left_width(node_data, node_id, node_state)
            .is_some()
            || self
                .get_border_right_width(node_data, node_id, node_state)
                .is_some()
            || self
                .get_border_top_width(node_data, node_id, node_state)
                .is_some()
            || self
                .get_border_bottom_width(node_data, node_id, node_state)
                .is_some()
    }

    pub fn has_box_shadow(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_box_shadow_left(node_data, node_id, node_state)
            .is_some()
            || self
                .get_box_shadow_right(node_data, node_id, node_state)
                .is_some()
            || self
                .get_box_shadow_top(node_data, node_id, node_state)
                .is_some()
            || self
                .get_box_shadow_bottom(node_data, node_id, node_state)
                .is_some()
    }

    pub fn get_property<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {
        // NOTE: This function is slow, but it is going to be called on every
        // node in parallel, so it should be rather fast in the end

        // First test if there is some user-defined override for the property
        if let Some(p) = self
            .user_overridden_properties
            .get(node_id)
            .and_then(|n| n.get(css_property_type))
        {
            return Some(p);
        }

        if !(node_state.normal || node_state.active || node_state.hover || node_state.focused) {
            return None;
        }

        // If that fails, see if there is an inline CSS property that matches
        // :focus > :active > :hover > :normal
        if node_state.focused {
            if let Some(p) = self
                .css_focus_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Focus(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_focus_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        if node_state.active {
            if let Some(p) = self
                .css_active_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Active(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_active_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        if node_state.hover {
            if let Some(p) = self
                .css_hover_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Hover(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_hover_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        if node_state.normal {
            if let Some(p) = self
                .css_normal_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Normal(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_normal_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        // Nothing found, use the default
        None
    }

    pub fn get_background_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundContent,
        )
        .and_then(|p| p.as_background_content())
    }

    // Method for getting hyphens property
    pub fn get_hyphens<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHyphensValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Hyphens)
            .and_then(|p| p.as_hyphens())
    }

    // Method for getting direction property
    pub fn get_direction<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleDirectionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Direction)
            .and_then(|p| p.as_direction())
    }

    // Method for getting white-space property
    pub fn get_white_space<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWhiteSpaceValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::WhiteSpace)
            .and_then(|p| p.as_white_space())
    }
    pub fn get_background_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundPositionVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundPosition,
        )
        .and_then(|p| p.as_background_position())
    }
    pub fn get_background_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundSizeVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundSize,
        )
        .and_then(|p| p.as_background_size())
    }
    pub fn get_background_repeat<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundRepeatVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundRepeat,
        )
        .and_then(|p| p.as_background_repeat())
    }
    pub fn get_font_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontSizeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontSize)
            .and_then(|p| p.as_font_size())
    }
    pub fn get_font_family<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontFamilyVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontFamily)
            .and_then(|p| p.as_font_family())
    }
    pub fn get_text_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextColorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextColor)
            .and_then(|p| p.as_text_color())
    }
    // Method for getting caret-color property
    pub fn get_caret_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretColorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::CaretColor)
            .and_then(|p| p.as_caret_color())
    }

    // Method for getting caret-animation-duration property
    pub fn get_caret_animation_duration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretAnimationDurationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CaretAnimationDuration,
        )
        .and_then(|p| p.as_caret_animation_duration())
    }

    // Method for getting selection-background-color property
    pub fn get_selection_background_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionBackgroundColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionBackgroundColor,
        )
        .and_then(|p| p.as_selection_background_color())
    }

    // Method for getting selection-color property
    pub fn get_selection_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionColor,
        )
        .and_then(|p| p.as_selection_color())
    }

    // Method for getting text-justify property
    pub fn get_text_justify<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTextJustifyValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextJustify,
        )
        .and_then(|p| p.as_text_justify())
    }

    // Method for getting z-index property
    pub fn get_z_index<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutZIndexValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ZIndex)
            .and_then(|p| p.as_z_index())
    }

    // Method for getting flex-basis property
    pub fn get_flex_basis<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexBasisValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexBasis)
            .and_then(|p| p.as_flex_basis())
    }

    // Method for getting column-gap property
    pub fn get_column_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutColumnGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnGap)
            .and_then(|p| p.as_column_gap())
    }

    // Method for getting row-gap property
    pub fn get_row_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutRowGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::RowGap)
            .and_then(|p| p.as_row_gap())
    }

    // Method for getting grid-template-columns property
    pub fn get_grid_template_columns<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridTemplateColumnsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridTemplateColumns,
        )
        .and_then(|p| p.as_grid_template_columns())
    }

    // Method for getting grid-template-rows property
    pub fn get_grid_template_rows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridTemplateRowsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridTemplateRows,
        )
        .and_then(|p| p.as_grid_template_rows())
    }

    // Method for getting grid-auto-columns property
    pub fn get_grid_auto_columns<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoColumnsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoColumns,
        )
        .and_then(|p| p.as_grid_auto_columns())
    }

    // Method for getting grid-auto-rows property
    pub fn get_grid_auto_rows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoRowsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoRows,
        )
        .and_then(|p| p.as_grid_auto_rows())
    }

    // Method for getting grid-column property
    pub fn get_grid_column<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridColumnValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridColumn)
            .and_then(|p| p.as_grid_column())
    }

    // Method for getting grid-row property
    pub fn get_grid_row<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridRowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridRow)
            .and_then(|p| p.as_grid_row())
    }

    // Method for getting grid-auto-flow property
    pub fn get_grid_auto_flow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoFlowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoFlow,
        )
        .and_then(|p| p.as_grid_auto_flow())
    }

    // Method for getting justify-self property
    pub fn get_justify_self<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifySelfValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifySelf,
        )
        .and_then(|p| p.as_justify_self())
    }

    // Method for getting justify-items property
    pub fn get_justify_items<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifyItemsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifyItems,
        )
        .and_then(|p| p.as_justify_items())
    }

    // Method for getting gap property
    pub fn get_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Gap)
            .and_then(|p| p.as_gap())
    }

    // Method for getting grid-gap property
    pub fn get_grid_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridGap)
            .and_then(|p| p.as_grid_gap())
    }

    // Method for getting align-self property
    pub fn get_align_self<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignSelfValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignSelf)
            .and_then(|p| p.as_align_self())
    }

    // Method for getting font property
    pub fn get_font<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Font)
            .and_then(|p| p.as_font())
    }

    // Method for getting writing-mode property
    pub fn get_writing_mode<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutWritingModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::WritingMode,
        )
        .and_then(|p| p.as_writing_mode())
    }

    // Method for getting clear property
    pub fn get_clear<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutClearValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Clear)
            .and_then(|p| p.as_clear())
    }

    // Method for getting scrollbar-style property
    pub fn get_scrollbar_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarStyle,
        )
        .and_then(|p| p.as_scrollbar_style())
    }

    // Method for getting scrollbar-width property
    pub fn get_scrollbar_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutScrollbarWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarWidth,
        )
        .and_then(|p| p.as_scrollbar_width())
    }

    // Method for getting scrollbar-color property
    pub fn get_scrollbar_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleScrollbarColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarColor,
        )
        .and_then(|p| p.as_scrollbar_color())
    }

    // Method for getting visibility property
    pub fn get_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleVisibilityValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Visibility)
            .and_then(|p| p.as_visibility())
    }

    // Method for getting break-before property
    pub fn get_break_before<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a PageBreakValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BreakBefore,
        )
        .and_then(|p| p.as_break_before())
    }

    // Method for getting break-after property
    pub fn get_break_after<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a PageBreakValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BreakAfter)
            .and_then(|p| p.as_break_after())
    }

    // Method for getting break-inside property
    pub fn get_break_inside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a BreakInsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BreakInside,
        )
        .and_then(|p| p.as_break_inside())
    }

    // Method for getting orphans property
    pub fn get_orphans<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a OrphansValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Orphans)
            .and_then(|p| p.as_orphans())
    }

    // Method for getting widows property
    pub fn get_widows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a WidowsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Widows)
            .and_then(|p| p.as_widows())
    }

    // Method for getting box-decoration-break property
    pub fn get_box_decoration_break<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a BoxDecorationBreakValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxDecorationBreak,
        )
        .and_then(|p| p.as_box_decoration_break())
    }

    // Method for getting column-count property
    pub fn get_column_count<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnCountValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnCount,
        )
        .and_then(|p| p.as_column_count())
    }

    // Method for getting column-width property
    pub fn get_column_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnWidth,
        )
        .and_then(|p| p.as_column_width())
    }

    // Method for getting column-span property
    pub fn get_column_span<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnSpanValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnSpan)
            .and_then(|p| p.as_column_span())
    }

    // Method for getting column-fill property
    pub fn get_column_fill<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnFillValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnFill)
            .and_then(|p| p.as_column_fill())
    }

    // Method for getting column-rule-width property
    pub fn get_column_rule_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleWidth,
        )
        .and_then(|p| p.as_column_rule_width())
    }

    // Method for getting column-rule-style property
    pub fn get_column_rule_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleStyle,
        )
        .and_then(|p| p.as_column_rule_style())
    }

    // Method for getting column-rule-color property
    pub fn get_column_rule_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleColor,
        )
        .and_then(|p| p.as_column_rule_color())
    }

    // Method for getting flow-into property
    pub fn get_flow_into<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a FlowIntoValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlowInto)
            .and_then(|p| p.as_flow_into())
    }

    // Method for getting flow-from property
    pub fn get_flow_from<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a FlowFromValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlowFrom)
            .and_then(|p| p.as_flow_from())
    }

    // Method for getting shape-outside property
    pub fn get_shape_outside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeOutsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeOutside,
        )
        .and_then(|p| p.as_shape_outside())
    }

    // Method for getting shape-margin property
    pub fn get_shape_margin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeMarginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeMargin,
        )
        .and_then(|p| p.as_shape_margin())
    }

    // Method for getting shape-image-threshold property
    pub fn get_shape_image_threshold<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeImageThresholdValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeImageThreshold,
        )
        .and_then(|p| p.as_shape_image_threshold())
    }

    // Method for getting content property
    pub fn get_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Content)
            .and_then(|p| p.as_content())
    }

    // Method for getting counter-reset property
    pub fn get_counter_reset<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CounterResetValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CounterReset,
        )
        .and_then(|p| p.as_counter_reset())
    }

    // Method for getting counter-increment property
    pub fn get_counter_increment<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CounterIncrementValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CounterIncrement,
        )
        .and_then(|p| p.as_counter_increment())
    }

    // Method for getting string-set property
    pub fn get_string_set<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StringSetValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::StringSet)
            .and_then(|p| p.as_string_set())
    }
    pub fn get_text_align<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextAlignValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextAlign)
            .and_then(|p| p.as_text_align())
    }
    pub fn get_line_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLineHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::LineHeight)
            .and_then(|p| p.as_line_height())
    }
    pub fn get_letter_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLetterSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::LetterSpacing,
        )
        .and_then(|p| p.as_letter_spacing())
    }
    pub fn get_word_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWordSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::WordSpacing,
        )
        .and_then(|p| p.as_word_spacing())
    }
    pub fn get_tab_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTabWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TabWidth)
            .and_then(|p| p.as_tab_width())
    }
    pub fn get_cursor<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleCursorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Cursor)
            .and_then(|p| p.as_cursor())
    }
    pub fn get_box_shadow_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowLeft,
        )
        .and_then(|p| p.as_box_shadow_left())
    }
    pub fn get_box_shadow_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowRight,
        )
        .and_then(|p| p.as_box_shadow_right())
    }
    pub fn get_box_shadow_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowTop,
        )
        .and_then(|p| p.as_box_shadow_top())
    }
    pub fn get_box_shadow_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowBottom,
        )
        .and_then(|p| p.as_box_shadow_bottom())
    }
    pub fn get_border_top_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopColor,
        )
        .and_then(|p| p.as_border_top_color())
    }
    pub fn get_border_left_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderLeftColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftColor,
        )
        .and_then(|p| p.as_border_left_color())
    }
    pub fn get_border_right_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderRightColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightColor,
        )
        .and_then(|p| p.as_border_right_color())
    }
    pub fn get_border_bottom_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomColor,
        )
        .and_then(|p| p.as_border_bottom_color())
    }
    pub fn get_border_top_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopStyle,
        )
        .and_then(|p| p.as_border_top_style())
    }
    pub fn get_border_left_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderLeftStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftStyle,
        )
        .and_then(|p| p.as_border_left_style())
    }
    pub fn get_border_right_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderRightStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightStyle,
        )
        .and_then(|p| p.as_border_right_style())
    }
    pub fn get_border_bottom_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomStyle,
        )
        .and_then(|p| p.as_border_bottom_style())
    }
    pub fn get_border_top_left_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopLeftRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopLeftRadius,
        )
        .and_then(|p| p.as_border_top_left_radius())
    }
    pub fn get_border_top_right_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopRightRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopRightRadius,
        )
        .and_then(|p| p.as_border_top_right_radius())
    }
    pub fn get_border_bottom_left_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomLeftRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomLeftRadius,
        )
        .and_then(|p| p.as_border_bottom_left_radius())
    }
    pub fn get_border_bottom_right_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomRightRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomRightRadius,
        )
        .and_then(|p| p.as_border_bottom_right_radius())
    }
    pub fn get_opacity<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleOpacityValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Opacity)
            .and_then(|p| p.as_opacity())
    }
    pub fn get_transform<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTransformVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Transform)
            .and_then(|p| p.as_transform())
    }
    pub fn get_transform_origin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTransformOriginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TransformOrigin,
        )
        .and_then(|p| p.as_transform_origin())
    }
    pub fn get_perspective_origin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StylePerspectiveOriginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PerspectiveOrigin,
        )
        .and_then(|p| p.as_perspective_origin())
    }
    pub fn get_backface_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackfaceVisibilityValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackfaceVisibility,
        )
        .and_then(|p| p.as_backface_visibility())
    }
    pub fn get_display<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutDisplayValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Display)
            .and_then(|p| p.as_display())
    }
    pub fn get_float<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFloatValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Float)
            .and_then(|p| p.as_float())
    }
    pub fn get_box_sizing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBoxSizingValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BoxSizing)
            .and_then(|p| p.as_box_sizing())
    }
    pub fn get_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Width)
            .and_then(|p| p.as_width())
    }
    pub fn get_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Height)
            .and_then(|p| p.as_height())
    }
    pub fn get_min_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMinWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MinWidth)
            .and_then(|p| p.as_min_width())
    }
    pub fn get_min_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMinHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MinHeight)
            .and_then(|p| p.as_min_height())
    }
    pub fn get_max_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMaxWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MaxWidth)
            .and_then(|p| p.as_max_width())
    }
    pub fn get_max_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMaxHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MaxHeight)
            .and_then(|p| p.as_max_height())
    }
    pub fn get_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPositionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Position)
            .and_then(|p| p.as_position())
    }
    pub fn get_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Top)
            .and_then(|p| p.as_top())
    }
    pub fn get_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBottomValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Bottom)
            .and_then(|p| p.as_bottom())
    }
    pub fn get_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutRightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Right)
            .and_then(|p| p.as_right())
    }
    pub fn get_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutLeftValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Left)
            .and_then(|p| p.as_left())
    }
    pub fn get_padding_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::PaddingTop)
            .and_then(|p| p.as_padding_top())
    }
    pub fn get_padding_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingBottomValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingBottom,
        )
        .and_then(|p| p.as_padding_bottom())
    }
    pub fn get_padding_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingLeftValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingLeft,
        )
        .and_then(|p| p.as_padding_left())
    }
    pub fn get_padding_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingRightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingRight,
        )
        .and_then(|p| p.as_padding_right())
    }
    pub fn get_margin_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MarginTop)
            .and_then(|p| p.as_margin_top())
    }
    pub fn get_margin_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginBottomValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MarginBottom,
        )
        .and_then(|p| p.as_margin_bottom())
    }
    pub fn get_margin_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginLeftValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MarginLeft)
            .and_then(|p| p.as_margin_left())
    }
    pub fn get_margin_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginRightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MarginRight,
        )
        .and_then(|p| p.as_margin_right())
    }
    pub fn get_border_top_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderTopWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopWidth,
        )
        .and_then(|p| p.as_border_top_width())
    }
    pub fn get_border_left_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderLeftWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftWidth,
        )
        .and_then(|p| p.as_border_left_width())
    }
    pub fn get_border_right_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderRightWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightWidth,
        )
        .and_then(|p| p.as_border_right_width())
    }
    pub fn get_border_bottom_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderBottomWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomWidth,
        )
        .and_then(|p| p.as_border_bottom_width())
    }
    pub fn get_overflow_x<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowX)
            .and_then(|p| p.as_overflow_x())
    }
    pub fn get_overflow_y<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowY)
            .and_then(|p| p.as_overflow_y())
    }
    pub fn get_flex_direction<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexDirectionValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::FlexDirection,
        )
        .and_then(|p| p.as_flex_direction())
    }
    pub fn get_flex_wrap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexWrapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexWrap)
            .and_then(|p| p.as_flex_wrap())
    }
    pub fn get_flex_grow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexGrowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexGrow)
            .and_then(|p| p.as_flex_grow())
    }
    pub fn get_flex_shrink<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexShrinkValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexShrink)
            .and_then(|p| p.as_flex_shrink())
    }
    pub fn get_justify_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifyContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifyContent,
        )
        .and_then(|p| p.as_justify_content())
    }
    pub fn get_align_items<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignItemsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignItems)
            .and_then(|p| p.as_align_items())
    }
    pub fn get_align_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::AlignContent,
        )
        .and_then(|p| p.as_align_content())
    }
    pub fn get_mix_blend_mode<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleMixBlendModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MixBlendMode,
        )
        .and_then(|p| p.as_mix_blend_mode())
    }
    pub fn get_filter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFilterVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Filter)
            .and_then(|p| p.as_filter())
    }
    pub fn get_backdrop_filter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFilterVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Filter)
            .and_then(|p| p.as_backdrop_filter())
    }
    pub fn get_text_shadow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextShadow)
            .and_then(|p| p.as_text_shadow())
    }

    // Width calculation methods
    pub fn calc_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_width(node_data, node_id, styled_node_state)
            .and_then(|w| Some(w.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_min_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_min_width(node_data, node_id, styled_node_state)
            .and_then(|w| Some(w.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_max_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_max_width(node_data, node_id, styled_node_state)
            .and_then(|w| Some(w.get_property()?.inner.to_pixels(reference_width)))
    }

    // Height calculation methods
    pub fn calc_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_height(node_data, node_id, styled_node_state)
            .and_then(|h| Some(h.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_min_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_min_height(node_data, node_id, styled_node_state)
            .and_then(|h| Some(h.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_max_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_max_height(node_data, node_id, styled_node_state)
            .and_then(|h| Some(h.get_property()?.inner.to_pixels(reference_height)))
    }

    // Position calculation methods
    pub fn calc_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_left(node_data, node_id, styled_node_state)
            .and_then(|l| Some(l.get_property()?.inner.to_pixels(reference_width)))
    }

    pub fn calc_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_right(node_data, node_id, styled_node_state)
            .and_then(|r| Some(r.get_property()?.inner.to_pixels(reference_width)))
    }

    pub fn calc_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_top(node_data, node_id, styled_node_state)
            .and_then(|t| Some(t.get_property()?.inner.to_pixels(reference_height)))
    }

    pub fn calc_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_bottom(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_height)))
    }

    // Border calculation methods
    pub fn calc_border_left_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_border_left_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_border_right_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_border_right_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_border_top_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_border_top_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_border_bottom_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_border_bottom_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    // Padding calculation methods
    pub fn calc_padding_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_padding_left(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_padding_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_padding_right(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_padding_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_padding_top(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_padding_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_padding_bottom(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    // Margin calculation methods
    pub fn calc_margin_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_margin_left(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_margin_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_margin_right(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_margin_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_margin_top(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_margin_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_margin_bottom(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }
}

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

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct NodeHierarchyItemId {
    pub inner: usize,
}

impl fmt::Debug for NodeHierarchyItemId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.into_crate_internal() {
            Some(n) => write!(f, "Some(NodeId({}))", n),
            None => write!(f, "None"),
        }
    }
}

impl fmt::Display for NodeHierarchyItemId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl NodeHierarchyItemId {
    pub const NONE: NodeHierarchyItemId = NodeHierarchyItemId { inner: 0 };
}

impl_option!(
    NodeHierarchyItemId,
    OptionNodeId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(NodeHierarchyItemId, NodeIdVec, NodeIdVecDestructor);
impl_vec_mut!(NodeHierarchyItemId, NodeIdVec);
impl_vec_debug!(NodeHierarchyItemId, NodeIdVec);
impl_vec_ord!(NodeHierarchyItemId, NodeIdVec);
impl_vec_eq!(NodeHierarchyItemId, NodeIdVec);
impl_vec_hash!(NodeHierarchyItemId, NodeIdVec);
impl_vec_partialord!(NodeHierarchyItemId, NodeIdVec);
impl_vec_clone!(NodeHierarchyItemId, NodeIdVec, NodeIdVecDestructor);
impl_vec_partialeq!(NodeHierarchyItemId, NodeIdVec);

impl NodeHierarchyItemId {
    #[inline]
    pub const fn into_crate_internal(&self) -> Option<NodeId> {
        NodeId::from_usize(self.inner)
    }

    #[inline]
    pub const fn from_crate_internal(t: Option<NodeId>) -> Self {
        Self {
            inner: NodeId::into_usize(&t),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzTagId {
    pub inner: u64,
}

impl_option!(
    AzTagId,
    OptionTagId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl AzTagId {
    pub const fn into_crate_internal(&self) -> TagId {
        TagId(self.inner)
    }
    pub const fn from_crate_internal(t: TagId) -> Self {
        AzTagId { inner: t.0 }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct NodeHierarchyItem {
    pub parent: usize,
    pub previous_sibling: usize,
    pub next_sibling: usize,
    pub last_child: usize,
}

impl NodeHierarchyItem {
    pub const fn zeroed() -> Self {
        Self {
            parent: 0,
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 0,
        }
    }
}

impl From<Node> for NodeHierarchyItem {
    fn from(node: Node) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: NodeId::into_usize(&node.parent),
            previous_sibling: NodeId::into_usize(&node.previous_sibling),
            next_sibling: NodeId::into_usize(&node.next_sibling),
            last_child: NodeId::into_usize(&node.last_child),
        }
    }
}

impl NodeHierarchyItem {
    pub fn parent_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.parent)
    }
    pub fn previous_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.previous_sibling)
    }
    pub fn next_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.next_sibling)
    }
    pub fn first_child_id(&self, current_node_id: NodeId) -> Option<NodeId> {
        self.last_child_id().map(|_| current_node_id + 1)
    }
    pub fn last_child_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.last_child)
    }
}

impl_vec!(
    NodeHierarchyItem,
    NodeHierarchyItemVec,
    NodeHierarchyItemVecDestructor
);
impl_vec_mut!(NodeHierarchyItem, NodeHierarchyItemVec);
impl_vec_debug!(AzNode, NodeHierarchyItemVec);
impl_vec_partialord!(AzNode, NodeHierarchyItemVec);
impl_vec_clone!(
    NodeHierarchyItem,
    NodeHierarchyItemVec,
    NodeHierarchyItemVecDestructor
);
impl_vec_partialeq!(AzNode, NodeHierarchyItemVec);

impl NodeHierarchyItemVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeHierarchyItem> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeHierarchyItem> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

impl<'a> NodeDataContainerRef<'a, NodeHierarchyItem> {
    #[inline]
    pub fn subtree_len(&self, parent_id: NodeId) -> usize {
        let self_item_index = parent_id.index();
        let next_item_index = match self[parent_id].next_sibling_id() {
            None => self.len(),
            Some(s) => s.index(),
        };
        next_item_index - self_item_index - 1
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct ParentWithNodeDepth {
    pub depth: usize,
    pub node_id: NodeHierarchyItemId,
}

impl core::fmt::Debug for ParentWithNodeDepth {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{{ depth: {}, node: {:?} }}",
            self.depth,
            self.node_id.into_crate_internal()
        )
    }
}

impl_vec!(
    ParentWithNodeDepth,
    ParentWithNodeDepthVec,
    ParentWithNodeDepthVecDestructor
);
impl_vec_mut!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_debug!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialord!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_clone!(
    ParentWithNodeDepth,
    ParentWithNodeDepthVec,
    ParentWithNodeDepthVecDestructor
);
impl_vec_partialeq!(ParentWithNodeDepth, ParentWithNodeDepthVec);

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct TagIdToNodeIdMapping {
    // Hit-testing tag ID (not all nodes have a tag, only nodes that are hit-testable)
    pub tag_id: AzTagId,
    /// Node ID of the node that has a tag
    pub node_id: NodeHierarchyItemId,
    /// Whether this node has a tab-index field
    pub tab_index: OptionTabIndex,
    /// Parents of this NodeID, sorted in depth order, necessary for efficient hit-testing
    pub parent_node_ids: NodeIdVec,
}

impl_vec!(
    TagIdToNodeIdMapping,
    TagIdToNodeIdMappingVec,
    TagIdToNodeIdMappingVecDestructor
);
impl_vec_mut!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);
impl_vec_debug!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);
impl_vec_partialord!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);
impl_vec_clone!(
    TagIdToNodeIdMapping,
    TagIdToNodeIdMappingVec,
    TagIdToNodeIdMappingVecDestructor
);
impl_vec_partialeq!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    pub root: NodeHierarchyItemId,
    /// Node ids in order of drawing
    pub children: ContentGroupVec,
}

impl_vec!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor);
impl_vec_mut!(ContentGroup, ContentGroupVec);
impl_vec_debug!(ContentGroup, ContentGroupVec);
impl_vec_partialord!(ContentGroup, ContentGroupVec);
impl_vec_clone!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor);
impl_vec_partialeq!(ContentGroup, ContentGroupVec);

#[derive(Debug, PartialEq, Clone)]
#[repr(C)]
pub struct StyledDom {
    pub root: NodeHierarchyItemId,
    pub node_hierarchy: NodeHierarchyItemVec,
    pub node_data: NodeDataVec,
    pub styled_nodes: StyledNodeVec,
    pub cascade_info: CascadeInfoVec,
    pub nodes_with_window_callbacks: NodeIdVec,
    pub nodes_with_not_callbacks: NodeIdVec,
    pub nodes_with_datasets: NodeIdVec,
    pub tag_ids_to_node_ids: TagIdToNodeIdMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub css_property_cache: CssPropertyCachePtr,
}

impl Default for StyledDom {
    fn default() -> Self {
        let root_node: NodeHierarchyItem = Node::ROOT.into();
        let root_node_id: NodeHierarchyItemId =
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO));
        Self {
            root: root_node_id,
            node_hierarchy: vec![root_node].into(),
            node_data: vec![NodeData::body()].into(),
            styled_nodes: vec![StyledNode::default()].into(),
            cascade_info: vec![CascadeInfo {
                index_in_parent: 0,
                is_last_child: true,
            }]
            .into(),
            tag_ids_to_node_ids: Vec::new().into(),
            non_leaf_nodes: vec![ParentWithNodeDepth {
                depth: 0,
                node_id: root_node_id,
            }]
            .into(),
            nodes_with_window_callbacks: Vec::new().into(),
            nodes_with_not_callbacks: Vec::new().into(),
            nodes_with_datasets: Vec::new().into(),
            css_property_cache: CssPropertyCachePtr::new(CssPropertyCache::empty(1)),
        }
    }
}

impl StyledDom {
    // NOTE: After calling this function, the DOM will be reset to an empty DOM.
    // This is for memory optimization, so that the DOM does not need to be cloned.
    //
    // The CSS will be left in-place, but will be re-ordered
    pub fn new(dom: &mut Dom, mut css: CssApiWrapper) -> Self {
        use core::mem;

        use crate::dom::EventFilter;

        let mut swap_dom = Dom::body();

        mem::swap(dom, &mut swap_dom);

        let compact_dom: CompactDom = swap_dom.into();
        let non_leaf_nodes = compact_dom
            .node_hierarchy
            .as_ref()
            .get_parents_sorted_by_depth();
        let node_hierarchy: NodeHierarchyItemVec = compact_dom
            .node_hierarchy
            .as_ref()
            .internal
            .iter()
            .map(|i| (*i).into())
            .collect::<Vec<NodeHierarchyItem>>()
            .into();

        let mut styled_nodes = vec![
            StyledNode {
                tag_id: OptionTagId::None,
                state: StyledNodeState::new()
            };
            compact_dom.len()
        ];

        // fill out the css property cache: compute the inline properties first so that
        // we can early-return in case the css is empty

        let mut css_property_cache = CssPropertyCache::empty(compact_dom.node_data.len());

        let html_tree =
            construct_html_cascade_tree(&compact_dom.node_hierarchy.as_ref(), &non_leaf_nodes[..]);

        let non_leaf_nodes = non_leaf_nodes
            .iter()
            .map(|(depth, node_id)| ParentWithNodeDepth {
                depth: *depth,
                node_id: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
            })
            .collect::<Vec<_>>();

        let non_leaf_nodes: ParentWithNodeDepthVec = non_leaf_nodes.into();

        // apply all the styles from the CSS
        let tag_ids = css_property_cache.restyle(
            &mut css.css,
            &compact_dom.node_data.as_ref(),
            &node_hierarchy,
            &non_leaf_nodes,
            &html_tree.as_ref(),
        );

        tag_ids
            .iter()
            .filter_map(|tag_id_node_id_mapping| {
                tag_id_node_id_mapping
                    .node_id
                    .into_crate_internal()
                    .map(|node_id| (node_id, tag_id_node_id_mapping.tag_id))
            })
            .for_each(|(nid, tag_id)| {
                styled_nodes[nid.index()].tag_id = OptionTagId::Some(tag_id);
            });

        // Pre-filter all EventFilter::Window and EventFilter::Not nodes
        // since we need them in the CallbacksOfHitTest::new function
        let nodes_with_window_callbacks = compact_dom
            .node_data
            .as_ref()
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, c)| {
                let node_has_none_callbacks = c.get_callbacks().iter().any(|cb| match cb.event {
                    EventFilter::Window(_) => true,
                    _ => false,
                });
                if node_has_none_callbacks {
                    Some(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                        node_id,
                    ))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let nodes_with_not_callbacks = compact_dom
            .node_data
            .as_ref()
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, c)| {
                let node_has_none_callbacks = c.get_callbacks().iter().any(|cb| match cb.event {
                    EventFilter::Not(_) => true,
                    _ => false,
                });
                if node_has_none_callbacks {
                    Some(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                        node_id,
                    ))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // collect nodes with either dataset or callback properties
        let nodes_with_datasets = compact_dom
            .node_data
            .as_ref()
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, c)| {
                if !c.get_callbacks().is_empty() || c.get_dataset().is_some() {
                    Some(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                        node_id,
                    ))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        StyledDom {
            root: NodeHierarchyItemId::from_crate_internal(Some(compact_dom.root)),
            node_hierarchy,
            node_data: compact_dom.node_data.internal.into(),
            cascade_info: html_tree.internal.into(),
            styled_nodes: styled_nodes.into(),
            tag_ids_to_node_ids: tag_ids.into(),
            nodes_with_window_callbacks: nodes_with_window_callbacks.into(),
            nodes_with_not_callbacks: nodes_with_not_callbacks.into(),
            nodes_with_datasets: nodes_with_datasets.into(),
            non_leaf_nodes,
            css_property_cache: CssPropertyCachePtr::new(css_property_cache),
        }
    }

    /// Appends another `StyledDom` as a child to the `self.root`
    /// without re-styling the DOM itself
    pub fn append_child(&mut self, mut other: Self) {
        // shift all the node ids in other by self.len()
        let self_len = self.node_hierarchy.as_ref().len();
        let other_len = other.node_hierarchy.as_ref().len();
        let self_tag_len = self.tag_ids_to_node_ids.as_ref().len();
        let self_root_id = self.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        let other_root_id = other.root.into_crate_internal().unwrap_or(NodeId::ZERO);

        // iterate through the direct root children and adjust the cascade_info
        let current_root_children_count = self_root_id
            .az_children(&self.node_hierarchy.as_container())
            .count();

        other.cascade_info.as_mut()[other_root_id.index()].index_in_parent =
            current_root_children_count as u32;
        other.cascade_info.as_mut()[other_root_id.index()].is_last_child = true;

        self.cascade_info.append(&mut other.cascade_info);

        // adjust node hierarchy
        for other in other.node_hierarchy.as_mut().iter_mut() {
            other.parent += self_len;
            other.previous_sibling += if other.previous_sibling == 0 {
                0
            } else {
                self_len
            };
            other.next_sibling += if other.next_sibling == 0 { 0 } else { self_len };
            other.last_child += if other.last_child == 0 { 0 } else { self_len };
        }

        other.node_hierarchy.as_container_mut()[other_root_id].parent =
            NodeId::into_usize(&Some(self_root_id));
        let current_last_child = self.node_hierarchy.as_container()[self_root_id].last_child_id();
        other.node_hierarchy.as_container_mut()[other_root_id].previous_sibling =
            NodeId::into_usize(&current_last_child);
        if let Some(current_last) = current_last_child {
            if self.node_hierarchy.as_container_mut()[current_last]
                .next_sibling_id()
                .is_some()
            {
                self.node_hierarchy.as_container_mut()[current_last].next_sibling +=
                    other_root_id.index() + 1;
            } else {
                self.node_hierarchy.as_container_mut()[current_last].next_sibling =
                    self_len + other_root_id.index() + 1;
            }
        }
        self.node_hierarchy.as_container_mut()[self_root_id].last_child =
            self_len + other_root_id.index() + 1;

        self.node_hierarchy.append(&mut other.node_hierarchy);
        self.node_data.append(&mut other.node_data);
        self.styled_nodes.append(&mut other.styled_nodes);
        self.get_css_property_cache_mut()
            .append(other.get_css_property_cache_mut());

        for tag_id_node_id in other.tag_ids_to_node_ids.iter_mut() {
            tag_id_node_id.tag_id.inner += self_tag_len as u64;
            tag_id_node_id.node_id.inner += self_len;
        }

        self.tag_ids_to_node_ids
            .append(&mut other.tag_ids_to_node_ids);

        for nid in other.nodes_with_window_callbacks.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_window_callbacks
            .append(&mut other.nodes_with_window_callbacks);

        for nid in other.nodes_with_not_callbacks.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_not_callbacks
            .append(&mut other.nodes_with_not_callbacks);

        for nid in other.nodes_with_datasets.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_datasets
            .append(&mut other.nodes_with_datasets);

        // edge case: if the other StyledDom consists of only one node
        // then it is not a parent itself
        if other_len != 1 {
            for other_non_leaf_node in other.non_leaf_nodes.iter_mut() {
                other_non_leaf_node.node_id.inner += self_len;
                other_non_leaf_node.depth += 1;
            }
            self.non_leaf_nodes.append(&mut other.non_leaf_nodes);
            self.non_leaf_nodes.sort_by(|a, b| a.depth.cmp(&b.depth));
        }
    }

    /// Inject scroll bar DIVs with relevant event handlers into the DOM
    ///
    /// This function essentially takes a DOM and inserts a wrapper DIV
    /// on every parent. First, all scrollbars are set to "display:none;"
    /// with a special library-internal marker that indicates that this
    /// DIV is a scrollbar. Then later on in the layout code, the items
    /// are set to "display: flex / block" as necessary, because
    /// this way scrollbars aren't treated as "special" objects (the event
    /// handling for scrollbars are just regular callback handlers).
    pub fn inject_scroll_bars(&mut self) {
        use azul_css::parser2::CssApiWrapper;

        // allocate 14 nodes for every node
        //
        // 0: root component
        // 1: |- vertical container (flex-direction: column-reverse, flex-grow: 1)
        // 2:    |- horizontal scrollbar (height: 15px, flex-direction: row)
        // 3:    |  |- left thumb
        // 4:    |  |- middle content
        // 5:    |  |   |- thumb track
        // 6:    |  |- right thumb
        // 7:    |- content container (flex-direction: row-reverse, flex-grow: 1)
        // 8:       |- vertical scrollbar (width: 15px, flex-direction: column)
        // 9:       |   |- top thumb
        // 10:      |   |- middle content
        // 11:      |   |    |- thumb track
        // 12:      |   |- bottom thumb
        // 13:      |- content container (flex-direction: row, flex-grow: 1)
        // 14:          |- self.root
        //                  |- ... self.children

        let dom_to_inject = Dom::div()
        // .with_class("__azul-native-scroll-root-component".into())
        .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
        .with_children(vec![

            Dom::div()
            // .with_class("__azul-native-scroll-vertical-container".into())
            .with_inline_style("display:flex; flex-grow:1; flex-direction:column-reverse;".into())
            .with_children(vec![

                Dom::div()
                // .with_class("__azul-native-scroll-horizontal-scrollbar".into())
                .with_inline_style("display:flex; flex-grow:1; flex-direction:row; height:15px; background:grey;".into())
                .with_children(vec![
                    Dom::div(),
                    // .with_class("__azul-native-scroll-horizontal-scrollbar-track-left".into()),
                    Dom::div()
                    // .with_class("__azul-native-scroll-horizontal-scrollbar-track-middle".into())
                    .with_children(vec![
                        Dom::div()
                        // .with_class("__azul-native-scroll-horizontal-scrollbar-track-thumb".into())
                    ].into()),
                    Dom::div()
                    // .with_class("__azul-native-scroll-horizontal-scrollbar-track-right".into()),
                ].into()),

                Dom::div()
                // .with_class("__azul-native-scroll-content-container-1".into())
                .with_inline_style("display:flex; flex-grow:1; flex-direction:row-reverse;".into())
                .with_children(vec![

                    Dom::div()
                    // .with_class("__azul-native-scroll-vertical-scrollbar".into())
                    .with_inline_style("display:flex; flex-grow:1; flex-direction:column; width:15px; background:grey;".into())
                    .with_children(vec![
                       Dom::div(),
                       // .with_class("__azul-native-scroll-vertical-scrollbar-track-top".into()),
                       Dom::div()
                       // .with_class("__azul-native-scroll-vertical-scrollbar-track-middle".into())
                       .with_children(vec![
                           Dom::div()
                           // .with_class("__azul-native-scroll-vertical-scrollbar-track-thumb".into())
                       ].into()),
                       Dom::div()
                       // .with_class("__azul-native-scroll-vertical-scrollbar-track-bottom".into()),
                    ].into()),

                    Dom::div()
                    // .with_class("__azul-native-scroll-content-container-1".into())
                    .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
                    .with_children(vec![
                        Dom::div() // <- this div is where the new children will be injected into
                    ].into())
                ].into())
            ].into())
        ].into())
        .style(CssApiWrapper::empty());

        // allocate new nodes
        let nodes_to_allocate =
            self.node_data.len() + (self.non_leaf_nodes.len() * dom_to_inject.node_data.len());

        // pre-allocate a new DOM tree with self.nodes.len() * dom_to_inject.nodes.len() nodes

        let mut new_styled_dom = StyledDom {
            root: self.root,
            node_hierarchy: vec![NodeHierarchyItem::zeroed(); nodes_to_allocate].into(),
            node_data: vec![NodeData::default(); nodes_to_allocate].into(),
            styled_nodes: vec![StyledNode::default(); nodes_to_allocate].into(),
            cascade_info: vec![CascadeInfo::default(); nodes_to_allocate].into(),
            nodes_with_window_callbacks: self.nodes_with_window_callbacks.clone(),
            nodes_with_not_callbacks: self.nodes_with_not_callbacks.clone(),
            nodes_with_datasets: self.nodes_with_datasets.clone(),
            tag_ids_to_node_ids: self.tag_ids_to_node_ids.clone(),
            non_leaf_nodes: self.non_leaf_nodes.clone(),
            css_property_cache: self.css_property_cache.clone(),
        };

        // inject self.root as the nth node
        let inject_as_id = 0;

        #[cfg(feature = "std")]
        {
            println!(
                "inject scroll bars:\r\n{}",
                dom_to_inject.get_html_string("", "", true)
            );
        }

        // *self = new_styled_dom;
    }

    /// Inject a menu bar into the root component
    pub fn inject_menu_bar(mut self, menu_bar: &Menu) -> Self {
        use azul_css::parser2::CssApiWrapper;

        use crate::window::MenuItem;

        let menu_dom = menu_bar
            .items
            .as_ref()
            .iter()
            .map(|mi| match mi {
                MenuItem::String(smi) => Dom::text(smi.label.clone().into_library_owned_string())
                    .with_inline_style("font-family:sans-serif;".into()),
                MenuItem::Separator => {
                    Dom::div().with_inline_style("padding:1px;background:grey;".into())
                }
                MenuItem::BreakLine => Dom::div(),
            })
            .collect::<Dom>()
            .with_inline_style(
                "
            height:20px;
            display:flex;
            flex-direction:row;"
                    .into(),
            )
            .style(CssApiWrapper::empty());

        let mut core_container = Dom::body().style(CssApiWrapper::empty());
        core_container.append_child(menu_dom);
        core_container.append_child(self);
        core_container
    }

    /// Same as `append_child()`, but as a builder method
    pub fn with_child(&mut self, other: Self) -> Self {
        let mut s = self.swap_with_default();
        s.append_child(other);
        s
    }

    pub fn restyle(&mut self, mut css: CssApiWrapper) {
        let new_tag_ids = self.css_property_cache.downcast_mut().restyle(
            &mut css.css,
            &self.node_data.as_container(),
            &self.node_hierarchy,
            &self.non_leaf_nodes,
            &self.cascade_info.as_container(),
        );

        // Restyling may change the tag IDs
        let mut styled_nodes_mut = self.styled_nodes.as_container_mut();

        styled_nodes_mut
            .internal
            .iter_mut()
            .for_each(|styled_node| {
                styled_node.tag_id = None.into();
            });

        new_tag_ids
            .iter()
            .filter_map(|tag_id_node_id_mapping| {
                tag_id_node_id_mapping
                    .node_id
                    .into_crate_internal()
                    .map(|node_id| (node_id, tag_id_node_id_mapping.tag_id))
            })
            .for_each(|(nid, tag_id)| {
                styled_nodes_mut[nid].tag_id = Some(tag_id).into();
            });

        self.tag_ids_to_node_ids = new_tag_ids.into();
    }

    /// Inserts default On::Scroll and On::Tab handle for scroll-able
    /// and tabindex-able nodes.
    #[inline]
    pub fn insert_default_system_callbacks(&mut self, config: DefaultCallbacksCfg) {
        use crate::{
            callbacks::Callback,
            dom::{CallbackData, EventFilter, FocusEventFilter, HoverEventFilter},
        };

        let scroll_refany = RefAny::new(DefaultScrollCallbackData {
            smooth_scroll: config.smooth_scroll,
        });

        for n in self.node_data.iter_mut() {
            // TODO: ScrollStart / ScrollEnd?
            if !n
                .callbacks
                .iter()
                .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Scroll))
            {
                n.callbacks.push(CallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Scroll),
                    data: scroll_refany.clone(),
                    callback: Callback {
                        cb: default_on_scroll,
                    },
                });
            }
        }

        if !config.enable_autotab {
            return;
        }

        let tab_data = RefAny::new(DefaultTabIndexCallbackData {});
        for focusable_node in self.tag_ids_to_node_ids.iter() {
            if focusable_node.tab_index.is_some() {
                let focusable_node_id = match focusable_node.node_id.into_crate_internal() {
                    Some(s) => s,
                    None => continue,
                };

                let mut node_data = &mut self.node_data.as_container_mut()[focusable_node_id];
                if !node_data
                    .callbacks
                    .iter()
                    .any(|cb| cb.event == EventFilter::Focus(FocusEventFilter::VirtualKeyDown))
                {
                    node_data.callbacks.push(CallbackData {
                        event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                        data: tab_data.clone(),
                        callback: Callback {
                            cb: default_on_tabindex,
                        },
                    });
                }
            }
        }
    }

    #[inline]
    pub fn node_count(&self) -> usize {
        self.node_data.len()
    }

    #[inline]
    pub fn get_css_property_cache<'a>(&'a self) -> &'a CssPropertyCache {
        &*self.css_property_cache.ptr
    }

    #[inline]
    pub fn get_css_property_cache_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        &mut *self.css_property_cache.ptr
    }

    #[inline]
    pub fn get_styled_node_state(&self, node_id: &NodeId) -> StyledNodeState {
        self.styled_nodes.as_container()[*node_id].state.clone()
    }

    /// Scans the display list for all font IDs + their font size
    pub fn scan_for_font_keys(
        &self,
        resources: &RendererResources,
    ) -> FastHashMap<ImmediateFontId, FastBTreeSet<Au>> {
        use crate::{resources::font_size_to_au, dom::NodeType::*};

        let keys = self
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);
                match node_data.get_node_type() {
                    Text(_) => {
                        let css_font_ids = self.get_css_property_cache().get_font_id_or_default(
                            &node_data,
                            &node_id,
                            &self.styled_nodes.as_container()[node_id].state,
                        );

                        let font_size = self.get_css_property_cache().get_font_size_or_default(
                            &node_data,
                            &node_id,
                            &self.styled_nodes.as_container()[node_id].state,
                        );

                        let style_font_families_hash =
                            StyleFontFamiliesHash::new(css_font_ids.as_ref());

                        let existing_font_key = resources
                            .get_font_family(&style_font_families_hash)
                            .and_then(|font_family_hash| {
                                resources
                                    .get_font_key(&font_family_hash)
                                    .map(|font_key| (font_family_hash, font_key))
                            });

                        let font_id = match existing_font_key {
                            Some((hash, key)) => ImmediateFontId::Resolved((*hash, *key)),
                            None => ImmediateFontId::Unresolved(css_font_ids),
                        };

                        Some((font_id, font_size_to_au(font_size)))
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        let mut map = FastHashMap::default();

        for (font_id, au) in keys.into_iter() {
            map.entry(font_id)
                .or_insert_with(|| FastBTreeSet::default())
                .insert(au);
        }

        map
    }

    /// Scans the display list for all image keys
    pub fn scan_for_image_keys(&self, css_image_cache: &ImageCache) -> FastBTreeSet<ImageRef> {
        use azul_css::props::style::StyleBackgroundContentVec;

        use crate::{resources::OptionImageMask, dom::NodeType::*};

        #[derive(Default)]
        struct ScanImageVec {
            node_type_image: Option<ImageRef>,
            background_image: Vec<ImageRef>,
            clip_mask: Option<ImageRef>,
        }

        let default_backgrounds: StyleBackgroundContentVec = Vec::new().into();

        let images = self
            .node_data
            .as_container()
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);
                let mut v = ScanImageVec::default();

                // If the node has an image content, it needs to be uploaded
                if let Image(id) = node_data.get_node_type() {
                    v.node_type_image = Some(id.clone());
                }

                // If the node has a CSS background image, it needs to be uploaded
                let opt_background_image = self.get_css_property_cache().get_background_content(
                    &node_data,
                    &node_id,
                    &self.styled_nodes.as_container()[node_id].state,
                );

                if let Some(style_backgrounds) = opt_background_image {
                    v.background_image = style_backgrounds
                        .get_property()
                        .unwrap_or(&default_backgrounds)
                        .iter()
                        .filter_map(|bg| {
                            use azul_css::props::style::StyleBackgroundContent::*;
                            let css_image_id = match bg {
                                Image(i) => i,
                                _ => return None,
                            };
                            let image_ref = css_image_cache.get_css_image_id(css_image_id)?;
                            Some(image_ref.clone())
                        })
                        .collect();
                }

                // If the node has a clip mask, it needs to be uploaded
                if let Some(clip_mask) = node_data.get_clip_mask() {
                    v.clip_mask = Some(clip_mask.image.clone());
                }

                v
            })
            .collect::<Vec<_>>();

        let mut set = FastBTreeSet::new();

        for scan_image in images.into_iter() {
            if let Some(n) = scan_image.node_type_image {
                set.insert(n);
            }
            if let Some(n) = scan_image.clip_mask {
                set.insert(n);
            }
            for bg in scan_image.background_image {
                set.insert(bg);
            }
        }

        set
    }

    #[must_use]
    pub fn restyle_nodes_hover(
        &mut self,
        nodes: &[NodeId],
        new_hover_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| self.styled_nodes.as_container()[*nid].state.clone())
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid].state.hover = new_hover_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let default_map = BTreeMap::default();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = css_property_cache
                    .css_hover_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();
                let mut keys_inherited: Vec<_> = css_property_cache
                    .cascaded_hover_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();
                let keys_inline: Vec<CssPropertyType> = node_data[*node_id]
                    .inline_css_props
                    .iter()
                    .filter_map(|prop| match prop {
                        NodeDataInlineCssProperty::Hover(h) => Some(h.get_type()),
                        _ => None,
                    })
                    .collect();
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property(
                            node_data,
                            node_id,
                            new_node_state,
                            prop,
                        );
                        if old == new {
                            None
                        } else {
                            Some(ChangedCssProperty {
                                previous_state: old_node_state.clone(),
                                previous_prop: match old {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                                current_state: new_node_state.clone(),
                                current_prop: match new {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                            })
                        }
                    })
                    .collect::<Vec<_>>();

                if changes.is_empty() {
                    None
                } else {
                    Some((*node_id, changes))
                }
            })
            .collect::<Vec<_>>();

        v.into_iter().collect()
    }

    #[must_use]
    pub fn restyle_nodes_active(
        &mut self,
        nodes: &[NodeId],
        new_active_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| self.styled_nodes.as_container()[*nid].state.clone())
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid].state.active = new_active_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let default_map = BTreeMap::default();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = css_property_cache
                    .css_active_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let mut keys_inherited: Vec<_> = css_property_cache
                    .cascaded_active_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let keys_inline: Vec<CssPropertyType> = node_data[*node_id]
                    .inline_css_props
                    .iter()
                    .filter_map(|prop| match prop {
                        NodeDataInlineCssProperty::Active(h) => Some(h.get_type()),
                        _ => None,
                    })
                    .collect();
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property(
                            node_data,
                            node_id,
                            new_node_state,
                            prop,
                        );
                        if old == new {
                            None
                        } else {
                            Some(ChangedCssProperty {
                                previous_state: old_node_state.clone(),
                                previous_prop: match old {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                                current_state: new_node_state.clone(),
                                current_prop: match new {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                            })
                        }
                    })
                    .collect::<Vec<_>>();

                if changes.is_empty() {
                    None
                } else {
                    Some((*node_id, changes))
                }
            })
            .collect::<Vec<_>>();

        v.into_iter().collect()
    }

    #[must_use]
    pub fn restyle_nodes_focus(
        &mut self,
        nodes: &[NodeId],
        new_focus_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| self.styled_nodes.as_container()[*nid].state.clone())
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid].state.focused = new_focus_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let default_map = BTreeMap::default();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = css_property_cache
                    .css_focus_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let mut keys_inherited: Vec<_> = css_property_cache
                    .cascaded_focus_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let keys_inline: Vec<CssPropertyType> = node_data[*node_id]
                    .inline_css_props
                    .iter()
                    .filter_map(|prop| match prop {
                        NodeDataInlineCssProperty::Focus(h) => Some(h.get_type()),
                        _ => None,
                    })
                    .collect();
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property(
                            node_data,
                            node_id,
                            new_node_state,
                            prop,
                        );
                        if old == new {
                            None
                        } else {
                            Some(ChangedCssProperty {
                                previous_state: old_node_state.clone(),
                                previous_prop: match old {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                                current_state: new_node_state.clone(),
                                current_prop: match new {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                            })
                        }
                    })
                    .collect::<Vec<_>>();

                if changes.is_empty() {
                    None
                } else {
                    Some((*node_id, changes))
                }
            })
            .collect::<Vec<_>>();

        v.into_iter().collect()
    }

    // Inserts a property into the self.user_overridden_properties
    #[must_use]
    pub fn restyle_user_property(
        &mut self,
        node_id: &NodeId,
        new_properties: &[CssProperty],
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        let mut map = BTreeMap::default();

        if new_properties.is_empty() {
            return map;
        }

        let node_data = self.node_data.as_container();
        let node_data = &node_data[*node_id];

        let node_states = &self.styled_nodes.as_container();
        let old_node_state = &node_states[*node_id].state;

        let changes: Vec<ChangedCssProperty> = {
            let css_property_cache = self.get_css_property_cache();

            new_properties
                .iter()
                .filter_map(|new_prop| {
                    let old_prop = css_property_cache.get_property(
                        node_data,
                        node_id,
                        old_node_state,
                        &new_prop.get_type(),
                    );

                    let old_prop = match old_prop {
                        None => CssProperty::auto(new_prop.get_type()),
                        Some(s) => s.clone(),
                    };

                    if old_prop == *new_prop {
                        None
                    } else {
                        Some(ChangedCssProperty {
                            previous_state: old_node_state.clone(),
                            previous_prop: old_prop,
                            // overriding a user property does not change the state
                            current_state: old_node_state.clone(),
                            current_prop: new_prop.clone(),
                        })
                    }
                })
                .collect()
        };

        let css_property_cache_mut = self.get_css_property_cache_mut();

        for new_prop in new_properties.iter() {
            if new_prop.is_initial() {
                let mut should_remove_map = false;
                if let Some(map) = css_property_cache_mut
                    .user_overridden_properties
                    .get_mut(node_id)
                {
                    // CssProperty::Initial = remove overridden property
                    map.remove(&new_prop.get_type());
                    should_remove_map = map.is_empty();
                }
                if should_remove_map {
                    css_property_cache_mut
                        .user_overridden_properties
                        .remove(node_id);
                }
            } else {
                css_property_cache_mut
                    .user_overridden_properties
                    .entry(*node_id)
                    .or_insert_with(|| BTreeMap::new())
                    .insert(new_prop.get_type(), new_prop.clone());
            }
        }

        if !changes.is_empty() {
            map.insert(*node_id, changes);
        }

        map
    }

    /// Scans the `StyledDom` for iframe callbacks
    pub fn scan_for_iframe_callbacks(&self) -> Vec<NodeId> {
        use crate::dom::NodeType;
        self.node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| match node_data.get_node_type() {
                NodeType::IFrame(_) => Some(NodeId::new(node_id)),
                _ => None,
            })
            .collect()
    }

    /// Scans the `StyledDom` for OpenGL callbacks
    pub(crate) fn scan_for_gltexture_callbacks(&self) -> Vec<NodeId> {
        use crate::dom::NodeType;
        self.node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                use crate::resources::DecodedImage;
                match node_data.get_node_type() {
                    NodeType::Image(image_ref) => {
                        if let DecodedImage::Callback(_) = image_ref.get_data() {
                            Some(NodeId::new(node_id))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Returns a HTML-formatted version of the DOM for easier debugging, i.e.
    ///
    /// ```rust,no_run,ignore
    /// Dom::div().with_id("hello")
    ///     .with_child(Dom::div().with_id("test"))
    /// ```
    ///
    /// will return:
    ///
    /// ```xml,no_run,ignore
    /// <div id="hello">
    ///      <div id="test" />
    /// </div>
    /// ```
    pub fn get_html_string(&self, custom_head: &str, custom_body: &str, test_mode: bool) -> String {
        let css_property_cache = self.get_css_property_cache();

        let mut output = String::new();

        // After which nodes should a close tag be printed?
        let mut should_print_close_tag_after_node = BTreeMap::new();

        let should_print_close_tag_debug = self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| {
                let parent_node_id = p.node_id.into_crate_internal()?;
                let mut total_last_child = None;
                recursive_get_last_child(
                    parent_node_id,
                    &self.node_hierarchy.as_ref(),
                    &mut total_last_child,
                );
                let total_last_child = total_last_child?;
                Some((parent_node_id, (total_last_child, p.depth)))
            })
            .collect::<BTreeMap<_, _>>();

        for (parent_id, (last_child, parent_depth)) in should_print_close_tag_debug {
            should_print_close_tag_after_node
                .entry(last_child)
                .or_insert_with(|| Vec::new())
                .push((parent_id, parent_depth));
        }

        let mut all_node_depths = self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| {
                let parent_node_id = p.node_id.into_crate_internal()?;
                Some((parent_node_id, p.depth))
            })
            .collect::<BTreeMap<_, _>>();

        for (parent_node_id, parent_depth) in self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| Some((p.node_id.into_crate_internal()?, p.depth)))
        {
            for child_id in parent_node_id.az_children(&self.node_hierarchy.as_container()) {
                all_node_depths.insert(child_id, parent_depth + 1);
            }
        }

        for node_id in self.node_hierarchy.as_container().linear_iter() {
            let depth = all_node_depths[&node_id];

            let node_data = &self.node_data.as_container()[node_id];
            let node_state = &self.styled_nodes.as_container()[node_id].state;
            let tabs = String::from("    ").repeat(depth);

            output.push_str("\r\n");
            output.push_str(&tabs);
            output.push_str(&node_data.debug_print_start(css_property_cache, &node_id, node_state));

            if let Some(content) = node_data.get_node_type().format().as_ref() {
                output.push_str(content);
            }

            let node_has_children = self.node_hierarchy.as_container()[node_id]
                .first_child_id(node_id)
                .is_some();
            if !node_has_children {
                let node_data = &self.node_data.as_container()[node_id];
                output.push_str(&node_data.debug_print_end());
            }

            if let Some(close_tag_vec) = should_print_close_tag_after_node.get(&node_id) {
                let mut close_tag_vec = close_tag_vec.clone();
                close_tag_vec.sort_by(|a, b| b.1.cmp(&a.1)); // sort by depth descending
                for (close_tag_parent_id, close_tag_depth) in close_tag_vec {
                    let node_data = &self.node_data.as_container()[close_tag_parent_id];
                    let tabs = String::from("    ").repeat(close_tag_depth);
                    output.push_str("\r\n");
                    output.push_str(&tabs);
                    output.push_str(&node_data.debug_print_end());
                }
            }
        }

        if !test_mode {
            format!(
                "
                <html>
                    <head>
                    <style>* {{ margin:0px; padding:0px; }}</style>
                    {custom_head}
                    </head>
                {output}
                {custom_body}
                </html>
            "
            )
        } else {
            output
        }
    }

    /// Returns the node ID of all sub-children of a node
    pub fn get_subtree(&self, parent: NodeId) -> Vec<NodeId> {
        let mut total_last_child = None;
        recursive_get_last_child(parent, &self.node_hierarchy.as_ref(), &mut total_last_child);
        if let Some(last) = total_last_child {
            (parent.index()..=last.index())
                .map(|id| NodeId::new(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    // Same as get_subtree, but only returns parents
    pub fn get_subtree_parents(&self, parent: NodeId) -> Vec<NodeId> {
        let mut total_last_child = None;
        recursive_get_last_child(parent, &self.node_hierarchy.as_ref(), &mut total_last_child);
        if let Some(last) = total_last_child {
            (parent.index()..=last.index())
                .filter_map(|id| {
                    if self.node_hierarchy.as_ref()[id].last_child_id().is_some() {
                        Some(NodeId::new(id))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_rects_in_rendering_order(&self) -> ContentGroup {
        Self::determine_rendering_order(
            &self.non_leaf_nodes.as_ref(),
            &self.node_hierarchy.as_container(),
            &self.styled_nodes.as_container(),
            &self.node_data.as_container(),
            &self.get_css_property_cache(),
        )
    }

    /// Returns the rendering order of the items (the rendering
    /// order doesn't have to be the original order)
    fn determine_rendering_order<'a>(
        non_leaf_nodes: &[ParentWithNodeDepth],
        node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
        styled_nodes: &NodeDataContainerRef<StyledNode>,
        node_data_container: &NodeDataContainerRef<NodeData>,
        css_property_cache: &CssPropertyCache,
    ) -> ContentGroup {
        let children_sorted = non_leaf_nodes
            .iter()
            .filter_map(|parent| {
                Some((
                    parent.node_id,
                    sort_children_by_position(
                        parent.node_id.into_crate_internal()?,
                        node_hierarchy,
                        styled_nodes,
                        node_data_container,
                        css_property_cache,
                    ),
                ))
            })
            .collect::<Vec<_>>();

        let children_sorted: BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>> =
            children_sorted.into_iter().collect();

        let mut root_content_group = ContentGroup {
            root: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
            children: Vec::new().into(),
        };

        fill_content_group_children(&mut root_content_group, &children_sorted);

        root_content_group
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut new = Self::default();
        core::mem::swap(self, &mut new);
        new
    }

    pub fn set_menu_bar(&mut self, menu: Menu) {
        if let Some(root) = self.root.into_crate_internal() {
            self.node_data.as_mut()[root.index()].set_menu_bar(menu)
        }
    }

    pub fn set_context_menu(&mut self, menu: Menu) {
        if let Some(root) = self.root.into_crate_internal() {
            self.node_data.as_mut()[root.index()].set_context_menu(menu);

            // add a new hit-testing tag for root node
            let mut new_tags = self.tag_ids_to_node_ids.clone().into_library_owned_vec();

            let tag_id = match self.styled_nodes.as_mut()[root.index()].tag_id {
                OptionTagId::Some(s) => s,
                OptionTagId::None => AzTagId::from_crate_internal(TagId::unique()),
            };

            new_tags.push(TagIdToNodeIdMapping {
                tag_id,
                node_id: self.root,
                tab_index: OptionTabIndex::None,
                parent_node_ids: NodeIdVec::from_const_slice(&[]),
            });

            self.styled_nodes.as_mut()[root.index()].tag_id = OptionTagId::Some(tag_id);
            self.tag_ids_to_node_ids = new_tags.into();
        }
    }

    // Computes the diff between the two DOMs
    // pub fn diff(&self, other: &Self) -> StyledDomDiff { /**/ }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefaultCallbacksCfg {
    pub smooth_scroll: bool,
    pub enable_autotab: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DefaultScrollCallbackData {
    pub smooth_scroll: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DefaultTabIndexCallbackData {}

/// Default On::TabIndex event handler
extern "C" fn default_on_tabindex(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DefaultTabIndexCallbackData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    Update::DoNothing
}

/// Default On::Scroll event handler
extern "C" fn default_on_scroll(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DefaultScrollCallbackData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mouse_state = info.get_current_mouse_state();

    let (scroll_x, scroll_y) = match (
        mouse_state.scroll_y.into_option(),
        mouse_state.scroll_x.into_option(),
    ) {
        (None, None) => return Update::DoNothing,
        (x, y) => (x.unwrap_or(0.0), y.unwrap_or(0.0)),
    };

    let hit_node_id = info.get_hit_node();

    let new_scroll_position = match info.get_scroll_position(hit_node_id) {
        Some(mut s) => {
            s.x += scroll_x;
            s.y += scroll_y;
            s
        }
        None => return Update::DoNothing,
    };

    info.set_scroll_position(hit_node_id, new_scroll_position);

    Update::DoNothing
}

fn fill_content_group_children(
    group: &mut ContentGroup,
    children_sorted: &BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>>,
) {
    if let Some(c) = children_sorted.get(&group.root) {
        // returns None for leaf nodes
        group.children = c
            .iter()
            .map(|child| ContentGroup {
                root: *child,
                children: Vec::new().into(),
            })
            .collect::<Vec<ContentGroup>>()
            .into();

        for c in group.children.as_mut() {
            fill_content_group_children(c, children_sorted);
        }
    }
}

fn sort_children_by_position<'a>(
    parent: NodeId,
    node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
    rectangles: &NodeDataContainerRef<StyledNode>,
    node_data_container: &NodeDataContainerRef<NodeData>,
    css_property_cache: &CssPropertyCache,
) -> Vec<NodeHierarchyItemId> {
    use azul_css::props::layout::LayoutPosition::*;

    let children_positions = parent
        .az_children(node_hierarchy)
        .map(|nid| {
            let position = css_property_cache
                .get_position(&node_data_container[nid], &nid, &rectangles[nid].state)
                .and_then(|p| p.clone().get_property_or_default())
                .unwrap_or_default();
            let id = NodeHierarchyItemId::from_crate_internal(Some(nid));
            (id, position)
        })
        .collect::<Vec<_>>();

    let mut not_absolute_children = children_positions
        .iter()
        .filter_map(|(node_id, position)| {
            if *position != Absolute {
                Some(*node_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let mut absolute_children = children_positions
        .iter()
        .filter_map(|(node_id, position)| {
            if *position == Absolute {
                Some(*node_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // Append the position:absolute children after the regular children
    not_absolute_children.append(&mut absolute_children);
    not_absolute_children
}

// calls get_last_child() recursively until the last child of the last child of the ... has been
// found
fn recursive_get_last_child(
    node_id: NodeId,
    node_hierarchy: &[NodeHierarchyItem],
    target: &mut Option<NodeId>,
) {
    match node_hierarchy[node_id.index()].last_child_id() {
        None => return,
        Some(s) => {
            *target = Some(s);
            recursive_get_last_child(s, node_hierarchy, target);
        }
    }
}
