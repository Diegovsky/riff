use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::BottomSheetContent;
use crate::app::components::{QueueBarWidget, QueuePageWidget};

/// Inside the scroller, so the scrollbar stays at the sheet's edge
const SHEET_MARGIN_X: i32 = 12;

/// About as long as the sheet takes to open
const HOLD_AT_TOP_MICROS: i64 = 500_000;

#[derive(Clone)]
pub struct BottomSheet {
    sheet: libadwaita::BottomSheet,
    queue_bar: QueueBarWidget,
    content: BottomSheetContent,
    has_track: Rc<Cell<bool>>,
}

impl BottomSheet {
    pub fn new(sheet: libadwaita::BottomSheet, queue_bar: QueueBarWidget, has_track: bool) -> Self {
        let content = BottomSheetContent::default();
        sheet.connect_open_notify(clone!(
            #[weak]
            content,
            move |sheet| {
                if let Some(page) = content.child().and_downcast::<QueuePageWidget>() {
                    if sheet.is_open() {
                        hold_at_top(&page);
                    }
                }
            }
        ));
        let bottom_sheet = Self {
            sheet,
            queue_bar,
            content,
            has_track: Rc::new(Cell::new(has_track)),
        };
        bottom_sheet.sync_bottom_bar();
        bottom_sheet
    }

    pub fn set_page(&self, page: Option<&QueuePageWidget>) {
        if let Some(old) = self.content.child().and_downcast::<QueuePageWidget>() {
            old.set_extra_margin_x(0);
            old.set_fit_content_height(false);
        }
        if let Some(page) = page {
            page.set_extra_margin_x(SHEET_MARGIN_X);
            page.set_fit_content_height(true);
        }
        self.content.set_child(page);
        self.sheet
            .set_sheet(page.is_some().then_some(&self.content));
        self.sync_bottom_bar();
    }

    pub fn set_has_track(&self, has_track: bool) {
        self.has_track.set(has_track);
        self.sync_bottom_bar();
    }

    pub fn is_open(&self) -> bool {
        self.sheet.is_open()
    }

    pub fn set_open(&self, open: bool) {
        self.sheet.set_open(open);
    }

    pub fn connect_open_notify<F: Fn() + 'static>(&self, f: F) {
        self.sheet.connect_open_notify(move |_| f());
    }

    fn sync_bottom_bar(&self) {
        let has_page = self.content.child().is_some();
        self.queue_bar.set_show_track(!has_page);
        self.sheet.set_can_open(has_page);
        let shown = has_page || self.has_track.get();
        self.sheet.set_bottom_bar(shown.then_some(&self.queue_bar));
    }
}

fn hold_at_top(page: &QueuePageWidget) {
    page.scroll_to_top();
    let start = Cell::new(None);
    page.add_tick_callback(move |page, clock| {
        page.scroll_to_top();
        let now = clock.frame_time();
        let elapsed = now - start.get().unwrap_or(now);
        start.set(start.get().or(Some(now)));
        if elapsed < HOLD_AT_TOP_MICROS {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}
