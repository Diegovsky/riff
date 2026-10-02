//! Reordering a track list's rows by drag and drop. Mouse and pen only: on a
//! touchscreen, dragging the list scrolls it.

use gtk::graphene::Point;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::{TrackListModel, TrackRow};
use crate::app::components::utils::ancestor;

const AUTOSCROLL_EDGE_PX: f64 = 48.0;
const AUTOSCROLL_MAX_STEP_PX: f64 = 14.0;

#[derive(Default)]
struct DragState {
    key: RefCell<Option<String>>,
    indicator: RefCell<Option<glib::WeakRef<TrackRow>>>,
    scroll_step: Cell<f64>,
    scroll_tick: RefCell<Option<gtk::TickCallbackId>>,
}

impl DragState {
    fn show_indicator(&self, row: Option<(&TrackRow, bool)>) {
        let old = self.indicator.take().and_then(|r| r.upgrade());
        if let Some(old) = old {
            old.set_drop_indicator(None);
        }
        if let Some((row, after)) = row {
            row.set_drop_indicator(Some(after));
            self.indicator.replace(Some(row.downgrade()));
        }
    }

    fn stop_scrolling(&self) {
        self.scroll_step.set(0.0);
        if let Some(tick) = self.scroll_tick.take() {
            tick.remove();
        }
    }

    fn end(&self) {
        self.key.replace(None);
        self.show_indicator(None);
        self.stop_scrolling();
    }
}

fn row_at(listview: &gtk::ListView, x: f64, y: f64) -> Option<(TrackRow, bool)> {
    let picked = listview.pick(x, y, gtk::PickFlags::DEFAULT)?;
    let row = picked
        .clone()
        .downcast::<TrackRow>()
        .ok()
        .or_else(|| ancestor::<_, TrackRow>(&picked))?;
    let point = listview.compute_point(&row, &Point::new(x as f32, y as f32))?;
    let after = f64::from(point.y()) > f64::from(row.height()) / 2.0;
    Some((row, after))
}

fn line_position(listview: &gtk::ListView, x: f64, row: TrackRow, after: bool) -> (TrackRow, bool) {
    if !after {
        return (row, after);
    }
    let bottom = row.compute_point(listview, &Point::new(0.0, row.height() as f32));
    let below = bottom.and_then(|p| row_at(listview, x, f64::from(p.y()) + 1.0));
    match below {
        Some((next, _)) if next != row => (next, false),
        _ => (row, after),
    }
}

fn is_touch(source: &gtk::DragSource) -> bool {
    source
        .current_event_device()
        .is_some_and(|d| d.source() == gdk::InputSource::Touchscreen)
}

pub fn connect<Model: TrackListModel + 'static>(
    listview: &gtk::ListView,
    model: &Rc<Model>,
) -> Rc<Reorder<Model>> {
    let state = Rc::new(DragState::default());

    let target = gtk::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
    let drop_at = clone!(
        #[weak]
        listview,
        #[weak]
        model,
        #[weak]
        state,
        #[upgrade_or]
        None,
        move |x: f64, y: f64| {
            let key = state.key.borrow().clone()?;
            let (row, after) = row_at(&listview, x, y)?;
            let target = row.row_key()?;
            model
                .can_drop_row(&key, &target, after)
                .then_some((key, row, target, after))
        }
    );
    let drop_at = Rc::new(drop_at);
    target.connect_motion(clone!(
        #[weak]
        listview,
        #[weak]
        state,
        #[strong]
        drop_at,
        #[upgrade_or]
        gdk::DragAction::empty(),
        move |_, x, y| {
            autoscroll(&listview, &state, y);
            match drop_at(x, y) {
                Some((_, row, _, after)) => {
                    let (row, after) = line_position(&listview, x, row, after);
                    state.show_indicator(Some((&row, after)));
                    gdk::DragAction::MOVE
                }
                None => {
                    state.show_indicator(None);
                    gdk::DragAction::empty()
                }
            }
        }
    ));
    target.connect_leave(clone!(
        #[weak]
        state,
        move |_| {
            state.show_indicator(None);
            state.stop_scrolling();
        }
    ));
    target.connect_drop(clone!(
        #[weak]
        model,
        #[weak]
        state,
        #[strong]
        drop_at,
        #[upgrade_or]
        false,
        move |_, _, x, y| {
            let dropped = drop_at(x, y);
            state.end();
            match dropped {
                Some((key, _, target, after)) => {
                    model.drop_row(&key, &target, after);
                    true
                }
                None => false,
            }
        }
    ));
    listview.add_controller(target);

    Rc::new(Reorder {
        model: Rc::downgrade(model),
        state,
    })
}

