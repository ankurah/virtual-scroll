//! UniFFI wrapper generation for ScrollManager
//!
//! Generates `{Model}ScrollManager` with `#[uniffi::Object]` and `#[uniffi::export]`

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Path};

/// Generate UniFFI implementation for the scroll manager (with paths)
pub fn generate_with_paths(
    scroll_manager_name: &Ident,
    view_path: &Path,
    livequery_path: &Path,
    timestamp_field: &str,
) -> TokenStream {
    generate_impl(scroll_manager_name, quote!(#view_path), quote!(#livequery_path), timestamp_field)
}

/// Generate UniFFI implementation for the scroll manager (with idents - for backwards compat)
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
        #[cfg(feature = "uniffi")]
        #[derive(::uniffi::Object)]
        pub struct #scroll_manager_name {
            core: std::sync::Mutex<::virtual_scroll::ScrollManager>,
            live_query: std::sync::Arc<#livequery_type>,
            timestamp_field: &'static str,
        }

        #[cfg(feature = "uniffi")]
        #[::uniffi::export]
        impl #scroll_manager_name {
            /// Create a new scroll manager with a live query
            ///
            /// # Arguments
            /// * `live_query` - The LiveQuery to manage scrolling for
            /// * `base_predicate` - Initial predicate (e.g., "room = 'abc'")
            /// * `viewport_height` - Initial viewport height in pixels
            #[uniffi::constructor]
            pub fn new(
                live_query: std::sync::Arc<#livequery_type>,
                base_predicate: String,
                viewport_height: f64,
            ) -> std::sync::Arc<Self> {
                let core = ::virtual_scroll::ScrollManager::new(
                    &base_predicate,
                    #timestamp_field,
                    viewport_height,
                );
                std::sync::Arc::new(Self {
                    core: std::sync::Mutex::new(core),
                    live_query,
                    timestamp_field: #timestamp_field,
                })
            }

            /// Process a scroll event
            ///
            /// Call this from your scroll handler. If pagination is needed,
            /// this automatically updates the LiveQuery's selection.
            ///
            /// # Arguments
            /// * `offset` - Current scroll position (scrollTop / contentOffset.y)
            /// * `content_height` - Total scrollable height
            /// * `viewport_height` - Visible viewport height
            /// * `scroll_delta` - Change since last event (negative = scrolling up)
            /// * `user_initiated` - True if user is actively scrolling
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
                    let mut core = self.core.lock().unwrap();
                    core.on_scroll(input)
                };

                // If a new selection is needed, update the LiveQuery
                if let Some(selection) = new_selection {
                    // Ignore errors - LiveQuery will handle them
                    let _ = self.live_query.update_selection(selection, vec![]).await;
                }
            }

            /// Notify the scroll manager that new results have arrived
            ///
            /// Call this after LiveQuery results change to update boundary detection.
            /// Pass the timestamps of the oldest and newest items in the result.
            pub fn on_results(
                &self,
                count: u32,
                oldest_timestamp: Option<i64>,
                newest_timestamp: Option<i64>,
            ) {
                let mut core = self.core.lock().unwrap();
                core.on_results(count as usize, oldest_timestamp, newest_timestamp);
            }

            /// Get items in display order (oldest first for chronological view)
            ///
            /// Items are automatically reversed when needed based on scroll mode.
            pub fn items(&self) -> Vec<std::sync::Arc<#view_type>> {
                let items = self.live_query.items();
                let core = self.core.lock().unwrap();
                if core.should_reverse_for_display() {
                    items.into_iter().rev().collect()
                } else {
                    items
                }
            }

            /// Jump to live mode (most recent content)
            ///
            /// Returns to real-time updates with newest content at bottom.
            pub async fn jump_to_live(&self) {
                let selection = {
                    let mut core = self.core.lock().unwrap();
                    core.jump_to_live()
                };
                let _ = self.live_query.update_selection(selection, vec![]).await;
            }

            /// Update the base filter predicate
            ///
            /// # Arguments
            /// * `predicate` - New base predicate
            /// * `reset_continuation` - If true, resets scroll position to live mode
            pub async fn update_filter(&self, predicate: String, reset_continuation: bool) {
                let selection = {
                    let mut core = self.core.lock().unwrap();
                    core.update_filter(&predicate, reset_continuation)
                };
                let _ = self.live_query.update_selection(selection, vec![]).await;
            }

            /// Get the current scroll mode as a string
            ///
            /// Returns "Live", "Backward", or "Forward"
            pub fn mode(&self) -> String {
                let core = self.core.lock().unwrap();
                format!("{:?}", core.mode())
            }

            /// Check if the container should auto-scroll to bottom
            ///
            /// Returns true when in live mode and near the bottom
            pub fn should_auto_scroll(&self) -> bool {
                let core = self.core.lock().unwrap();
                core.should_auto_scroll()
            }

            /// Check if a load is in progress
            pub fn is_loading(&self) -> bool {
                let core = self.core.lock().unwrap();
                core.is_loading()
            }

            /// Check if we've reached the earliest content
            pub fn at_earliest(&self) -> bool {
                let core = self.core.lock().unwrap();
                core.at_earliest()
            }

            /// Check if we're at the latest content
            pub fn at_latest(&self) -> bool {
                let core = self.core.lock().unwrap();
                core.at_latest()
            }

            /// Get the current selection string
            pub fn selection(&self) -> String {
                let core = self.core.lock().unwrap();
                core.selection()
            }

            /// Get debug metrics as a string
            pub fn debug_metrics(&self) -> String {
                let core = self.core.lock().unwrap();
                let m = core.metrics();
                format!(
                    "top_gap: {:.0}, bottom_gap: {:.0}, min_buffer: {:.0}, count: {}",
                    m.top_gap, m.bottom_gap, m.min_buffer, m.result_count
                )
            }
        }
    }
}
