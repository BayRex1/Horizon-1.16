//! Content inventory for an instance, alongside its persistent launch controls.
use super::browser_controls::Popup;
use super::*;
use horizon_launcher::browser::installed;

pub(super) struct ContentItem {
    title: String,
    detail: String,
    version: String,
    file: String,
}

impl Launcher {
    pub(super) fn instance_content_snapshot(&self) -> Vec<ContentItem> {
        let mut items = installed::Installed::load(&self.instance.home)
            .entries
            .into_iter()
            .map(|entry| ContentItem {
                title: entry.title,
                detail: format!("{} · {}", entry.kind.label(), entry.source.label()),
                version: entry.number,
                file: entry.file,
            })
            .collect::<Vec<_>>();
        if let Some(pack) = self
            .instance
            .id
            .imported_slug()
            .and_then(|slug| self.packs.iter().find(|pack| &pack.slug == slug))
        {
            items.extend(pack.rows.iter().map(|row| ContentItem {
                title: if row.name.is_empty() {
                    row.key.clone()
                } else {
                    row.name.clone()
                },
                detail: format!(
                    "{} · {} · {} · {}",
                    row.class, row.tier, row.license, row.reason
                ),
                version: row.status.label().into(),
                file: format!("mods/{}/carry", row.key),
            }));
        }
        items
    }

    pub(super) fn instance_content(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let query = self.instance.query.to_lowercase();
        let mut items = self
            .instance
            .content
            .iter()
            .filter(|item| item.title.to_lowercase().contains(&query))
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.title.to_lowercase());
        let browse = button("instance-browse", "Browse content", Kind::Secondary, &theme).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                if this.set_mods_target(this.instance.home.clone(), this.instance.title.clone()) {
                    this.mods.ui.view = browser_controls::LibraryView::Browse;
                    this.mods.browse.open = None;
                    this.go(Page::Mods);
                }
                cx.notify();
            }),
        );
        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(self.instance_field(instance_page::Field::Search, cx))
            .child(browse);
        let mut rows = blocks::panel(&theme).flex().flex_col();
        for item in &items {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .p(px(14.))
                    .border_b_1()
                    .border_color(theme.bevel)
                    .child(blocks::slot(None, &item.title, 38., &theme))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(4.))
                            .child(
                                blocks::pixel(16., FontWeight::MEDIUM)
                                    .text_color(theme.ink)
                                    .child(item.title.clone()),
                            )
                            .child(
                                text(theme::CAPTION)
                                    .text_color(theme.muted)
                                    .child(item.detail.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(160.))
                            .gap(px(4.))
                            .child(text(theme::CAPTION).child(item.version.clone()))
                            .child(
                                text(theme::MICRO)
                                    .text_color(theme.muted)
                                    .child(item.file.clone()),
                            ),
                    ),
            );
        }
        if items.is_empty() {
            rows = rows.child(centered(
                "No content matches. Browse to add projects, or change the search.",
                &theme,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(toolbar)
            .child(
                text(theme::CAPTION)
                    .text_color(theme.muted)
                    .child(format!("{} projects", items.len())),
            )
            .child(rows)
    }
}

impl Launcher {
    pub(super) fn instance_menu(&self, popup: Popup, height: f32, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut menu = div()
            .id("instance-menu-body")
            .flex()
            .flex_col()
            .gap(px(2.))
            .max_h(px((height - 12.).max(30.)))
            .overflow_y_scroll();
        match popup {
            Popup::InstanceBuild => {
                for tag in &self.installed {
                    let tag = tag.clone();
                    menu = menu.child(
                        button(
                            format!("instance-build-{tag}"),
                            tag.clone(),
                            Kind::Ghost,
                            &t,
                        )
                        .justify_start()
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, window, cx| {
                                let mut preferences = this.instance.preferences.clone();
                                preferences.build = Some(tag.clone());
                                match preferences.save(&this.instance.home) {
                                    Ok(()) => {
                                        this.instance.preferences = preferences;
                                        if this.instance.id == InstanceId::Horizon {
                                            this.settings.selected = Some(tag.clone());
                                            this.settings.save();
                                        }
                                    }
                                    Err(error) => this.instance.message = Some(error),
                                }
                                this.close_browser_popup(window);
                                cx.notify();
                            },
                        )),
                    );
                }
            }
            Popup::InstanceMore => {
                let home = self.instance.home.clone();
                menu = menu
                    .child(
                        button(
                            "instance-menu-folder",
                            "Open instance folder",
                            Kind::Ghost,
                            &t,
                        )
                        .justify_start()
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, window, cx| {
                                this.close_browser_popup(window);
                                cx.open_with_system(&home);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        button("instance-menu-refresh", "Refresh content", Kind::Ghost, &t)
                            .justify_start()
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.close_browser_popup(window);
                                this.reload_instance();
                                this.set_mods_target(
                                    this.instance.home.clone(),
                                    this.instance.title.clone(),
                                );
                                cx.notify();
                            })),
                    );
            }
            _ => {}
        }
        blocks::panel(&t).p(px(6.)).child(menu)
    }
}
