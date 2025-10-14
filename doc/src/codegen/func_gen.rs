//! Function and DLL binding generation
//!
//! This module generates Rust DLL bindings from api.json data.
//! It's a port of the `generate_rust_dll_bindings` function from oldbuild.py.
//!
//! Generates three submodules:
//! 1. `types` - All struct/enum definitions (using struct_gen)
//! 2. `static_link` - Wrapper functions with mem::transmute for static linking
//! 3. `dynamic_link` - extern "C" declarations for dynamic linking

use std::collections::HashMap;

use anyhow::Result;
use indexmap::IndexMap;

use super::struct_gen::{generate_structs, GenerateConfig, StructMetadata};
use crate::{
    api::{ApiData, ClassData, VersionData},
    utils::{
        analyze::{analyze_type, is_primitive_arg, search_for_class_by_class_name},
        string::snake_case_to_lower_camel,
    },
};

/// Function signature: (fn_args, return_type)
/// e.g., ("x: f32, y: f32", "Point")
pub type FunctionSignature = (String, String);

/// Map of function name to signature
/// e.g., "AzPoint_new" -> ("x: f32, y: f32", "AzPoint")
pub type FunctionsMap = HashMap<String, FunctionSignature>;

/// Generate the complete DLL bindings module
/// Returns the generated Rust code as a string
pub fn generate_rust_dll_bindings(
    version_data: &VersionData,
    structs_map: &HashMap<String, StructMetadata>,
    functions_map: &FunctionsMap,
    config: &GenerateConfig,
) -> Result<String> {
    let mut code = String::new();

    // Add header from patch file
    // code += read_file(root_folder + "/api/_patches/azul.rs/dll.rs")
    code.push_str("// DLL bindings module\n");
    code.push_str("// This module provides both static and dynamic linking options\n\n");

    // Module visibility directives
    code.push_str("    #[cfg(not(feature = \"link-static\"))]\n");
    code.push_str("    pub use self::dynamic_link::*;\n");
    code.push_str("    #[cfg(feature = \"link-static\")]\n");
    code.push_str("    pub use self::static_link::*;\n");
    code.push_str("    pub use self::types::*;\n");
    code.push_str("\n");

    // Generate types module with all structs
    code.push_str("    mod types {\n");
    code.push_str("        use core::ffi::c_void;\n\n");

    let types_config = GenerateConfig {
        prefix: config.prefix.clone(),
        indent: 8,
        autoderive: true,
        private_pointers: config.private_pointers,
        no_derive: false,
        wrapper_postfix: config.wrapper_postfix.clone(),
    };

    let types_code = generate_structs(version_data, structs_map, &types_config)?;
    code.push_str(&types_code);
    code.push_str("    }\n\n");

    // Generate static_link module
    code.push_str("    #[cfg(feature = \"link-static\")]\n");
    code.push_str("    #[allow(non_snake_case)]\n");
    code.push_str("    mod static_link {\n");
    code.push_str("        use core::ffi::c_void;\n");
    code.push_str("        use core::mem::transmute;\n");
    code.push_str("        use super::types::*;\n\n");

    for (fn_name, (fn_args, fn_return)) in functions_map {
        let return_arrow = if fn_return.is_empty() { "" } else { " -> " };
        let fn_args_transmuted = strip_fn_arg_types_mem_transmute(fn_args);

        code.push_str(&format!(
            "        pub(crate) fn {}({}){}{}  {{ unsafe {{ transmute(azul_dll::{}({})) }} }}\n",
            fn_name, fn_args, return_arrow, fn_return, fn_name, fn_args_transmuted
        ));
    }

    code.push_str("    }\n\n");

    // Generate dynamic_link module
    code.push_str("    #[cfg(not(feature = \"link-static\"))]\n");
    code.push_str("    mod dynamic_link {\n");
    code.push_str("        use core::ffi::c_void;\n\n");
    code.push_str("        use super::types::*;\n\n");
    code.push_str("        #[cfg_attr(target_os = \"windows\", link(name=\"azul.dll\"))] // https://github.com/rust-lang/cargo/issues/9082\n");
    code.push_str("        #[cfg_attr(not(target_os = \"windows\"), link(name=\"azul\"))] // https://github.com/rust-lang/cargo/issues/9082\n");
    code.push_str("        extern \"C\" {\n");

    for (fn_name, (fn_args, fn_return)) in functions_map {
        let return_arrow = if fn_return.is_empty() { "" } else { " -> " };
        let fn_args_stripped = strip_fn_arg_types(fn_args);

        code.push_str(&format!(
            "            pub(crate) fn {}({}){}{};\n",
            fn_name, fn_args_stripped, return_arrow, fn_return
        ));
    }

    code.push_str("        }\n\n");
    code.push_str("    }\n\n");

    code.push_str("\n\n");

    Ok(code)
}

