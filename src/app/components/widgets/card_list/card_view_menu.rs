use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::cell::Cell;
use std::rc::Rc;

use super::widget::CardList;
use crate::app::components::{CardLayout, CardSize, SortOrder};
use crate::app::{ActionDispatcher, BrowserAction};

// GObject subclass for the popover template

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(file = "src/app/components/widgets/card_list/card_view_menu.blp")]
    pub struct CardViewMenuPopover {
        #[template_child]
        pub decrease_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub increase_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub sort_box: TemplateChild<gtk::Box>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardViewMenuPopover {
        const NAME: &'static str = "CardViewMenuPopover";
        type Type = super::CardViewMenuPopoverWidget;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CardViewMenuPopover {}
    impl WidgetImpl for CardViewMenuPopover {}
    impl PopoverImpl for CardViewMenuPopover {}
}

glib::wrapper! {
    pub struct CardViewMenuPopoverWidget(ObjectSubclass<imp::CardViewMenuPopover>)
        @extends gtk::Widget, gtk::Popover;
}

impl CardViewMenuPopoverWidget {
    fn new() -> Self {
        glib::Object::new()
    }
}

// Public API

/// Returns the sort to actually apply: the user's preferred sort if it's
/// available for this page, otherwise the first available sort.
pub(super) fn effective_sort(preferred: SortOrder, available: &[SortOrder]) -> SortOrder {
    if available.contains(&preferred) {
        preferred
    } else {
        available.first().copied().unwrap_or(SortOrder::RecentlyAdded)
    }
}

fn icon_for_layout(layout: CardLayout) -> &'static str {
    match layout {
        CardLayout::Vertical => "view-grid-symbolic",
        CardLayout::ImageOnly => "view-app-grid-symbolic",
        CardLayout::Horizontal => "view-list-symbolic",
    }
}

/// A Nautilus-style split button: clicking cycles the card layout,
/// the dropdown arrow opens a popover with icon size controls and sort options.
pub struct CardViewMenu {
    pub split_button: libadwaita::SplitButton,
}

impl CardViewMenu {
    pub fn new(
        page_id: String,
        available_sorts: &[SortOrder],
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        current_sort: Rc<Cell<SortOrder>>,
        card_list: Rc<CardList>,
        dispatcher: Rc<dyn ActionDispatcher>,
    ) -> Self {
        let popover = CardViewMenuPopoverWidget::new();
        let imp = popover.imp();

        // Wire size buttons
        Self::connect_size_buttons(
            &imp.decrease_btn,
            &imp.increase_btn,
            size.get(),
            Rc::clone(&size),
            Rc::clone(&card_list),
            Rc::clone(&dispatcher),
        );

        // Wire sort radio buttons into sort_box
        Self::populate_sort_section(
            &imp.sort_box,
            &page_id,
            available_sorts,
            current_sort.get(),
            current_sort,
            Rc::clone(&card_list),
            Rc::clone(&dispatcher),
        );

        // Sync button sensitivity when popover opens
        let size_ref = Rc::clone(&size);
        let dec = imp.decrease_btn.clone();
        let inc = imp.increase_btn.clone();
        popover.connect_show(move |_| {
            let s = size_ref.get();
            dec.set_sensitive(s != CardSize::Small);
            inc.set_sensitive(s != CardSize::Large);
        });

        let split_button = libadwaita::SplitButton::new();
        split_button.set_icon_name(icon_for_layout(layout.get()));
        split_button.set_popover(Some(&popover));

        let layout_ref = Rc::clone(&layout);
        let card_list_ref = Rc::clone(&card_list);
        let dispatch = Rc::clone(&dispatcher);
        split_button.connect_clicked(move |btn| {
            let next = layout_ref.get().next();
            layout_ref.set(next);
            btn.set_icon_name(icon_for_layout(next));
            card_list_ref.update_layout(next);
            dispatch.dispatch(BrowserAction::ChangeCardLayout(next).into());
        });

        Self { split_button }
    }

    pub fn widget(&self) -> &libadwaita::SplitButton {
        &self.split_button
    }

    pub fn sync(&self, layout: CardLayout) {
        self.split_button.set_icon_name(icon_for_layout(layout));
    }

    fn connect_size_buttons(
        decrease_btn: &gtk::Button,
        increase_btn: &gtk::Button,
        current_size: CardSize,
        size: Rc<Cell<CardSize>>,
        card_list: Rc<CardList>,
        dispatcher: Rc<dyn ActionDispatcher>,
    ) {
        decrease_btn.set_sensitive(current_size != CardSize::Small);
        increase_btn.set_sensitive(current_size != CardSize::Large);

        let size_ref = Rc::clone(&size);
        let card_list_ref = Rc::clone(&card_list);
        let inc_btn = increase_btn.clone();
        let dispatch = Rc::clone(&dispatcher);
        decrease_btn.connect_clicked(move |btn| {
            let new_size = size_ref.get().decrease();
            size_ref.set(new_size);
            card_list_ref.update_size(new_size);
            btn.set_sensitive(new_size != CardSize::Small);
            inc_btn.set_sensitive(true);
            dispatch.dispatch(BrowserAction::ChangeCardSize(new_size).into());
        });

        let size_ref = size;
        let card_list_ref = card_list;
        let dec_btn = decrease_btn.clone();
        increase_btn.connect_clicked(move |btn| {
            let new_size = size_ref.get().increase();
            size_ref.set(new_size);
            card_list_ref.update_size(new_size);
            btn.set_sensitive(new_size != CardSize::Large);
            dec_btn.set_sensitive(true);
            dispatcher.dispatch(BrowserAction::ChangeCardSize(new_size).into());
        });
    }

    fn populate_sort_section(
        sort_box: &gtk::Box,
        page_id: &str,
        available_sorts: &[SortOrder],
        current_sort: SortOrder,
        sort: Rc<Cell<SortOrder>>,
        card_list: Rc<CardList>,
        dispatcher: Rc<dyn ActionDispatcher>,
    ) {
        let all_sort_options = [
            SortOrder::RecentlyAdded,
            SortOrder::Alphabetic,
            SortOrder::Creator,
            SortOrder::DateReleased,
            SortOrder::Popularity,
        ];

        let mut first_btn: Option<gtk::CheckButton> = None;
        for order in all_sort_options {
            if !available_sorts.contains(&order) {
                continue;
            }
            let btn = gtk::CheckButton::with_label(&order.label());
            if let Some(ref group) = first_btn {
                btn.set_group(Some(group));
            } else {
                first_btn = Some(btn.clone());
            }
            btn.set_active(order == current_sort);

            let page = page_id.to_string();
            let card_list_ref = Rc::clone(&card_list);
            let sort_ref = Rc::clone(&sort);
            let dispatch = Rc::clone(&dispatcher);
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    sort_ref.set(order);
                    card_list_ref.set_sort(order);
                    dispatch.dispatch(BrowserAction::ChangeSortOrder(page.clone(), order).into());
                }
            });

            sort_box.append(&btn);
        }
    }
}
