//! Macro for generating typed scroll manager wrappers
//!
//! This crate provides the `generate_scroll_manager!` macro which generates
//! platform-specific scroll manager types for use with UniFFI and WASM.
//!
//! # Usage
//!
//! Apply in your **bindings crate** (not model crate) to keep models platform-agnostic:
//!
//! ```ignore
//! use ankurah_virtual_scroll_derive::generate_scroll_manager;
//!
//! // Re-export model types
//! pub use my_model_crate::*;
//!
//! // Generate scroll manager for Message model
//! generate_scroll_manager!(
//!     Message,           // Model type
//!     MessageView,       // View type
//!     MessageLiveQuery,  // LiveQuery type
//!     timestamp_field = "timestamp"
//! );
//! ```
//!
//! This generates `MessageScrollManager` for the appropriate platform
//! (UniFFI when `uniffi` feature enabled, WASM when `wasm` feature enabled).

mod uniffi;
mod wasm;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::{Parse, ParseStream}, parse_macro_input, Ident, LitStr, Path, Token};

/// Configuration parsed from generate_scroll_manager! macro arguments
struct ScrollManagerConfig {
    model_path: Path,
    view_path: Path,
    livequery_path: Path,
    timestamp_field: String,
}

impl ScrollManagerConfig {
    /// Get the simple name from a path (last segment)
    fn model_name(&self) -> &Ident {
        &self.model_path.segments.last().unwrap().ident
    }
}

impl Parse for ScrollManagerConfig {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse: Model, View, LiveQuery, timestamp_field = "field"
        // Paths can be simple (Message) or qualified (my_crate::Message)
        let model_path: Path = input.parse()?;
        input.parse::<Token![,]>()?;

        let view_path: Path = input.parse()?;
        input.parse::<Token![,]>()?;

        let livequery_path: Path = input.parse()?;
        input.parse::<Token![,]>()?;

        // Parse timestamp_field = "value"
        let key: Ident = input.parse()?;
        if key != "timestamp_field" {
            return Err(syn::Error::new(key.span(), "expected `timestamp_field`"));
        }
        input.parse::<Token![=]>()?;
        let timestamp_field: LitStr = input.parse()?;

        Ok(Self {
            model_path,
            view_path,
            livequery_path,
            timestamp_field: timestamp_field.value(),
        })
    }
}

/// Generate a typed scroll manager wrapper for a model
///
/// # Arguments
///
/// - Model type name (e.g., `Message`)
/// - View type name (e.g., `MessageView`)
/// - LiveQuery type name (e.g., `MessageLiveQuery`)
/// - `timestamp_field = "field_name"` - The timestamp field used for pagination
///
/// # Generated Types
///
/// For a model named `Message`, this generates:
/// - `MessageScrollManager` - Platform-specific scroll manager wrapper
///
/// The scroll manager wraps `ankurah_virtual_scroll::ScrollManager` and integrates with
/// the model's `LiveQuery` type for reactive pagination.
///
/// # Features
///
/// - With `uniffi` feature: generates UniFFI-compatible scroll manager
/// - With `wasm` feature: generates WASM-compatible scroll manager
#[proc_macro]
pub fn generate_scroll_manager(input: TokenStream) -> TokenStream {
    let config = parse_macro_input!(input as ScrollManagerConfig);

    let model_name = config.model_name();
    let scroll_manager_name = format_ident!("{}ScrollManager", model_name);
    let view_path = &config.view_path;
    let livequery_path = &config.livequery_path;
    let timestamp_field = &config.timestamp_field;

    // Generate UniFFI implementation
    let uniffi_impl = uniffi::generate_with_paths(&scroll_manager_name, view_path, livequery_path, timestamp_field);

    // Generate WASM implementation
    let wasm_impl = wasm::generate_with_paths(&scroll_manager_name, view_path, livequery_path, timestamp_field);

    let expanded = quote! {
        #uniffi_impl
        #wasm_impl
    };

    expanded.into()
}

// Keep the derive macro for backwards compatibility (deprecated)
/// **Deprecated**: Use `generate_scroll_manager!` macro in bindings crate instead.
///
/// This derive macro is deprecated because it requires ankurah-virtual-scroll dependency
/// in the model crate, which should remain platform-agnostic.
#[proc_macro_derive(VirtualScroll, attributes(virtual_scroll))]
pub fn derive_virtual_scroll(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);

    let timestamp_field = match parse_timestamp_field(&input) {
        Ok(field) => field,
        Err(e) => return e.to_compile_error().into(),
    };

    let model_name = &input.ident;
    let scroll_manager_name = format_ident!("{}ScrollManager", model_name);
    let view_name = format_ident!("{}View", model_name);
    let livequery_name = format_ident!("{}LiveQuery", model_name);

    // Generate UniFFI implementation
    let uniffi_impl = uniffi::generate(&scroll_manager_name, &view_name, &livequery_name, &timestamp_field);

    // Generate WASM implementation
    let wasm_impl = wasm::generate(&scroll_manager_name, &view_name, &livequery_name, &timestamp_field);

    let hygiene_module = format_ident!("__virtual_scroll_impl_{}", to_snake_case(&model_name.to_string()));

    let expanded = quote! {
        mod #hygiene_module {
            use super::*;

            #uniffi_impl
            #wasm_impl
        }
        pub use #hygiene_module::*;
    };

    expanded.into()
}

fn parse_timestamp_field(input: &syn::DeriveInput) -> Result<String, syn::Error> {
    for attr in &input.attrs {
        if attr.path().is_ident("virtual_scroll") {
            let mut timestamp_field = None;
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("timestamp_field") {
                    let value: LitStr = meta.value()?.parse()?;
                    timestamp_field = Some(value.value());
                    Ok(())
                } else {
                    Err(meta.error("unknown attribute"))
                }
            })?;
            if let Some(field) = timestamp_field {
                return Ok(field);
            }
        }
    }

    Err(syn::Error::new_spanned(
        input,
        "missing required attribute: #[virtual_scroll(timestamp_field = \"...\")]",
    ))
}

/// Convert PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    s.chars()
        .enumerate()
        .flat_map(|(i, c)| {
            if c.is_uppercase() && i > 0 {
                vec!['_', c.to_lowercase().next().unwrap()]
            } else {
                vec![c.to_lowercase().next().unwrap()]
            }
        })
        .collect()
}
