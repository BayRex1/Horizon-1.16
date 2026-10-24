//! Shared native paging controls for the browser and instance workspace.
use super::browser_controls::{LibraryView, Popup};
use super::*;
use horizon_launcher::pagination::{PageSize, Pagination};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Collection {
    Browser,
    Instance,
}

impl Launcher {
    fn content_paging(&self, collection: Collection) -> Pagination {
        match collection {
            Collection::Browser => self.mods.ui.paging,
            Collection::Instance => self.instance.content_list.paging,
        }
    }

    pub(super) fn content_view_button(
        &self,
        collection: Collection,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let popup = match collection {
            Collection::Browser => Popup::BrowserView,
            Collection::Instance => Popup::InstanceView,
        };
        let count = self.content_paging(collection).size.count();
        self.browser_menu_button(popup, format!("View: {count}"), cx)
    }

    pub(super) fn content_view_menu(&self, popup: Popup, cx: &mut Context<Self>) -> Div {
        let collection = if popup == Popup::InstanceView {
            Collection::Instance
        } else {
            Collection::Browser
        };
        let current = self.content_paging(collection).size;
        let t = self.theme;
        blocks::panel(&t)
            .flex()
            .flex_col()
            .p(px(6.))
            .child(
                text(theme::CAPTION)
                    .p(px(8.))
                    .text_color(t.muted)
                    .child("Projects per page"),
            )
            .children(PageSize::ALL.map(|size| {
                button(
                    format!("page-size-{}", size.count()),
                    format!("{} per page", size.count()),
                    Kind::Ghost,
                    &t,
                )
                .justify_start()
                .when(size == current, |button| button.bg(t.raised))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.set_content_page_size(collection, size);
                    this.close_browser_popup(window);
                    cx.notify();
                }))
            }))
    }

    pub(super) fn content_pager(
        &self,
        collection: Collection,
        total: Option<usize>,
        count: usize,
        has_next: bool,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let paging = self.content_paging(collection);
        let start = if count == 0 { 0 } else { paging.offset() + 1 };
        let end = if count == 0 {
            0
        } else {
            paging.offset() + count
        };
        let range = match total {
            Some(total) => format!("{start}–{end} of {total}"),
            None => format!("{start}–{end}"),
        };
        let page = match total {
            Some(total) => format!("Page {} of {}", paging.index + 1, paging.pages(total)),
            None => format!("Page {}", paging.index + 1),
        };
        let mut controls = div().flex().flex_wrap().items_center().gap(px(8.));
        for (id, label, index, enabled) in [
            ("first", "First", 0, paging.index > 0),
            (
                "previous",
                "Previous",
                paging.index.saturating_sub(1),
                paging.index > 0,
            ),
        ] {
            controls = controls.child(self.content_page_button(
                collection,
                id,
                label,
                index,
                enabled && !loading,
                cx,
            ));
        }
        controls = controls
            .child(
                text(theme::CAPTION)
                    .px(px(8.))
                    .text_color(t.ink_soft)
                    .child(page),
            )
            .child(self.content_page_button(
                collection,
                "next",
                "Next",
                paging.index + 1,
                has_next && !loading,
                cx,
            ));
        if let Some(total) = total {
            controls = controls.child(self.content_page_button(
                collection,
                "last",
                "Last",
                paging.pages(total) - 1,
                has_next && !loading,
                cx,
            ));
        }
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(text(theme::CAPTION).text_color(t.muted).child(range))
            .child(controls)
    }

    fn content_page_button(
        &self,
        collection: Collection,
        id: &str,
        label: &str,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let action = button(
            format!("content-page-{id}"),
            label.to_owned(),
            Kind::Ghost,
            &self.theme,
        );
        if !enabled {
            return disabled(action);
        }
        action.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.set_content_page(collection, index);
            cx.notify();
        }))
    }

    fn set_content_page_size(&mut self, collection: Collection, size: PageSize) {
        match collection {
            Collection::Browser => self.mods.ui.paging.set_size(size),
            Collection::Instance => self.instance.content_list.paging.set_size(size),
        }
        self.set_content_page(collection, 0);
    }

    fn set_content_page(&mut self, collection: Collection, index: usize) {
        match collection {
            Collection::Browser => {
                self.mods.ui.paging.index = index;
                self.mods.reset_lists();
                if self.mods.ui.view == LibraryView::Browse {
                    self.mods_search(self.mods.ui.paging.offset());
                }
            }
            Collection::Instance => {
                self.instance.content_list.paging.index = index;
                self.instance.content_list.rows.reset();
            }
        }
        self.content_scroll
            .handle
            .set_offset(gpui::point(px(0.), px(0.)));
    }
}
