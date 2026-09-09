//! Segmented button module.
//!
//! Provides [`SegmentedButton`], a pill-shaped button that hosts several icons
//! in a single row with an expand/collapse animation, and [`ExpandBehavior`]
//! to control whether expansion is triggered by click or hover. The underlying
//! GObject widget is [`SegmentedButtonWidget`].

mod widget;
pub use widget::{expose_widgets, ExpandBehavior, SegmentedButton};
