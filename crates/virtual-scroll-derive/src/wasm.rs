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
    // Extract the model name from scroll manager name (e.g., "Message" from "MessageScrollManager")
    let model_name = scroll_manager_name.to_string().replace("ScrollManager", "");

    // Generate names following the pattern: {Model}VisibleSet and {Model}VisibleSetSignal
    let visible_set_signal_name = syn::Ident::new(
        &format!("{}VisibleSetSignal", model_name),
        scroll_manager_name.span(),
    );
    let visible_set_name = syn::Ident::new(
        &format!("{}VisibleSet", model_name),
        scroll_manager_name.span(),
    );

    quote! {
        #[cfg(feature = "wasm")]
        mod __wasm_scroll_manager {
            use super::*;
            use ::wasm_bindgen::prelude::*;
            use ::ankurah_signals::Peek;
            use ::std::cell::RefCell;
            use ::std::rc::Rc;

            /// WASM wrapper for VisibleSet data
            #[wasm_bindgen]
            pub struct #visible_set_name {
                items: Vec<#view_type>,
                intersection_entity_id: Option<String>,
                intersection_index: Option<usize>,
                has_more_older: bool,
                has_more_newer: bool,
                should_auto_scroll: bool,
            }

            #[wasm_bindgen]
            impl #visible_set_name {
                #[wasm_bindgen(getter)]
                pub fn items(&self) -> Vec<#view_type> {
                    self.items.clone()
                }

                #[wasm_bindgen(js_name = hasMoreOlder)]
                pub fn has_more_older(&self) -> bool {
                    self.has_more_older
                }

                #[wasm_bindgen(js_name = hasMoreNewer)]
                pub fn has_more_newer(&self) -> bool {
                    self.has_more_newer
                }

                #[wasm_bindgen(js_name = shouldAutoScroll)]
                pub fn should_auto_scroll(&self) -> bool {
                    self.should_auto_scroll
                }

                /// Get the intersection item info (for scroll stability)
                pub fn intersection(&self) -> JsValue {
                    match (&self.intersection_entity_id, &self.intersection_index) {
                        (Some(entity_id), Some(index)) => {
                            let obj = ::ankurah::derive_deps::js_sys::Object::new();
                            let _ = ::ankurah::derive_deps::js_sys::Reflect::set(
                                &obj,
                                &JsValue::from_str("entityId"),
                                &JsValue::from_str(entity_id),
                            );
                            let _ = ::ankurah::derive_deps::js_sys::Reflect::set(
                                &obj,
                                &JsValue::from_str("index"),
                                &JsValue::from_f64(*index as f64),
                            );
                            obj.into()
                        }
                        _ => JsValue::NULL,
                    }
                }
            }

            /// WASM wrapper for VisibleSet signal - call .get() to read current value
            #[wasm_bindgen]
            pub struct #visible_set_signal_name {
                inner: ::ankurah_signals::Read<::virtual_scroll::VisibleSet<#view_type>>,
            }

            #[wasm_bindgen]
            impl #visible_set_signal_name {
                /// Get the current visible set value
                ///
                /// In React, calling this from within a signalObserver component
                /// will automatically subscribe to changes.
                pub fn get(&self) -> #visible_set_name {
                    use ::ankurah_signals::Get;
                    let vs = self.inner.get();
                    #visible_set_name {
                        items: vs.items.clone(),
                        intersection_entity_id: vs.intersection.as_ref().map(|i| i.entity_id.to_string()),
                        intersection_index: vs.intersection.as_ref().map(|i| i.index),
                        has_more_older: vs.has_more_older,
                        has_more_newer: vs.has_more_newer,
                        should_auto_scroll: vs.should_auto_scroll,
                    }
                }
            }

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
                ///
                /// Call `setViewportHeight()` once the container is rendered to set the actual viewport height.
                #[wasm_bindgen(constructor)]
                pub fn new(
                    ctx: &::ankurah::core::context::Context,
                    predicate: String,
                    order_by: String,
                ) -> Result<#scroll_manager_name, JsValue> {
                    let order_by = ::virtual_scroll::parse_order_by(&order_by)
                        .map_err(|e| JsValue::from_str(&e))?;

                    let manager = ::virtual_scroll::ScrollManager::<#view_type>::new(
                        ctx,
                        &predicate as &str,
                        order_by,
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
                    let manager = self.inner.borrow();
                    manager.start().await;
                    Ok(())
                }

                /// Get the visible set signal
                ///
                /// Returns a signal wrapper - call .get() to read current value.
                /// In React with signalObserver, .get() automatically subscribes to changes.
                #[wasm_bindgen(js_name = visibleSet)]
                pub fn visible_set(&self) -> #visible_set_signal_name {
                    let manager = self.inner.borrow();
                    #visible_set_signal_name {
                        inner: manager.visible_set(),
                    }
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
                    let manager = self.inner.borrow();
                    let result = manager.on_scroll(top_gap, bottom_gap, scrolling_up).await;
                    match result {
                        Some(dir) => Ok(JsValue::from_str(&format!("{:?}", dir))),
                        None => Ok(JsValue::NULL),
                    }
                }

                /// Get the loading state
                #[wasm_bindgen(js_name = isLoading)]
                pub fn is_loading(&self) -> bool {
                    let manager = self.inner.borrow();
                    manager.is_loading().peek()
                }

                /// Jump to live mode (most recent content)
                #[wasm_bindgen(js_name = jumpToLive)]
                pub async fn jump_to_live(&self) -> Result<(), JsValue> {
                    let manager = self.inner.borrow();
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
                    let manager = self.inner.borrow();
                    manager.update_filter(&predicate as &str, reset_position).await;
                    Ok(())
                }

                /// Update viewport height (call when container resizes)
                #[wasm_bindgen(js_name = setViewportHeight)]
                pub fn set_viewport_height(&self, height: f64) {
                    let manager = self.inner.borrow();
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
