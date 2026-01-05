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
                has_more_preceding: bool,
                has_more_following: bool,
                should_auto_scroll: bool,
            }

            #[wasm_bindgen]
            impl #visible_set_name {
                #[wasm_bindgen(getter)]
                pub fn items(&self) -> Vec<#view_type> {
                    self.items.clone()
                }

                #[wasm_bindgen(js_name = hasMorePreceding)]
                pub fn has_more_preceding(&self) -> bool {
                    self.has_more_preceding
                }

                #[wasm_bindgen(js_name = hasMoreFollowing)]
                pub fn has_more_following(&self) -> bool {
                    self.has_more_following
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
                inner: ::ankurah_signals::Read<::ankurah_virtual_scroll::VisibleSet<#view_type>>,
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
                        has_more_preceding: vs.has_more_preceding,
                        has_more_following: vs.has_more_following,
                        should_auto_scroll: vs.should_auto_scroll,
                    }
                }
            }

            /// WASM wrapper for ScrollManager
            ///
            /// Manages virtual scroll state and integrates with Ankurah's LiveQuery.
            #[wasm_bindgen]
            pub struct #scroll_manager_name {
                inner: Rc<::ankurah_virtual_scroll::ScrollManager<#view_type>>,
            }

            #[wasm_bindgen]
            impl #scroll_manager_name {
                /// Create a new scroll manager
                ///
                /// # Arguments
                /// * `ctx` - Ankurah context
                /// * `predicate` - Base filter predicate (e.g., "room = 'abc'")
                /// * `order_by` - ORDER BY clause (e.g., "timestamp DESC")
                /// * `minimum_row_height` - Guaranteed minimum item height in pixels
                /// * `buffer_factor` - Buffer as multiple of viewport (2.0 = 2x viewport buffer)
                /// * `viewport_height` - Viewport height in pixels
                #[wasm_bindgen(constructor)]
                pub fn new(
                    ctx: &::ankurah::core::context::Context,
                    predicate: String,
                    order_by: String,
                    minimum_row_height: u32,
                    buffer_factor: f64,
                    viewport_height: u32,
                ) -> Result<#scroll_manager_name, JsValue> {
                    let order_by = ::ankurah_virtual_scroll::parse_order_by(&order_by)
                        .map_err(|e| JsValue::from_str(&e))?;

                    let manager = ::ankurah_virtual_scroll::ScrollManager::<#view_type>::new(
                        ctx,
                        &predicate as &str,
                        order_by,
                        minimum_row_height,
                        buffer_factor,
                        viewport_height,
                    ).map_err(|e| JsValue::from_str(&format!("Failed to create ScrollManager: {:?}", e)))?;

                    Ok(Self {
                        inner: Rc::new(manager),
                    })
                }

                /// Initialize the scroll manager and populate initial items
                ///
                /// Must be called after construction.
                #[wasm_bindgen]
                pub async fn start(&self) -> Result<(), JsValue> {
                    self.inner.start().await;
                    Ok(())
                }

                /// Get the visible set signal
                ///
                /// Returns a signal wrapper - call .get() to read current value.
                /// In React with signalObserver, .get() automatically subscribes to changes.
                #[wasm_bindgen(js_name = visibleSet)]
                pub fn visible_set(&self) -> #visible_set_signal_name {
                    #visible_set_signal_name {
                        inner: self.inner.visible_set(),
                    }
                }

                /// Process a scroll event
                ///
                /// # Arguments
                /// * `first_visible` - EntityId of the first (oldest) visible item
                /// * `last_visible` - EntityId of the last (newest) visible item
                /// * `scrolling_backward` - True if user is scrolling toward older items
                #[wasm_bindgen(js_name = onScroll)]
                pub fn on_scroll(
                    &self,
                    first_visible: String,
                    last_visible: String,
                    scrolling_backward: bool,
                ) {
                    let first_id: ::ankurah_virtual_scroll::Id = first_visible.parse()
                        .expect("Invalid first_visible EntityId");
                    let last_id: ::ankurah_virtual_scroll::Id = last_visible.parse()
                        .expect("Invalid last_visible EntityId");
                    self.inner.on_scroll(first_id, last_id, scrolling_backward);
                }

                /// Get the current scroll mode
                #[wasm_bindgen(getter)]
                pub fn mode(&self) -> String {
                    format!("{:?}", self.inner.mode())
                }

                /// Get the current selection (predicate + order by) as a string.
                #[wasm_bindgen(js_name = currentSelection)]
                pub fn current_selection(&self) -> String {
                    self.inner.current_selection()
                }
            }
        }

        #[cfg(feature = "wasm")]
        pub use __wasm_scroll_manager::*;
    }
}
