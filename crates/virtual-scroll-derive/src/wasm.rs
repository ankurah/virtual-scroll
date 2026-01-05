//! WASM wrapper generation for ScrollManager
//!
//! Generates `{Model}ScrollManager` with `#[wasm_bindgen]`
//!
//! The wrapper owns a `ScrollManager<ViewType>` and exposes it to JavaScript
//! with wasm_bindgen-compatible methods.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Path};

/// Generate WASM implementation for the scroll manager (with paths)
pub fn generate_with_paths(
    scroll_manager_name: &Ident,
    view_path: &Path,
    _livequery_path: &Path,
    timestamp_field: &str,
) -> TokenStream {
    generate_impl(scroll_manager_name, quote!(#view_path), timestamp_field)
}

/// Generate WASM implementation for the scroll manager (with idents - for backwards compat)
pub fn generate(
    scroll_manager_name: &Ident,
    view_name: &Ident,
    _livequery_name: &Ident,
    timestamp_field: &str,
) -> TokenStream {
    generate_impl(scroll_manager_name, quote!(#view_name), timestamp_field)
}

fn generate_impl(
    scroll_manager_name: &Ident,
    view_type: TokenStream,
    _timestamp_field: &str,
) -> TokenStream {
    quote! {
        #[cfg(feature = "wasm")]
        mod __wasm_scroll_manager {
            use super::*;
            use ::wasm_bindgen::prelude::*;
            use ::ankurah_signals::Peek;
            use ::std::cell::RefCell;
            use ::std::rc::Rc;

            /// WASM wrapper for ScrollManager
            ///
            /// Manages virtual scroll state and integrates with Ankurah's LiveQuery.
            #[wasm_bindgen]
            pub struct #scroll_manager_name {
                inner: Rc<RefCell<::virtual_scroll::ScrollManager<#view_type>>>,
            }

            #[wasm_bindgen]
            impl #scroll_manager_name {
                /// Create a new scroll manager
                ///
                /// # Arguments
                /// * `ctx` - Ankurah context
                /// * `predicate` - Base filter predicate (e.g., "room = 'abc'")
                /// * `order_by` - ORDER BY clause (e.g., "timestamp DESC")
                /// * `viewport_height` - Initial viewport height in pixels
                #[wasm_bindgen(constructor)]
                pub fn new(
                    ctx: &::ankurah::core::context::Context,
                    predicate: String,
                    order_by: String,
                    viewport_height: f64,
                ) -> Result<#scroll_manager_name, JsValue> {
                    let order_by = ::virtual_scroll::parse_order_by(&order_by)
                        .map_err(|e| JsValue::from_str(&e))?;

                    let manager = ::virtual_scroll::ScrollManager::<#view_type>::new(
                        ctx,
                        &predicate as &str,
                        order_by,
                        viewport_height,
                    ).map_err(|e| JsValue::from_str(&format!("Failed to create ScrollManager: {:?}", e)))?;

                    Ok(Self {
                        inner: Rc::new(RefCell::new(manager)),
                    })
                }

                /// Initialize the scroll manager and populate initial items
                ///
                /// Must be called after construction.
                #[wasm_bindgen]
                pub async fn start(&self) -> Result<(), JsValue> {
                    let mut manager = self.inner.borrow_mut();
                    manager.start().await;
                    Ok(())
                }

                /// Process a scroll event
                ///
                /// Returns the load direction if pagination was triggered, null otherwise.
                #[wasm_bindgen(js_name = onScroll)]
                pub async fn on_scroll(
                    &self,
                    top_gap: f64,
                    bottom_gap: f64,
                    scrolling_up: bool,
                ) -> Result<JsValue, JsValue> {
                    let mut manager = self.inner.borrow_mut();
                    let result = manager.on_scroll(top_gap, bottom_gap, scrolling_up).await;
                    match result {
                        Some(dir) => Ok(JsValue::from_str(&format!("{:?}", dir))),
                        None => Ok(JsValue::NULL),
                    }
                }

                /// Get the loading state signal
                #[wasm_bindgen(js_name = isLoading)]
                pub fn is_loading(&self) -> bool {
                    let manager = self.inner.borrow();
                    manager.is_loading().peek()
                }

                /// Get current items (convenience getter)
                #[wasm_bindgen(getter)]
                pub fn items(&self) -> Vec<#view_type> {
                    let manager = self.inner.borrow();
                    manager.visible_set().peek().items.clone()
                }

                /// Check if there's more older content to load
                #[wasm_bindgen(js_name = hasMoreOlder)]
                pub fn has_more_older(&self) -> bool {
                    let manager = self.inner.borrow();
                    manager.visible_set().peek().has_more_older
                }

                /// Check if there's more newer content to load
                #[wasm_bindgen(js_name = hasMoreNewer)]
                pub fn has_more_newer(&self) -> bool {
                    let manager = self.inner.borrow();
                    manager.visible_set().peek().has_more_newer
                }

                /// Check if auto-scroll should be enabled
                #[wasm_bindgen(js_name = shouldAutoScroll)]
                pub fn should_auto_scroll(&self) -> bool {
                    let manager = self.inner.borrow();
                    manager.visible_set().peek().should_auto_scroll
                }

                /// Get the intersection item info (for scroll stability)
                ///
                /// Returns null if no intersection, otherwise {entityId, index}
                #[wasm_bindgen(getter)]
                pub fn intersection(&self) -> JsValue {
                    let manager = self.inner.borrow();
                    match &manager.visible_set().peek().intersection {
                        Some(intersection) => {
                            let obj = ::ankurah::derive_deps::js_sys::Object::new();
                            let _ = ::ankurah::derive_deps::js_sys::Reflect::set(
                                &obj,
                                &JsValue::from_str("entityId"),
                                &JsValue::from_str(&intersection.entity_id.to_string()),
                            );
                            let _ = ::ankurah::derive_deps::js_sys::Reflect::set(
                                &obj,
                                &JsValue::from_str("index"),
                                &JsValue::from_f64(intersection.index as f64),
                            );
                            obj.into()
                        }
                        None => JsValue::NULL,
                    }
                }

                /// Jump to live mode (most recent content)
                #[wasm_bindgen(js_name = jumpToLive)]
                pub async fn jump_to_live(&self) -> Result<(), JsValue> {
                    let mut manager = self.inner.borrow_mut();
                    manager.jump_to_live().await;
                    Ok(())
                }

                /// Update the filter predicate
                ///
                /// If reset_position is true, returns to live mode.
                #[wasm_bindgen(js_name = updateFilter)]
                pub async fn update_filter(
                    &self,
                    predicate: String,
                    reset_position: bool,
                ) -> Result<(), JsValue> {
                    let mut manager = self.inner.borrow_mut();
                    manager.update_filter(&predicate as &str, reset_position).await;
                    Ok(())
                }

                /// Update viewport height (call when container resizes)
                #[wasm_bindgen(js_name = setViewportHeight)]
                pub fn set_viewport_height(&self, height: f64) {
                    let mut manager = self.inner.borrow_mut();
                    manager.set_viewport_height(height);
                }

                /// Get the current scroll mode
                #[wasm_bindgen(getter)]
                pub fn mode(&self) -> String {
                    let manager = self.inner.borrow();
                    format!("{:?}", manager.mode())
                }
            }
        }

        #[cfg(feature = "wasm")]
        pub use __wasm_scroll_manager::*;
    }
}
