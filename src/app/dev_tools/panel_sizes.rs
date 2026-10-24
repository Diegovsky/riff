//! Panel size overlay (debug builds only).
//!
//! A colored box over each panel with its size, and the window's in the center.

use gtk::cairo::{FontSlant, FontWeight};
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Panels to annotate, each a distinct RGB color. Names are widget ids from
/// `window.blp`.
const PANELS: [(&str, (f64, f64, f64)); 5] = [
    ("navigation_panel", (0.15, 0.70, 0.25)), // green: navigation panel column
    ("app_header", (0.90, 0.75, 0.10)),       // yellow: content header bar
    ("content_stack", (0.60, 0.25, 0.85)),    // purple: page stack
    ("playback", (0.95, 0.20, 0.20)),         // red: playback bar
    ("utility_panel", (0.20, 0.55, 0.95)),    // blue: utility panel
];

const CLASS_PANELS: [(&str, (f64, f64, f64)); 4] = [
    ("bottom-sheet-content", (0.10, 0.80, 0.85)), // cyan: bottom sheet content
    ("queue-page__selection-bar", (0.95, 0.45, 0.10)), // orange: bottom sheet header
    ("utility-panel__header", (0.55, 0.85, 0.20)), // lime: queue side panel header
    ("queue-page__content", (0.95, 0.30, 0.65)),  // pink: queue list (either place)
];

struct Target {
    widget: gtk::Widget,
    color: (f64, f64, f64),
    by_class: bool,
}

/// The widgets to annotate, in drawing order.
fn targets(builder: &gtk::Builder, root: &gtk::Widget) -> Vec<Target> {
    let mut targets: Vec<_> = PANELS
        .iter()
        .filter_map(|(name, color)| {
            Some(Target {
                widget: builder.object::<gtk::Widget>(*name)?,
                color: *color,
                by_class: false,
            })
        })
        .collect();
    for (class, color) in CLASS_PANELS.iter() {
        collect_with_class(root, class, *color, &mut targets);
    }
    targets
}

/// Mapped widgets under `widget` with `class`. Unmapped subtrees are skipped,
/// as nothing in them is on screen.
fn collect_with_class(
    widget: &gtk::Widget,
    class: &str,
    color: (f64, f64, f64),
    out: &mut Vec<Target>,
) {
    if !widget.is_mapped() {
        return;
    }
    if widget.has_css_class(class) {
        out.push(Target {
            widget: widget.clone(),
            color,
            by_class: true,
        });
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        collect_with_class(&c, class, color, out);
        child = c.next_sibling();
    }
}

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
    let window_for_draw = window.clone();
    let enabled_for_draw = Rc::clone(&enabled);
    area.set_draw_func(move |area, cr, _width, _height| {
        if !enabled_for_draw.get() {
            return;
        }

        let root = window_for_draw.upcast_ref::<gtk::Widget>();
        for (index, target) in targets(&builder_for_draw, root).iter().enumerate() {
            let (widget, color) = (&target.widget, target.color);
            // Skip panels not on screen, e.g. the navigation panel when collapsed.
            if !widget.is_mapped() {
                continue;
            }
            let Some(bounds) = widget.compute_bounds(area) else {
                continue;
            };

            // Inset more per nesting level so shared edges don't overlap.
            // Widgets found by class sit inside the panels: on their edges,
            // with the label in the bottom right, clear of the panels' labels
            let inset = if target.by_class {
                1.0
            } else {
                index as f64 * 1.5 + 1.0
            };
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

            let (label_x, label_y) = if target.by_class {
                (x + w - box_w, y + h - box_h)
            } else {
                (x, y)
            };

            // Solid label background so text stays legible.
            cr.set_source_rgba(color.0, color.1, color.2, 0.6);
            cr.rectangle(label_x, label_y, box_w, box_h);
            let _ = cr.fill();

            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(label_x + pad, label_y + pad + fe.ascent());
            let _ = cr.show_text(&text);
        }

        draw_window_size(area, cr);
    });

    // Repaint only when a panel size changes: hash all dimensions each frame
    // and redraw when the hash moves
    let builder_for_tick = builder.clone();
    let window_for_tick = window.clone();
    let enabled_for_tick = Rc::clone(&enabled);
    let last_signature: Cell<u64> = Cell::new(u64::MAX);
    area.add_tick_callback(move |area, _clock| {
        if enabled_for_tick.get() {
            let mut signature: u64 = 0;
            let root = window_for_tick.upcast_ref::<gtk::Widget>();
            for target in targets(&builder_for_tick, root) {
                let widget = &target.widget;
                signature = signature
                    .wrapping_mul(1_000_003)
                    .wrapping_add(widget.width() as u64);
                signature = signature
                    .wrapping_mul(1_000_003)
                    .wrapping_add(widget.height() as u64);
                // Widgets found by class come and go (e.g. the selection header)
                signature = signature.wrapping_mul(1_000_003).wrapping_add(1);
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

fn draw_window_size(area: &gtk::DrawingArea, cr: &gtk::cairo::Context) {
    let text = format!("Window  W: {}px   H: {}px", area.width(), area.height());
    cr.select_font_face("monospace", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(18.0);
    let (Ok(fe), Ok(te)) = (cr.font_extents(), cr.text_extents(&text)) else {
        return;
    };
    let pad = 8.0;
    let box_w = te.width() + pad * 2.0;
    let box_h = fe.height() + pad * 2.0;
    let x = (area.width() as f64 - box_w) / 2.0;
    let y = (area.height() as f64 - box_h) / 2.0;

    cr.set_source_rgba(0.0, 0.0, 0.0, 0.75);
    cr.rectangle(x, y, box_w, box_h);
    let _ = cr.fill();

    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(x + pad - te.x_bearing(), y + pad + fe.ascent());
    let _ = cr.show_text(&text);
}
