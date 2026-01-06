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
            use ::tokio::sync::Mutex;
            use ::virtual_scroll::ankurah_signals::{Peek, Subscribe};

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
                has_more_older: bool,
                has_more_newer: bool,
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
                pub fn has_more_older(&self) -> bool {
                    self.has_more_older
                }

                #[uniffi::method]
                pub fn has_more_newer(&self) -> bool {
                    self.has_more_newer
                }

                #[uniffi::method]
                pub fn should_auto_scroll(&self) -> bool {
                    self.should_auto_scroll
                }
            }

            impl #visible_set_name {
                fn from_core(core: &::virtual_scroll::VisibleSet<#view_path>) -> ::std::sync::Arc<Self> {
                    let intersection = core.intersection.as_ref().map(|i| {
                        ::std::sync::Arc::new(#intersection_name {
                            entity_id: i.entity_id.to_string(),
                            index: i.index as u32,
                        })
                    });

                    ::std::sync::Arc::new(Self {
                        items: core.items.iter().map(|v| ::std::sync::Arc::new(v.clone())).collect(),
                        intersection,
                        has_more_older: core.has_more_older,
                        has_more_newer: core.has_more_newer,
                        should_auto_scroll: core.should_auto_scroll,
                    })
                }
            }

            /// Signal wrapper for visible_set - exposes get() and subscribe()
            #[derive(::uniffi::Object)]
            pub struct #visible_set_signal_name {
                manager: Arc<#scroll_manager_name>,
                _subscriptions: ::std::sync::Mutex<Vec<::virtual_scroll::ankurah_signals::SubscriptionGuard>>,
            }

            #[::uniffi::export]
            impl #visible_set_signal_name {
                /// Get the current value
                #[uniffi::method]
                pub fn get(&self) -> Arc<#visible_set_name> {
                    let manager = self.manager.inner.blocking_lock();
                    let signal = manager.visible_set();
                    #visible_set_name::from_core(&signal.peek())
                }

                /// Subscribe to changes. Returns immediately and calls callback on each change.
                /// The callback is also called immediately with the current value.
                #[uniffi::method]
                pub fn subscribe(&self, callback: Box<dyn #callback_name>) {
                    let cb = Arc::new(callback);

                    // Get current value and subscribe while holding lock
                    let (guard, initial) = {
                        let manager = self.manager.inner.blocking_lock();
                        let signal = manager.visible_set();
                        let initial = #visible_set_name::from_core(&signal.peek());

                        let cb_clone = cb.clone();
                        let guard = signal.subscribe(move |visible_set| {
                            let wrapped = #visible_set_name::from_core(&visible_set);
                            cb_clone.on_change(wrapped);
                        });
                        (guard, initial)
                    };

                    // Store subscription guard to keep it alive
                    self._subscriptions.lock().unwrap().push(guard);

                    // Call with initial value (outside lock)
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
            pub struct #scroll_manager_name {
                inner: Mutex<::virtual_scroll::ScrollManager<#view_path>>,
            }

            #[::uniffi::export]
            impl #scroll_manager_name {
                #[uniffi::constructor]
                pub fn new(
                    ctx: &::ankurah::Context,
                    predicate: String,
                    order_by: String,
                ) -> Result<Arc<Self>, ::ankurah::error::RetrievalError> {
                    let order_by = ::virtual_scroll::parse_order_by(&order_by)
                        .map_err(|e| ::ankurah::error::RetrievalError::Other(e.into()))?;

                    let manager = ::virtual_scroll::ScrollManager::<#view_path>::new(
                        ctx,
                        &predicate as &str,
                        order_by,
                    )?;

                    Ok(Arc::new(Self {
                        inner: Mutex::new(manager),
                    }))
                }

                /// Get the visible_set signal for subscribing to changes
                #[uniffi::method]
                pub fn visible_set(self: Arc<Self>) -> Arc<#visible_set_signal_name> {
                    #visible_set_signal_name::new(self)
                }

                /// Initialize and start populating items
                #[uniffi::method]
                pub async fn start(self: Arc<Self>) {
                    let manager = self.inner.lock().await;
                    manager.start().await;
                }

                /// Process a scroll event
                #[uniffi::method]
                pub async fn on_scroll(
                    self: Arc<Self>,
                    top_gap: f64,
                    bottom_gap: f64,
                    scrolling_up: bool,
                ) -> Option<String> {
                    let manager = self.inner.lock().await;
                    let result = manager.on_scroll(top_gap, bottom_gap, scrolling_up).await;
                    result.map(|dir| format!("{:?}", dir))
                }

                /// Get the current loading state
                #[uniffi::method]
                pub fn is_loading(&self) -> bool {
                    let manager = self.inner.blocking_lock();
                    manager.is_loading().peek()
                }

                /// Get the current scroll mode
                #[uniffi::method]
                pub fn mode(&self) -> String {
                    let manager = self.inner.blocking_lock();
                    format!("{:?}", manager.mode())
                }

                /// Jump to live mode
                #[uniffi::method]
                pub async fn jump_to_live(self: Arc<Self>) {
                    let manager = self.inner.lock().await;
                    manager.jump_to_live().await;
                }

                /// Update the base filter predicate
                #[uniffi::method]
                pub async fn update_filter(self: Arc<Self>, predicate: String, reset_position: bool) {
                    let manager = self.inner.lock().await;
                    manager.update_filter(&predicate as &str, reset_position).await;
                }

                /// Update viewport height
                #[uniffi::method]
                pub fn set_viewport_height(&self, height: f64) {
                    let manager = self.inner.blocking_lock();
                    manager.set_viewport_height(height);
                }
            }
        }

        #[cfg(feature = "uniffi")]
        pub use __uniffi_scroll_manager::*;
    }
}
