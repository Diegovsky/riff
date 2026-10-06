use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::cell::Cell;
use std::rc::Rc;

use super::widget::CardList;
use crate::app::components::{CardLayout, CardSize, SortOrder};
use crate::app::models::FilterOption;
use crate::app::{BrowserAction, Dispatcher};

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
        pub sort_section: TemplateChild<gtk::Box>,
        #[template_child]
        pub sort_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub filter_section: TemplateChild<gtk::Box>,
        #[template_child]
        pub filter_box: TemplateChild<gtk::Box>,
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
        @extends gtk::Widget, gtk::Popover,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::ShortcutManager;
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
        available
            .first()
            .copied()
            .unwrap_or(SortOrder::RecentlyAdded)
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
/// the dropdown arrow opens a popover with icon size controls, sort options
/// and (optionally) filter options.
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
        filters: &[FilterOption],
        on_filter_changed: impl Fn(&str, usize) + 'static,
        card_list: Rc<CardList>,
        dispatcher: Dispatcher,
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
            dispatcher.clone(),
        );

        // Wire sort radio buttons into sort_box
        Self::populate_sort_section(
            &imp.sort_box,
            &page_id,
            available_sorts,
            current_sort.get(),
            current_sort,
            Rc::clone(&card_list),
            dispatcher.clone(),
        );

        // Hide the entire sort section if no sort options are available.
        imp.sort_section.set_visible(!available_sorts.is_empty());

        Self::populate_filter_section(
            &imp.filter_box,
            filters,
            on_filter_changed,
            Rc::clone(&card_list),
        );
        imp.filter_section.set_visible(!filters.is_empty());

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
        let dispatch = dispatcher.clone();
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
        dispatcher: Dispatcher,
    ) {
        decrease_btn.set_sensitive(current_size != CardSize::Small);
        increase_btn.set_sensitive(current_size != CardSize::Large);

        let size_ref = Rc::clone(&size);
        let card_list_ref = Rc::clone(&card_list);
        let inc_btn = increase_btn.clone();
        let dispatch = dispatcher.clone();
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
        dispatcher: Dispatcher,
    ) {
        let all_sort_options = [
            SortOrder::RecentlyAdded,
            SortOrder::Alphabetic,
            SortOrder::Creator,
            SortOrder::DateReleased,
            SortOrder::Popularity,
        ];
        let orders: Vec<SortOrder> = all_sort_options
            .iter()
            .copied()
            .filter(|order| available_sorts.contains(order))
            .collect();

        let page = page_id.to_string();
        append_radio_group(
            sort_box,
            orders
                .iter()
                .map(|order| (order.label(), *order == current_sort))
                .collect(),
            move |i| {
                let order = orders[i];
                sort.set(order);
                card_list.set_sort(order);
                dispatcher.dispatch(BrowserAction::ChangeSortOrder(page.clone(), order).into());
            },
        );
    }

    fn populate_filter_section(
        filter_box: &gtk::Box,
        filters: &[FilterOption],
        on_filter_changed: impl Fn(&str, usize) + 'static,
        card_list: Rc<CardList>,
    ) {
        let categories: Vec<String> = filters.iter().map(|f| f.category.clone()).collect();
        append_radio_group(
            filter_box,
            filters
                .iter()
                .enumerate()
                .map(|(i, option)| (option.label.clone(), i == 0))
                .collect(),
            move |i| {
                card_list.set_filter(&categories[i]);
                on_filter_changed(&categories[i], card_list.visible_count());
            },
        );
    }
}

fn append_radio_group(
    container: &gtk::Box,
    options: Vec<(String, bool)>,
    on_select: impl Fn(usize) + 'static,
) {
    let on_select = Rc::new(on_select);
    let mut group: Option<gtk::CheckButton> = None;
    for (i, (label, active)) in options.into_iter().enumerate() {
        let btn = gtk::CheckButton::with_label(&label);
        btn.set_group(group.as_ref());
        group.get_or_insert_with(|| btn.clone());
        btn.set_active(active);

        let on_select = Rc::clone(&on_select);
        btn.connect_toggled(move |b| {
            if b.is_active() {
                on_select(i);
            }
        });
        container.append(&btn);
    }
}
