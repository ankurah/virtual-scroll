//! Derive macro for generating typed scroll manager wrappers
//!
//! This crate provides the `#[derive(VirtualScroll)]` macro which generates
//! platform-specific scroll manager types for use with UniFFI and WASM.
//!
//! # Usage
//!
//! ```ignore
//! use virtual_scroll_derive::VirtualScroll;
//!
//! #[derive(Model, VirtualScroll)]
//! #[virtual_scroll(timestamp_field = "timestamp")]
//! pub struct Message {
//!     pub text: YrsString,
//!     pub timestamp: LWW<i64>,
//!     // ...
//! }
//! ```
//!
//! This generates `MessageScrollManager` for both UniFFI and WASM targets.

use proc_macro::TokenStream;

/// Derive macro for generating typed scroll manager wrappers
///
/// # Attributes
///
/// - `#[virtual_scroll(timestamp_field = "field_name")]` - Required. Specifies the
///   timestamp field used for pagination ordering.
#[proc_macro_derive(VirtualScroll, attributes(virtual_scroll))]
pub fn derive_virtual_scroll(_input: TokenStream) -> TokenStream {
    // TODO: Implement in Phase 4
    // - Parse #[virtual_scroll(timestamp_field = "...")] attribute
    // - Generate {Model}ScrollManager for UniFFI
    // - Generate {Model}ScrollManager for WASM
    TokenStream::new()
}
