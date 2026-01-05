//! WASM wrapper generation for ScrollManager
//!
//! Generates `{Model}ScrollManager` with `#[wasm_bindgen]`

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Path};

/// Generate WASM implementation for the scroll manager (with paths)
pub fn generate_with_paths(
    scroll_manager_name: &Ident,
    view_path: &Path,
    livequery_path: &Path,
    timestamp_field: &str,
) -> TokenStream {
    generate_impl(scroll_manager_name, quote!(#view_path), quote!(#livequery_path), timestamp_field)
}

/// Generate WASM implementation for the scroll manager (with idents - for backwards compat)
pub fn generate(
    scroll_manager_name: &Ident,
    view_name: &Ident,
    livequery_name: &Ident,
    timestamp_field: &str,
) -> TokenStream {
    generate_impl(scroll_manager_name, quote!(#view_name), quote!(#livequery_name), timestamp_field)
}

fn generate_impl(
    scroll_manager_name: &Ident,
    view_type: TokenStream,
    livequery_type: TokenStream,
    timestamp_field: &str,
) -> TokenStream {
    quote! {
        #[cfg(feature = "wasm")]
        #[::wasm_bindgen::prelude::wasm_bindgen]
        pub struct #scroll_manager_name {
            core: std::cell::RefCell<::virtual_scroll::ScrollManager>,
            live_query: #livequery_type,
            timestamp_field: &'static str,
        }

        #[cfg(feature = "wasm")]
        #[::wasm_bindgen::prelude::wasm_bindgen]
        impl #scroll_manager_name {
            /// Create a new scroll manager with a live query
            #[wasm_bindgen(constructor)]
            pub fn new(
                live_query: #livequery_type,
                base_predicate: String,
                viewport_height: f64,
            ) -> Self {
                let core = ::virtual_scroll::ScrollManager::new(
                    &base_predicate,
                    #timestamp_field,
                    viewport_height,
                );
                Self {
                    core: std::cell::RefCell::new(core),
                    live_query,
                    timestamp_field: #timestamp_field,
                }
            }

            /// Process a scroll event
            ///
            /// Call this from your scroll handler. If pagination is needed,
            /// this automatically updates the LiveQuery's selection.
            #[wasm_bindgen(js_name = onScroll)]
            pub async fn on_scroll(
                &self,
                offset: f64,
                content_height: f64,
                viewport_height: f64,
                scroll_delta: f64,
                user_initiated: bool,
            ) {
                let input = ::virtual_scroll::ScrollInput {
                    offset,
                    content_height,
                    viewport_height,
                    scroll_delta,
                    user_initiated,
                };

                let new_selection = {
                    let mut core = self.core.borrow_mut();
                    core.on_scroll(input)
                };

                // If a new selection is needed, update the LiveQuery
                if let Some(selection) = new_selection {
                    let empty_args = ::wasm_bindgen::JsValue::from(::ankurah::derive_deps::js_sys::Array::new());
                    let _ = self.live_query.update_selection(selection, &empty_args).await;
                }
            }

            /// Notify the scroll manager that new results have arrived
            #[wasm_bindgen(js_name = onResults)]
            pub fn on_results(
                &self,
                count: u32,
                oldest_timestamp: Option<i64>,
                newest_timestamp: Option<i64>,
            ) {
                let mut core = self.core.borrow_mut();
                core.on_results(count as usize, oldest_timestamp, newest_timestamp);
            }

            /// Get items in display order (oldest first for chronological view)
            #[wasm_bindgen(getter)]
            pub fn items(&self) -> Vec<#view_type> {
                let items = self.live_query.items();
                let core = self.core.borrow();
                if core.should_reverse_for_display() {
                    items.into_iter().rev().collect()
                } else {
                    items
                }
            }

            /// Jump to live mode (most recent content)
            #[wasm_bindgen(js_name = jumpToLive)]
            pub async fn jump_to_live(&self) {
                let selection = {
                    let mut core = self.core.borrow_mut();
                    core.jump_to_live()
                };
                let empty_args = ::wasm_bindgen::JsValue::from(::ankurah::derive_deps::js_sys::Array::new());
                let _ = self.live_query.update_selection(selection, &empty_args).await;
            }

            /// Update the base filter predicate
            #[wasm_bindgen(js_name = updateFilter)]
            pub async fn update_filter(&self, predicate: String, reset_continuation: bool) {
                let selection = {
                    let mut core = self.core.borrow_mut();
                    core.update_filter(&predicate, reset_continuation)
                };
                let empty_args = ::wasm_bindgen::JsValue::from(::ankurah::derive_deps::js_sys::Array::new());
                let _ = self.live_query.update_selection(selection, &empty_args).await;
            }

            /// Get the current scroll mode
            #[wasm_bindgen(getter)]
            pub fn mode(&self) -> String {
                let core = self.core.borrow();
                format!("{:?}", core.mode())
            }

            /// Check if the container should auto-scroll to bottom
            #[wasm_bindgen(js_name = shouldAutoScroll)]
            pub fn should_auto_scroll(&self) -> bool {
                let core = self.core.borrow();
                core.should_auto_scroll()
            }

            /// Check if a load is in progress
            #[wasm_bindgen(js_name = isLoading)]
            pub fn is_loading(&self) -> bool {
                let core = self.core.borrow();
                core.is_loading()
            }

            /// Check if we've reached the earliest content
            #[wasm_bindgen(js_name = atEarliest)]
            pub fn at_earliest(&self) -> bool {
                let core = self.core.borrow();
                core.at_earliest()
            }

            /// Check if we're at the latest content
            #[wasm_bindgen(js_name = atLatest)]
            pub fn at_latest(&self) -> bool {
                let core = self.core.borrow();
                core.at_latest()
            }

            /// Get the current selection string
            #[wasm_bindgen(getter)]
            pub fn selection(&self) -> String {
                let core = self.core.borrow();
                core.selection()
            }

            /// Get debug metrics as a string
            #[wasm_bindgen(js_name = debugMetrics)]
            pub fn debug_metrics(&self) -> String {
                let core = self.core.borrow();
                let m = core.metrics();
                format!(
                    "top_gap: {:.0}, bottom_gap: {:.0}, min_buffer: {:.0}, count: {}",
                    m.top_gap, m.bottom_gap, m.min_buffer, m.result_count
                )
            }
        }
    }
}
