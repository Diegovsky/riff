//! Panel size overlay (debug builds only).
//!
//! Floats a colored box over each major panel, labelled with its pixel size.

use gtk::cairo::{FontSlant, FontWeight};
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Panels to annotate, each a distinct RGB color. Names are widget ids from
/// `window.blp`.
const PANELS: [(&str, (f64, f64, f64)); 4] = [
    ("sidebar_panel", (0.15, 0.70, 0.25)), // green: sidebar column
    ("app_header", (0.90, 0.75, 0.10)),    // yellow: content header bar
    ("navigation_stack", (0.60, 0.25, 0.85)), // purple: page stack
    ("playback", (0.95, 0.20, 0.20)),      // red: playback bar
];

/// Set up the overlay and bind it to the "Panel Sizes" `switch`.
pub fn wire(builder: &gtk::Builder, switch: &gtk::Switch) {
    let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();

    // Reparent the content under an overlay so a transparent drawing area can
    // sit on top without affecting layout.
    let content = window.content();
    window.set_content(None::<&gtk::Widget>);

    let overlay = gtk::Overlay::new();
    if let Some(child) = &content {
        overlay.set_child(Some(child));
    }

    let area = gtk::DrawingArea::new();
    area.set_can_target(false); // never intercept pointer input
    area.set_can_focus(false);
    area.set_visible(false);
    overlay.add_overlay(&area);

    window.set_content(Some(&overlay));

    let enabled = Rc::new(Cell::new(false));

    let builder_for_draw = builder.clone();
    let enabled_for_draw = Rc::clone(&enabled);
    area.set_draw_func(move |area, cr, _width, _height| {
        if !enabled_for_draw.get() {
            return;
        }

        for (index, (name, color)) in PANELS.iter().enumerate() {
            let Some(widget) = builder_for_draw.object::<gtk::Widget>(*name) else {
                continue;
            };
            // Skip panels not on screen, e.g. the sidebar when collapsed.
            if !widget.is_mapped() {
                continue;
            }
            let Some(bounds) = widget.compute_bounds(area) else {
                continue;
            };

            // Inset more per nesting level so shared edges don't overlap.
            let inset = index as f64 * 1.5 + 1.0;
            let x = bounds.x() as f64 + inset;
            let y = bounds.y() as f64 + inset;
            let w = (bounds.width() as f64 - inset * 2.0).max(0.0);
            let h = (bounds.height() as f64 - inset * 2.0).max(0.0);

            // Bounding box.
            cr.set_source_rgba(color.0, color.1, color.2, 0.6);
            cr.set_line_width(2.0);
            cr.rectangle(x, y, w, h);
            let _ = cr.stroke();

            // Size readout in application pixels.
            let text = format!("W: {}px   H: {}px", widget.width(), widget.height());
            cr.select_font_face("monospace", FontSlant::Normal, FontWeight::Bold);
            cr.set_font_size(12.0);
            let Ok(fe) = cr.font_extents() else {
                continue;
            };
            let Ok(te) = cr.text_extents(&text) else {
                continue;
            };
            let pad = 4.0;
            let box_w = te.width() + pad * 2.0;
            let box_h = fe.height() + pad * 2.0;

            // Solid label background so text stays legible.
            cr.set_source_rgba(color.0, color.1, color.2, 0.6);
            cr.rectangle(x, y, box_w, box_h);
            let _ = cr.fill();

            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(x + pad, y + pad + fe.ascent());
            let _ = cr.show_text(&text);
        }
    });

    // Repaint only when a panel size changes: hash all dimensions each frame
    // and redraw when the hash moves.
    let builder_for_tick = builder.clone();
    let enabled_for_tick = Rc::clone(&enabled);
    let last_signature: Cell<u64> = Cell::new(u64::MAX);
    area.add_tick_callback(move |area, _clock| {
        if enabled_for_tick.get() {
            let mut signature: u64 = 0;
            for (name, _) in PANELS.iter() {
                if let Some(widget) = builder_for_tick.object::<gtk::Widget>(*name) {
                    signature = signature
                        .wrapping_mul(1_000_003)
                        .wrapping_add(widget.width() as u64);
                    signature = signature
                        .wrapping_mul(1_000_003)
                        .wrapping_add(widget.height() as u64);
                }
            }
            if signature != last_signature.get() {
                last_signature.set(signature);
                area.queue_draw();
            }
        }
        glib::ControlFlow::Continue
    });

    switch.connect_state_set(move |_, active| {
        enabled.set(active);
        area.set_visible(active);
        area.queue_draw();
        glib::Propagation::Proceed
    });
}