/// Strip argument names and keep only types with mem::transmute
/// "x: f32, y: f32" -> "transmute(x), transmute(y)"
fn strip_fn_arg_types_mem_transmute(arg_list: &str) -> String {
    if arg_list.is_empty() {
        return String::new();
    }

    arg_list
        .split(',')
        .map(|arg| {
            let parts: Vec<&str> = arg.trim().split(':').collect();
            if parts.len() >= 2 {
                let arg_name = parts[0].trim();
                format!("transmute({})", arg_name)
            } else {
                "transmute(?)".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Strip argument names and keep only types
/// "x: f32, y: f32" -> "f32, f32"
fn strip_fn_arg_types(arg_list: &str) -> String {
    if arg_list.is_empty() {
        return String::new();
    }

    arg_list
        .split(',')
        .map(|arg| {
            let parts: Vec<&str> = arg.trim().split(':').collect();
            if parts.len() >= 2 {
                parts[1..].join(":").trim().to_string()
            } else {
                arg.trim().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a FunctionsMap by processing all classes in the API
/// This collects:
/// - Constructor functions (ClassName_constructorName)
/// - Member functions (ClassName_functionName)
/// - delete functions (ClassName_delete)
/// - deepCopy functions (ClassName_deepCopy)
pub fn build_functions_map(version_data: &VersionData, prefix: &str) -> Result<FunctionsMap> {
    let mut functions_map = HashMap::new();

    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            let class_ptr_name = format!("{}{}", prefix, class_name);

            // Process constructors
            if let Some(constructors) = &class_data.constructors {
                for (fn_name, constructor) in constructors {
                    let c_fn_name =
                        format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));

                    let fn_args = build_fn_args_c_api(
                        Some(&constructor.fn_args),
                        class_name,
                        &class_ptr_name,
                        false, // is_member_function
                        version_data,
                        prefix,
                    )?;

                    let returns =
                        build_return_type(constructor.returns.as_ref(), version_data, prefix)?;

                    functions_map.insert(c_fn_name, (fn_args, returns));
                }
            }

            // Process member functions
            if let Some(functions) = &class_data.functions {
                for (fn_name, function) in functions {
                    let c_fn_name =
                        format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));

                    let fn_args = build_fn_args_c_api(
                        Some(&function.fn_args),
                        class_name,
                        &class_ptr_name,
                        true, // is_member_function
                        version_data,
                        prefix,
                    )?;

                    let returns =
                        build_return_type(function.returns.as_ref(), version_data, prefix)?;

                    functions_map.insert(c_fn_name, (fn_args, returns));
                }
            }

            // Add delete function if needed
            let class_can_be_copied = class_data
                .derive
                .as_ref()
                .map_or(false, |d| d.contains(&"Copy".to_string()));
            let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
            let treat_external_as_ptr = class_data.external.is_some() && class_data.is_boxed_object;

            if !class_can_be_copied && (class_has_custom_destructor || treat_external_as_ptr) {
                let delete_fn_name = format!("{}_delete", class_ptr_name);
                let delete_args = format!("object: &mut {}", class_ptr_name);
                functions_map.insert(delete_fn_name, (delete_args, String::new()));
            }

            // Add deepCopy function if needed
            let class_can_be_cloned = class_data.clone.unwrap_or(true);
            if treat_external_as_ptr && class_can_be_cloned {
                let copy_fn_name = format!("{}_deepCopy", class_ptr_name);
                let copy_args = format!("object: &{}", class_ptr_name);
                functions_map.insert(copy_fn_name, (copy_args, class_ptr_name.clone()));
            }
        }
    }

    Ok(functions_map)
}

/// Build function arguments for C API
fn build_fn_args_c_api(
    fn_args: Option<&Vec<IndexMap<String, String>>>,
    class_name: &str,
    class_ptr_name: &str,
    is_member_function: bool,
    version_data: &VersionData,
    prefix: &str,
) -> Result<String> {
    let mut args = Vec::new();

    // Add self parameter for member functions
    if is_member_function {
        args.push(format!("self_: &{}", class_ptr_name));
    }

    // Add other arguments
    if let Some(fn_args_list) = fn_args {
        for arg_map in fn_args_list {
            for (arg_name, arg_type) in arg_map {
                let (prefix_str, base_type, suffix) = analyze_type(arg_type);

                let resolved_type = if is_primitive_arg(&base_type) {
                    arg_type.clone()
                } else if let Some((_, found_class)) =
                    search_for_class_by_class_name(version_data, &base_type)
                {
                    format!("{}{}{}{}", prefix_str, prefix, found_class, suffix)
                } else {
                    arg_type.clone()
                };

                args.push(format!("{}: {}", arg_name, resolved_type));
            }
        }
    }

    Ok(args.join(", "))
}

/// Build return type
fn build_return_type(
    return_info: Option<&crate::api::ReturnTypeData>,
    version_data: &VersionData,
    prefix: &str,
) -> Result<String> {
    if let Some(ret) = return_info {
        let return_type = &ret.r#type;
        let (prefix_str, base_type, suffix) = analyze_type(return_type);

        if is_primitive_arg(&base_type) {
            Ok(return_type.clone())
        } else if let Some((_, found_class)) =
            search_for_class_by_class_name(version_data, &base_type)
        {
            Ok(format!("{}{}{}{}", prefix_str, prefix, found_class, suffix))
        } else {
            Ok(return_type.clone())
        }
    } else {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_fn_arg_types() {
        assert_eq!(strip_fn_arg_types("x: f32, y: f32"), "f32, f32");
        assert_eq!(strip_fn_arg_types("self_: &AzPoint"), "&AzPoint");
        assert_eq!(strip_fn_arg_types(""), "");
    }

    #[test]
    fn test_strip_fn_arg_types_mem_transmute() {
        assert_eq!(
            strip_fn_arg_types_mem_transmute("x: f32, y: f32"),
            "transmute(x), transmute(y)"
        );
        assert_eq!(
            strip_fn_arg_types_mem_transmute("self_: &AzPoint"),
            "transmute(self_)"
        );
        assert_eq!(strip_fn_arg_types_mem_transmute(""), "");
    }
}
