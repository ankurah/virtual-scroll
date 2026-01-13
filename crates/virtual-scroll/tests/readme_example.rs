//! Example code for README documentation
//!
//! This file provides compile-checked examples for the README via liaison transclusion.
//! The function is not actually run as a test but is validated by `cargo test --workspace`.

mod common;

use ankurah::model::View;
use ankurah_signals::Get;
use ankurah_virtual_scroll::ScrollManager;
use common::{TestMessageView, durable_sled_setup};

// liaison id=rust-usage
/// Example: Creating and using a ScrollManager
#[allow(dead_code)]
async fn scroll_manager_example() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = durable_sled_setup().await?;

    // Create scroll manager with full configuration
    let scroll_manager = ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",              // Filter predicate (e.g., "room = 'general'")
        "timestamp DESC",    // Display order
        40,                  // Minimum row height (pixels)
        2.0,                 // Buffer factor (2.0 = 2x viewport)
        600,                 // Viewport height (pixels)
    )?;

    // Initialize (runs initial query)
    scroll_manager.start().await;

    // Read visible items from the signal
    let visible_set = scroll_manager.visible_set().get();
    for item in &visible_set.items {
        // Access item fields via the View trait
        let _id = item.entity().id();
    }

    // Notify on scroll events with first/last visible EntityIds
    if let (Some(first), Some(last)) = (visible_set.items.first(), visible_set.items.last()) {
        let first_visible_id = first.entity().id();
        let last_visible_id = last.entity().id();
        let scrolling_backward = true; // true = scrolling toward older items

        scroll_manager.on_scroll(first_visible_id, last_visible_id, scrolling_backward);
    }

    Ok(())
}
// liaison end
