//! CSS properties for flowing content into regions (`flow-into`, `flow-from`).

use alloc::string::{String, ToString};

use crate::{corety::AzString, props::formatter::PrintAsCssValue};

// --- flow-into ---

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum FlowInto {
    None,
    Named(AzString),
}

impl Default for FlowInto {
    fn default() -> Self {
        Self::None
    }
}

impl PrintAsCssValue for FlowInto {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Named(s) => s.to_string(),
        }
    }
}

// --- flow-from ---

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum FlowFrom {
    None,
    Named(AzString),
}

impl Default for FlowFrom {
    fn default() -> Self {
        Self::None
    }
}

impl PrintAsCssValue for FlowFrom {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Named(s) => s.to_string(),
        }
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
mod parser {
    use super::*;

    macro_rules! define_flow_parser {
        (
            $fn_name:ident,
            $struct_name:ident,
            $error_name:ident,
            $error_owned_name:ident,
            $prop_name:expr
        ) => {
            #[derive(Clone, PartialEq)]
            pub enum $error_name<'a> {
                InvalidValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                InvalidValue(v) => format!("Invalid {} value: \"{}\"", $prop_name, v),
            }}

            #[derive(Debug, Clone, PartialEq)]
            pub enum $error_owned_name {
                InvalidValue(String),
            }

            impl<'a> $error_name<'a> {
                pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::InvalidValue(s) => $error_owned_name::InvalidValue(s.to_string()),
                    }
                }
            }

            impl $error_owned_name {
                pub fn to_shared<'a>(&'a self) -> $error_name<'a> {
                    match self {
                        Self::InvalidValue(s) => $error_name::InvalidValue(s.as_str()),
                    }
                }
            }

            pub fn $fn_name<'a>(input: &'a str) -> Result<$struct_name, $error_name<'a>> {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    return Err($error_name::InvalidValue(input));
                }
                match trimmed {
                    "none" => Ok($struct_name::None),
                    // any other value is a custom identifier
                    ident => Ok($struct_name::Named(ident.to_string().into())),
                }
            }
        };
    }

    define_flow_parser!(
        parse_flow_into,
        FlowInto,
        FlowIntoParseError,
        FlowIntoParseErrorOwned,
        "flow-into"
    );
    define_flow_parser!(
        parse_flow_from,
        FlowFrom,
        FlowFromParseError,
        FlowFromParseErrorOwned,
        "flow-from"
    );
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flow_into() {
        assert_eq!(parse_flow_into("none").unwrap(), FlowInto::None);
        assert_eq!(
            parse_flow_into("my-article-flow").unwrap(),
            FlowInto::Named("my-article-flow".into())
        );
        assert!(parse_flow_into("").is_err());
    }

    #[test]
    fn test_parse_flow_from() {
        assert_eq!(parse_flow_from("none").unwrap(), FlowFrom::None);
        assert_eq!(
            parse_flow_from("  main-thread  ").unwrap(),
            FlowFrom::Named("main-thread".into())
        );
        assert!(parse_flow_from("").is_err());
    }
}
