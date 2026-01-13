//! UniFFI wrapper generation for ScrollManager
//!
//! Generates `{Model}ScrollManager` with `#[uniffi::Object]` and `#[uniffi::export]`
//!
//! The wrapper owns a `ScrollManager<ViewType>` and exposes it to React Native
//! with UniFFI-compatible methods.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Path};

/// Generate UniFFI implementation for the scroll manager (with paths)
pub fn generate_with_paths(
    scroll_manager_name: &Ident,
    view_path: &Path,
    _livequery_path: &Path,
    timestamp_field: &str,
) -> TokenStream {
    let model_name = &scroll_manager_name.to_string().replace("ScrollManager", "");
    let model_ident = Ident::new(model_name, scroll_manager_name.span());

    generate_impl(scroll_manager_name, &model_ident, view_path, timestamp_field)
}

/// Generate UniFFI implementation for the scroll manager (with idents - for backwards compat)
pub fn generate(
    scroll_manager_name: &Ident,
    view_name: &Ident,
    _livequery_name: &Ident,
    timestamp_field: &str,
) -> TokenStream {
    let model_name = &scroll_manager_name.to_string().replace("ScrollManager", "");
    let model_ident = Ident::new(model_name, scroll_manager_name.span());

    generate_impl(scroll_manager_name, &model_ident, &syn::parse_quote!(#view_name), timestamp_field)
}

fn generate_impl(
    scroll_manager_name: &Ident,
    model_name: &Ident,
    view_path: &Path,
    _timestamp_field: &str,
) -> TokenStream {
    let visible_set_name = format_ident!("{}VisibleSet", model_name);
    let visible_set_signal_name = format_ident!("{}VisibleSetSignal", model_name);
    let intersection_name = format_ident!("{}Intersection", model_name);
    let callback_name = format_ident!("{}VisibleSetCallback", model_name);

    quote! {
        // Callback interface for signal subscription
        #[cfg(feature = "uniffi")]
        #[::uniffi::export(callback_interface)]
        pub trait #callback_name: Send + Sync {
            fn on_change(&self, value: ::std::sync::Arc<#visible_set_name>);
        }

        #[cfg(feature = "uniffi")]
        mod __uniffi_scroll_manager {
            use super::*;
            use ::std::sync::Arc;
            use ::ankurah_virtual_scroll::ankurah_signals::{Get, Peek, Subscribe};

            /// Intersection item for scroll stability
            #[derive(::uniffi::Object)]
            pub struct #intersection_name {
                entity_id: String,
                index: u32,
            }

            #[::uniffi::export]
            impl #intersection_name {
                #[uniffi::method]
                pub fn entity_id(&self) -> String {
                    self.entity_id.clone()
                }

                #[uniffi::method]
                pub fn index(&self) -> u32 {
                    self.index
                }
            }

            /// Visible set containing items and scroll state
            #[derive(::uniffi::Object)]
            pub struct #visible_set_name {
                items: Vec<::std::sync::Arc<#view_path>>,
                intersection: Option<::std::sync::Arc<#intersection_name>>,
                has_more_preceding: bool,
                has_more_following: bool,
                should_auto_scroll: bool,
            }

            #[::uniffi::export]
            impl #visible_set_name {
                #[uniffi::method]
                pub fn items(&self) -> Vec<::std::sync::Arc<#view_path>> {
                    self.items.clone()
                }

                #[uniffi::method]
                pub fn intersection(&self) -> Option<::std::sync::Arc<#intersection_name>> {
                    self.intersection.clone()
                }

                #[uniffi::method]
                pub fn has_more_preceding(&self) -> bool {
                    self.has_more_preceding
                }

                #[uniffi::method]
                pub fn has_more_following(&self) -> bool {
                    self.has_more_following
                }

                #[uniffi::method]
                pub fn should_auto_scroll(&self) -> bool {
                    self.should_auto_scroll
                }
            }

            impl #visible_set_name {
                fn from_core(core: &::ankurah_virtual_scroll::VisibleSet<#view_path>) -> ::std::sync::Arc<Self> {
                    let intersection = core.intersection.as_ref().map(|i| {
                        ::std::sync::Arc::new(#intersection_name {
                            entity_id: i.entity_id.to_string(),
                            index: i.index as u32,
                        })
                    });

                    ::std::sync::Arc::new(Self {
                        items: core.items.iter().map(|v| ::std::sync::Arc::new(v.clone())).collect(),
                        intersection,
                        has_more_preceding: core.has_more_preceding,
                        has_more_following: core.has_more_following,
                        should_auto_scroll: core.should_auto_scroll,
                    })
                }
            }

            /// Signal wrapper for visible_set - exposes get() and subscribe()
            #[derive(::uniffi::Object)]
            pub struct #visible_set_signal_name {
                manager: Arc<#scroll_manager_name>,
                _subscriptions: ::std::sync::Mutex<Vec<::ankurah_virtual_scroll::ankurah_signals::SubscriptionGuard>>,
            }

            #[::uniffi::export]
            impl #visible_set_signal_name {
                #[uniffi::method]
                pub fn get(&self) -> Arc<#visible_set_name> {
                    #visible_set_name::from_core(&self.manager.0.visible_set().get())
                }

                #[uniffi::method]
                pub fn subscribe(&self, callback: Box<dyn #callback_name>) {
                    let cb = Arc::new(callback);
                    let signal = self.manager.0.visible_set();
                    let initial = #visible_set_name::from_core(&signal.get());
                    let cb_clone = cb.clone();
                    let guard = signal.subscribe(move |visible_set| {
                        cb_clone.on_change(#visible_set_name::from_core(&visible_set));
                    });
                    self._subscriptions.lock().unwrap().push(guard);
                    cb.on_change(initial);
                }
            }

            impl #visible_set_signal_name {
                fn new(manager: Arc<#scroll_manager_name>) -> Arc<Self> {
                    Arc::new(Self {
                        manager,
                        _subscriptions: ::std::sync::Mutex::new(Vec::new()),
                    })
                }
            }

            /// UniFFI wrapper for ScrollManager
            #[derive(::uniffi::Object)]
            pub struct #scroll_manager_name(::ankurah_virtual_scroll::ScrollManager<#view_path>);

            #[::uniffi::export]
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
                #[uniffi::constructor]
                pub fn new(
                    ctx: &::ankurah::Context,
                    predicate: String,
                    order_by: String,
                    minimum_row_height: u32,
                    buffer_factor: f64,
                    viewport_height: u32,
                ) -> Result<Arc<Self>, ::ankurah::error::RetrievalError> {
                    let order_by = ::ankurah_virtual_scroll::parse_order_by(&order_by)
                        .map_err(|e| ::ankurah::error::RetrievalError::Other(e.into()))?;
                    Ok(Arc::new(Self(::ankurah_virtual_scroll::ScrollManager::<#view_path>::new(
                        ctx,
                        &predicate as &str,
                        order_by,
                        minimum_row_height,
                        buffer_factor,
                        viewport_height,
                    )?)))
                }

                #[uniffi::method]
                pub fn visible_set(self: Arc<Self>) -> Arc<#visible_set_signal_name> {
                    #visible_set_signal_name::new(self)
                }

                /// Initialize the scroll manager and populate initial items
                #[uniffi::method]
                pub async fn start(self: Arc<Self>) {
                    self.0.start().await;
                }

                /// Process a scroll event
                ///
                /// # Arguments
                /// * `first_visible` - EntityId string of the first (oldest) visible item
                /// * `last_visible` - EntityId string of the last (newest) visible item
                /// * `scrolling_backward` - True if user is scrolling toward older items
                #[uniffi::method]
                pub fn on_scroll(self: Arc<Self>, first_visible: String, last_visible: String, scrolling_backward: bool) {
                    let first_id: ::ankurah_virtual_scroll::Id = first_visible.parse()
                        .expect("Invalid first_visible EntityId");
                    let last_id: ::ankurah_virtual_scroll::Id = last_visible.parse()
                        .expect("Invalid last_visible EntityId");
                    self.0.on_scroll(first_id, last_id, scrolling_backward);
                }

                /// Get the current scroll mode
                #[uniffi::method]
                pub fn mode(&self) -> String {
                    format!("{:?}", self.0.mode())
                }

                /// Get the current selection (predicate + order by) as a string
                #[uniffi::method]
                pub fn current_selection(&self) -> String {
                    self.0.current_selection()
                }
            }
        }

        #[cfg(feature = "uniffi")]
        pub use __uniffi_scroll_manager::*;
    }
}