pub struct Reorder<Model> {
    model: Weak<Model>,
    state: Rc<DragState>,
}

impl<Model: TrackListModel + 'static> Reorder<Model> {
    pub fn attach(self: &Rc<Self>, row: &TrackRow) {
        if !row.mark_draggable() {
            return;
        }
        let source = gtk::DragSource::new();
        source.set_actions(gdk::DragAction::MOVE);
        let reorder = Rc::downgrade(self);
        source.connect_prepare(clone!(
            #[weak]
            row,
            #[upgrade_or]
            None,
            move |source, x, y| reorder.upgrade()?.prepare(source, &row, x, y)
        ));
        source.connect_drag_end(clone!(
            #[weak(rename_to = state)]
            self.state,
            move |_, _, _| state.end()
        ));
        row.add_controller(source);
    }

    fn prepare(
        &self,
        source: &gtk::DragSource,
        row: &TrackRow,
        x: f64,
        y: f64,
    ) -> Option<gdk::ContentProvider> {
        if is_touch(source) {
            debug!("reorder: not from a touchscreen");
            return None;
        }
        let model = self.model.upgrade()?;
        let key = row.row_key()?;
        if !model.can_drag_row(&key) {
            debug!("reorder: row {key} can't be dragged");
            return None;
        }
        debug!("reorder: dragging row {key}");
        source.set_icon(
            Some(&gtk::WidgetPaintable::new(Some(row))),
            x as i32,
            y as i32,
        );
        self.state.key.replace(Some(key.clone()));
        Some(gdk::ContentProvider::for_value(&key.to_value()))
    }
}

fn autoscroll(listview: &gtk::ListView, state: &Rc<DragState>, y: f64) {
    let Some(scrolled) = ancestor::<_, gtk::ScrolledWindow>(listview) else {
        return;
    };
    let Some(point) = listview.compute_point(&scrolled, &Point::new(0.0, y as f32)) else {
        return;
    };
    let y = f64::from(point.y());
    let height = f64::from(scrolled.height());
    let depth = if y < AUTOSCROLL_EDGE_PX {
        y - AUTOSCROLL_EDGE_PX
    } else if y > height - AUTOSCROLL_EDGE_PX {
        y - (height - AUTOSCROLL_EDGE_PX)
    } else {
        0.0
    };
    let step = (depth / AUTOSCROLL_EDGE_PX).clamp(-1.0, 1.0) * AUTOSCROLL_MAX_STEP_PX;
    state.scroll_step.set(step);
    if step == 0.0 {
        state.stop_scrolling();
        return;
    }
    if state.scroll_tick.borrow().is_some() {
        return;
    }
    let adj = scrolled.vadjustment();
    let tick = listview.add_tick_callback(clone!(
        #[weak]
        state,
        #[upgrade_or]
        glib::ControlFlow::Break,
        move |_, _| {
            let max = adj.upper() - adj.page_size();
            adj.set_value((adj.value() + state.scroll_step.get()).clamp(0.0, max));
            glib::ControlFlow::Continue
        }
    ));
    state.scroll_tick.replace(Some(tick));
}
