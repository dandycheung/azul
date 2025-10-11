//! CSS properties for CSS Grid layout.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    format_rust_code::FormatAsRustCode,
    props::{basic::pixel::PixelValue, formatter::PrintAsCssValue},
};

// --- grid-template-columns / grid-template-rows ---

/// Represents a single track sizing function for grid
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum GridTrackSizing {
    /// Fixed pixel/percent size
    Fixed(PixelValue),
    /// fr units (stored as integer to satisfy Eq/Ord/Hash)
    Fr(i32),
    /// min-content
    MinContent,
    /// max-content
    MaxContent,
    /// auto
    Auto,
    /// minmax(min, max)
    MinMax(Box<GridTrackSizing>, Box<GridTrackSizing>),
    /// fit-content(size)
    FitContent(PixelValue),
}

impl core::fmt::Debug for GridTrackSizing {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridTrackSizing {
    fn default() -> Self {
        GridTrackSizing::Auto
    }
}

impl PrintAsCssValue for GridTrackSizing {
    fn print_as_css_value(&self) -> String {
        match self {
            GridTrackSizing::Fixed(px) => px.print_as_css_value(),
            GridTrackSizing::Fr(f) => format!("{}fr", f),
            GridTrackSizing::MinContent => "min-content".to_string(),
            GridTrackSizing::MaxContent => "max-content".to_string(),
            GridTrackSizing::Auto => "auto".to_string(),
            GridTrackSizing::MinMax(min, max) => {
                format!(
                    "minmax({}, {})",
                    min.print_as_css_value(),
                    max.print_as_css_value()
                )
            }
            GridTrackSizing::FitContent(size) => {
                format!("fit-content({})", size.print_as_css_value())
            }
        }
    }
}

/// Represents `grid-template-columns` or `grid-template-rows`
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridTemplate {
    pub tracks: Vec<GridTrackSizing>,
}

impl core::fmt::Debug for GridTemplate {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridTemplate {
    fn default() -> Self {
        GridTemplate { tracks: Vec::new() }
    }
}

impl PrintAsCssValue for GridTemplate {
    fn print_as_css_value(&self) -> String {
        if self.tracks.is_empty() {
            "none".to_string()
        } else {
            self.tracks
                .iter()
                .map(|t| t.print_as_css_value())
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

// --- grid-auto-columns / grid-auto-rows ---

pub type GridAutoTracks = GridTemplate;

// --- grid-row / grid-column (grid line placement) ---

/// Represents a grid line position (start or end)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum GridLine {
    /// auto
    Auto,
    /// Line number (1-based, negative for counting from end)
    Line(i32),
    /// Named line with optional span count
    Named(String, Option<i32>),
    /// span N
    Span(i32),
}

impl core::fmt::Debug for GridLine {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridLine {
    fn default() -> Self {
        GridLine::Auto
    }
}

impl PrintAsCssValue for GridLine {
    fn print_as_css_value(&self) -> String {
        match self {
            GridLine::Auto => "auto".to_string(),
            GridLine::Line(n) => n.to_string(),
            GridLine::Named(name, None) => name.clone(),
            GridLine::Named(name, Some(n)) => format!("{} {}", name, n),
            GridLine::Span(n) => format!("span {}", n),
        }
    }
}

/// Represents `grid-row` or `grid-column` (start / end)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridPlacement {
    pub start: GridLine,
    pub end: GridLine,
}

impl core::fmt::Debug for GridPlacement {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridPlacement {
    fn default() -> Self {
        GridPlacement {
            start: GridLine::Auto,
            end: GridLine::Auto,
        }
    }
}

impl PrintAsCssValue for GridPlacement {
    fn print_as_css_value(&self) -> String {
        if self.end == GridLine::Auto {
            self.start.print_as_css_value()
        } else {
            format!(
                "{} / {}",
                self.start.print_as_css_value(),
                self.end.print_as_css_value()
            )
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum GridParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(GridParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { GridParseError<'a>, {
    InvalidValue(e) => format!("Invalid grid value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum GridParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> GridParseError<'a> {
    pub fn to_contained(&self) -> GridParseErrorOwned {
        match self {
            GridParseError::InvalidValue(s) => GridParseErrorOwned::InvalidValue(s.to_string()),
        }
    }
}

#[cfg(feature = "parser")]
impl GridParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> GridParseError<'a> {
        match self {
            GridParseErrorOwned::InvalidValue(s) => GridParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_grid_template<'a>(input: &'a str) -> Result<GridTemplate, GridParseError<'a>> {
    use crate::props::basic::pixel::parse_pixel_value;

    let input = input.trim();

    if input == "none" {
        return Ok(GridTemplate::default());
    }

    let mut tracks = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for ch in input.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            ' ' if paren_depth == 0 => {
                if !current.trim().is_empty() {
                    let track_str = current.trim().to_string();
                    tracks.push(
                        parse_grid_track_owned(&track_str)
                            .map_err(|_| GridParseError::InvalidValue(input))?,
                    );
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        let track_str = current.trim().to_string();
        tracks.push(
            parse_grid_track_owned(&track_str).map_err(|_| GridParseError::InvalidValue(input))?,
        );
    }

    Ok(GridTemplate { tracks })
}

#[cfg(feature = "parser")]
fn parse_grid_track_owned(input: &str) -> Result<GridTrackSizing, ()> {
    use crate::props::basic::pixel::parse_pixel_value;

    let input = input.trim();

    if input == "auto" {
        return Ok(GridTrackSizing::Auto);
    }

    if input == "min-content" {
        return Ok(GridTrackSizing::MinContent);
    }

    if input == "max-content" {
        return Ok(GridTrackSizing::MaxContent);
    }

    if input.ends_with("fr") {
        let num_str = &input[..input.len() - 2].trim();
        if let Ok(num) = num_str.parse::<f32>() {
            return Ok(GridTrackSizing::Fr((num * 100.0) as i32));
        }
        return Err(());
    }

    if input.starts_with("minmax(") && input.ends_with(')') {
        let content = &input[7..input.len() - 1];
        let parts: Vec<&str> = content.split(',').collect();
        if parts.len() == 2 {
            let min = parse_grid_track_owned(parts[0].trim())?;
            let max = parse_grid_track_owned(parts[1].trim())?;
            return Ok(GridTrackSizing::MinMax(Box::new(min), Box::new(max)));
        }
        return Err(());
    }

    if input.starts_with("fit-content(") && input.ends_with(')') {
        let size_str = &input[12..input.len() - 1].trim();
        if let Ok(size) = parse_pixel_value(size_str) {
            return Ok(GridTrackSizing::FitContent(size));
        }
        return Err(());
    }

    // Try to parse as pixel value
    if let Ok(px) = parse_pixel_value(input) {
        return Ok(GridTrackSizing::Fixed(px));
    }

    Err(())
}

#[cfg(feature = "parser")]
pub fn parse_grid_placement<'a>(input: &'a str) -> Result<GridPlacement, GridParseError<'a>> {
    let input = input.trim();

    if input == "auto" {
        return Ok(GridPlacement::default());
    }

    // Split by "/"
    let parts: Vec<&str> = input.split('/').map(|s| s.trim()).collect();

    let start = parse_grid_line_owned(parts[0]).map_err(|_| GridParseError::InvalidValue(input))?;
    let end = if parts.len() > 1 {
        parse_grid_line_owned(parts[1]).map_err(|_| GridParseError::InvalidValue(input))?
    } else {
        GridLine::Auto
    };

    Ok(GridPlacement { start, end })
}

// --- grid-auto-flow ---

/// Represents the `grid-auto-flow` property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutGridAutoFlow {
    Row,
    Column,
    RowDense,
    ColumnDense,
}

impl Default for LayoutGridAutoFlow {
    fn default() -> Self {
        LayoutGridAutoFlow::Row
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutGridAutoFlow {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            LayoutGridAutoFlow::Row => "row".to_string(),
            LayoutGridAutoFlow::Column => "column".to_string(),
            LayoutGridAutoFlow::RowDense => "row dense".to_string(),
            LayoutGridAutoFlow::ColumnDense => "column dense".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum GridAutoFlowParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(GridAutoFlowParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { GridAutoFlowParseError<'a>, {
    InvalidValue(e) => format!("Invalid grid-auto-flow value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum GridAutoFlowParseErrorOwned {
    InvalidValue(alloc::string::String),
}

#[cfg(feature = "parser")]
impl<'a> GridAutoFlowParseError<'a> {
    pub fn to_contained(&self) -> GridAutoFlowParseErrorOwned {
        match self {
            GridAutoFlowParseError::InvalidValue(s) => {
                GridAutoFlowParseErrorOwned::InvalidValue(s.to_string())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl GridAutoFlowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> GridAutoFlowParseError<'a> {
        match self {
            GridAutoFlowParseErrorOwned::InvalidValue(s) => {
                GridAutoFlowParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_grid_auto_flow<'a>(
    input: &'a str,
) -> Result<LayoutGridAutoFlow, GridAutoFlowParseError<'a>> {
    match input.trim() {
        "row" => Ok(LayoutGridAutoFlow::Row),
        "column" => Ok(LayoutGridAutoFlow::Column),
        "row dense" => Ok(LayoutGridAutoFlow::RowDense),
        "column dense" => Ok(LayoutGridAutoFlow::ColumnDense),
        "dense" => Ok(LayoutGridAutoFlow::RowDense),
        _ => Err(GridAutoFlowParseError::InvalidValue(input)),
    }
}

// --- justify-self / justify-items ---

/// Represents `justify-self` for grid items
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutJustifySelf {
    Auto,
    Start,
    End,
    Center,
    Stretch,
}

impl Default for LayoutJustifySelf {
    fn default() -> Self {
        Self::Auto
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutJustifySelf {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            LayoutJustifySelf::Auto => "auto".to_string(),
            LayoutJustifySelf::Start => "start".to_string(),
            LayoutJustifySelf::End => "end".to_string(),
            LayoutJustifySelf::Center => "center".to_string(),
            LayoutJustifySelf::Stretch => "stretch".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum JustifySelfParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum JustifySelfParseErrorOwned {
    InvalidValue(alloc::string::String),
}

#[cfg(feature = "parser")]
impl<'a> JustifySelfParseError<'a> {
    pub fn to_contained(&self) -> JustifySelfParseErrorOwned {
        match self {
            JustifySelfParseError::InvalidValue(s) => {
                JustifySelfParseErrorOwned::InvalidValue(s.to_string())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl JustifySelfParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> JustifySelfParseError<'a> {
        match self {
            JustifySelfParseErrorOwned::InvalidValue(s) => {
                JustifySelfParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl_debug_as_display!(JustifySelfParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { JustifySelfParseError<'a>, {
    InvalidValue(e) => format!("Invalid justify-self value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
pub fn parse_layout_justify_self<'a>(
    input: &'a str,
) -> Result<LayoutJustifySelf, JustifySelfParseError<'a>> {
    match input.trim() {
        "auto" => Ok(LayoutJustifySelf::Auto),
        "start" | "flex-start" => Ok(LayoutJustifySelf::Start),
        "end" | "flex-end" => Ok(LayoutJustifySelf::End),
        "center" => Ok(LayoutJustifySelf::Center),
        "stretch" => Ok(LayoutJustifySelf::Stretch),
        _ => Err(JustifySelfParseError::InvalidValue(input)),
    }
}

/// Represents `justify-items` for grid containers
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutJustifyItems {
    Start,
    End,
    Center,
    Stretch,
}

impl Default for LayoutJustifyItems {
    fn default() -> Self {
        Self::Stretch
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutJustifyItems {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            LayoutJustifyItems::Start => "start".to_string(),
            LayoutJustifyItems::End => "end".to_string(),
            LayoutJustifyItems::Center => "center".to_string(),
            LayoutJustifyItems::Stretch => "stretch".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum JustifyItemsParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum JustifyItemsParseErrorOwned {
    InvalidValue(alloc::string::String),
}

#[cfg(feature = "parser")]
impl<'a> JustifyItemsParseError<'a> {
    pub fn to_contained(&self) -> JustifyItemsParseErrorOwned {
        match self {
            JustifyItemsParseError::InvalidValue(s) => {
                JustifyItemsParseErrorOwned::InvalidValue(s.to_string())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl JustifyItemsParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> JustifyItemsParseError<'a> {
        match self {
            JustifyItemsParseErrorOwned::InvalidValue(s) => {
                JustifyItemsParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl_debug_as_display!(JustifyItemsParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { JustifyItemsParseError<'a>, {
    InvalidValue(e) => format!("Invalid justify-items value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
pub fn parse_layout_justify_items<'a>(
    input: &'a str,
) -> Result<LayoutJustifyItems, JustifyItemsParseError<'a>> {
    match input.trim() {
        "start" => Ok(LayoutJustifyItems::Start),
        "end" => Ok(LayoutJustifyItems::End),
        "center" => Ok(LayoutJustifyItems::Center),
        "stretch" => Ok(LayoutJustifyItems::Stretch),
        _ => Err(JustifyItemsParseError::InvalidValue(input)),
    }
}

// --- gap (single value type) ---

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutGap {
    pub inner: crate::props::basic::pixel::PixelValue,
}

impl core::fmt::Debug for LayoutGap {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutGap {
    fn print_as_css_value(&self) -> alloc::string::String {
        self.inner.print_as_css_value()
    }
}

// Implement FormatAsRustCode for the new types so they can be emitted by the
// code generator.
impl FormatAsRustCode for LayoutGridAutoFlow {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutGridAutoFlow::{}",
            match self {
                LayoutGridAutoFlow::Row => "Row",
                LayoutGridAutoFlow::Column => "Column",
                LayoutGridAutoFlow::RowDense => "RowDense",
                LayoutGridAutoFlow::ColumnDense => "ColumnDense",
            }
        )
    }
}

impl FormatAsRustCode for LayoutJustifySelf {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutJustifySelf::{}",
            match self {
                LayoutJustifySelf::Auto => "Auto",
                LayoutJustifySelf::Start => "Start",
                LayoutJustifySelf::End => "End",
                LayoutJustifySelf::Center => "Center",
                LayoutJustifySelf::Stretch => "Stretch",
            }
        )
    }
}

impl FormatAsRustCode for LayoutJustifyItems {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutJustifyItems::{}",
            match self {
                LayoutJustifyItems::Start => "Start",
                LayoutJustifyItems::End => "End",
                LayoutJustifyItems::Center => "Center",
                LayoutJustifyItems::Stretch => "Stretch",
            }
        )
    }
}

impl FormatAsRustCode for LayoutGap {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        // LayoutGap wraps a PixelValue which implements FormatAsRustCode via helpers;
        // print as LayoutGap::Exact(LAYERVALUE) is not required here — use the CSS string
        format!("LayoutGap::Exact({})", self.inner)
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_gap<'a>(
    input: &'a str,
) -> Result<LayoutGap, crate::props::basic::pixel::CssPixelValueParseError<'a>> {
    crate::props::basic::pixel::parse_pixel_value(input).map(|p| LayoutGap { inner: p })
}

#[cfg(feature = "parser")]
fn parse_grid_line_owned(input: &str) -> Result<GridLine, ()> {
    let input = input.trim();

    if input == "auto" {
        return Ok(GridLine::Auto);
    }

    if input.starts_with("span ") {
        let num_str = &input[5..].trim();
        if let Ok(num) = num_str.parse::<i32>() {
            return Ok(GridLine::Span(num));
        }
        return Err(());
    }

    // Try to parse as line number
    if let Ok(num) = input.parse::<i32>() {
        return Ok(GridLine::Line(num));
    }

    // Otherwise treat as named line
    Ok(GridLine::Named(input.to_string(), None))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    // Grid template tests
    #[test]
    fn test_parse_grid_template_none() {
        let result = parse_grid_template("none").unwrap();
        assert_eq!(result.tracks.len(), 0);
    }

    #[test]
    fn test_parse_grid_template_single_px() {
        let result = parse_grid_template("100px").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert!(matches!(result.tracks[0], GridTrackSizing::Fixed(_)));
    }

    #[test]
    fn test_parse_grid_template_multiple_tracks() {
        let result = parse_grid_template("100px 200px 1fr").unwrap();
        assert_eq!(result.tracks.len(), 3);
    }

    #[test]
    fn test_parse_grid_template_fr_units() {
        let result = parse_grid_template("1fr 2fr 1fr").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(result.tracks[0], GridTrackSizing::Fr(100)));
        assert!(matches!(result.tracks[1], GridTrackSizing::Fr(200)));
    }

    #[test]
    fn test_parse_grid_template_fractional_fr() {
        let result = parse_grid_template("0.5fr 1.5fr").unwrap();
        assert_eq!(result.tracks.len(), 2);
        assert!(matches!(result.tracks[0], GridTrackSizing::Fr(50)));
        assert!(matches!(result.tracks[1], GridTrackSizing::Fr(150)));
    }

    #[test]
    fn test_parse_grid_template_auto() {
        let result = parse_grid_template("auto 100px auto").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(result.tracks[0], GridTrackSizing::Auto));
        assert!(matches!(result.tracks[2], GridTrackSizing::Auto));
    }

    #[test]
    fn test_parse_grid_template_min_max_content() {
        let result = parse_grid_template("min-content max-content auto").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(result.tracks[0], GridTrackSizing::MinContent));
        assert!(matches!(result.tracks[1], GridTrackSizing::MaxContent));
    }

    #[test]
    fn test_parse_grid_template_minmax() {
        let result = parse_grid_template("minmax(100px, 1fr)").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert!(matches!(result.tracks[0], GridTrackSizing::MinMax(_, _)));
    }

    #[test]
    fn test_parse_grid_template_minmax_complex() {
        let result = parse_grid_template("minmax(min-content, max-content)").unwrap();
        assert_eq!(result.tracks.len(), 1);
    }

    #[test]
    fn test_parse_grid_template_fit_content() {
        let result = parse_grid_template("fit-content(200px)").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert!(matches!(result.tracks[0], GridTrackSizing::FitContent(_)));
    }

    #[test]
    fn test_parse_grid_template_mixed() {
        let result = parse_grid_template("100px minmax(100px, 1fr) auto 2fr").unwrap();
        assert_eq!(result.tracks.len(), 4);
    }

    #[test]
    fn test_parse_grid_template_percent() {
        let result = parse_grid_template("25% 50% 25%").unwrap();
        assert_eq!(result.tracks.len(), 3);
    }

    #[test]
    fn test_parse_grid_template_em_units() {
        let result = parse_grid_template("10em 20em 1fr").unwrap();
        assert_eq!(result.tracks.len(), 3);
    }

    // Grid placement tests
    #[test]
    fn test_parse_grid_placement_auto() {
        let result = parse_grid_placement("auto").unwrap();
        assert!(matches!(result.start, GridLine::Auto));
        assert!(matches!(result.end, GridLine::Auto));
    }

    #[test]
    fn test_parse_grid_placement_line_number() {
        let result = parse_grid_placement("1").unwrap();
        assert!(matches!(result.start, GridLine::Line(1)));
        assert!(matches!(result.end, GridLine::Auto));
    }

    #[test]
    fn test_parse_grid_placement_negative_line() {
        let result = parse_grid_placement("-1").unwrap();
        assert!(matches!(result.start, GridLine::Line(-1)));
    }

    #[test]
    fn test_parse_grid_placement_span() {
        let result = parse_grid_placement("span 2").unwrap();
        assert!(matches!(result.start, GridLine::Span(2)));
    }

    #[test]
    fn test_parse_grid_placement_start_end() {
        let result = parse_grid_placement("1 / 3").unwrap();
        assert!(matches!(result.start, GridLine::Line(1)));
        assert!(matches!(result.end, GridLine::Line(3)));
    }

    #[test]
    fn test_parse_grid_placement_span_end() {
        let result = parse_grid_placement("1 / span 2").unwrap();
        assert!(matches!(result.start, GridLine::Line(1)));
        assert!(matches!(result.end, GridLine::Span(2)));
    }

    #[test]
    fn test_parse_grid_placement_named_line() {
        let result = parse_grid_placement("header-start").unwrap();
        assert!(matches!(result.start, GridLine::Named(_, _)));
    }

    #[test]
    fn test_parse_grid_placement_named_start_end() {
        let result = parse_grid_placement("header-start / header-end").unwrap();
        assert!(matches!(result.start, GridLine::Named(_, _)));
        assert!(matches!(result.end, GridLine::Named(_, _)));
    }

    // Edge cases
    #[test]
    fn test_parse_grid_template_whitespace() {
        let result = parse_grid_template("  100px   200px  ").unwrap();
        assert_eq!(result.tracks.len(), 2);
    }

    #[test]
    fn test_parse_grid_placement_whitespace() {
        let result = parse_grid_placement("  1  /  3  ").unwrap();
        assert!(matches!(result.start, GridLine::Line(1)));
        assert!(matches!(result.end, GridLine::Line(3)));
    }

    #[test]
    fn test_parse_grid_template_zero_fr() {
        let result = parse_grid_template("0fr").unwrap();
        assert!(matches!(result.tracks[0], GridTrackSizing::Fr(0)));
    }

    #[test]
    fn test_parse_grid_placement_zero_line() {
        let result = parse_grid_placement("0").unwrap();
        assert!(matches!(result.start, GridLine::Line(0)));
    }
}
