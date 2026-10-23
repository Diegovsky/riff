use gtk::prelude::*;

pub fn hide_pages_not_showing(stack: &gtk::Stack) {
    let update = |stack: &gtk::Stack| {
        let showing = stack.visible_child();
        let mut child = stack.first_child();
        while let Some(page) = child {
            child = page.next_sibling();
            page.set_visible(Some(&page) == showing.as_ref());
        }
    };
    update(stack);
    stack.connect_visible_child_notify(update);
}

pub fn show_page(stack: &gtk::Stack, name: &str) {
    if let Some(page) = stack.child_by_name(name) {
        page.set_visible(true);
    }
    stack.set_visible_child_name(name);
}
