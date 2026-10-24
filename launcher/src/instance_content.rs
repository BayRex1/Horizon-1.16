//! Real content rows for the Content Workspace, including the imported pack's coverage report.
use super::browser_controls::Popup;
use super::*;
use horizon_launcher::{
    browser::{installed, Kind as ContentKind},
    instance_profile,
};
use std::path::PathBuf;

#[derive(Clone)]
pub(super) enum Origin {
    Managed(Box<installed::Entry>),
    Ported { key: String, path: PathBuf },
    LocalPack { name: String, path: PathBuf },
}
#[derive(Clone)]
pub(super) struct ContentItem {
    key: String,
    title: String,
    detail: String,
    version: String,
    file: String,
    kind: ContentKind,
    enabled: Option<bool>,
    origin: Origin,
}

impl Launcher {
    pub(super) fn instance_content_snapshot(&self) -> Vec<ContentItem> {
        let mut items = installed::Installed::load(&self.instance.home)
            .entries
            .into_iter()
            .map(|entry| {
                let selection = instance_profile::enabled(&self.instance.home, &entry);
                let detail = match &selection {
                    Ok(_) => instance_profile::taken_down(&entry).map_or_else(
                        || format!("{} · {}", entry.kind.label(), entry.source.label()),
                        |reason| format!("Taken down: {reason}"),
                    ),
                    Err(error) => error.clone(),
                };
                ContentItem {
                    key: format!("{}:{}", entry.source.label(), entry.project),
                    title: entry.title.clone(),
                    detail,
                    version: entry.number.clone(),
                    file: entry.file.clone(),
                    kind: entry.kind,
                    enabled: selection.ok(),
                    origin: Origin::Managed(Box::new(entry)),
                }
            })
            .collect::<Vec<_>>();
        if let Ok(entries) = std::fs::read_dir(self.instance.home.join("resourcepacks")) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let relative = format!("resourcepacks/{name}");
                if !path.is_file()
                    || path
                        .extension()
                        .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
                    || items.iter().any(|item| item.file == relative)
                {
                    continue;
                }
                let selection = instance_profile::resource_pack_enabled(&self.instance.home, &name);
                let detail = selection
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "Resource pack · Local file".into());
                items.push(ContentItem {
                    key: format!("local:{name}"),
                    title: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    detail,
                    version: "Local file".into(),
                    file: relative,
                    kind: ContentKind::ResourcePack,
                    enabled: selection.ok(),
                    origin: Origin::LocalPack { name, path },
                });
            }
        }
        if let Some(pack) = self
            .instance
            .id
            .imported_slug()
            .and_then(|slug| self.packs.iter().find(|pack| &pack.slug == slug))
        {
            for row in &pack.rows {
                let path = pack.work.join("mods").join(&row.key).join("carry");
                let available = pack.mods.contains(&path);
                items.push(ContentItem {
                    key: format!("ported:{}", row.key),
                    title: if row.name.is_empty() {
                        row.key.clone()
                    } else {
                        row.name.clone()
                    },
                    detail: format!(
                        "{} · {} · {} · {} · {}",
                        row.status.label(),
                        row.class,
                        row.tier,
                        row.license,
                        row.reason
                    ),
                    version: row.status.label().into(),
                    file: format!("mods/{}/carry", row.key),
                    kind: ContentKind::Mod,
                    enabled: available
                        .then(|| !self.instance.preferences.disabled_ported.contains(&row.key)),
                    origin: Origin::Ported {
                        key: row.key.clone(),
                        path,
                    },
                });
            }
        }
        items
    }

    fn instance_item_has_update(&self, item: &ContentItem) -> bool {
        match &item.origin {
            Origin::Managed(entry) => {
                self.mods.home == self.instance.home
                    && self
                        .mods
                        .updates
                        .contains_key(&(entry.source, entry.project.clone()))
            }
            _ => false,
        }
    }

    pub(super) fn instance_content(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut items = self
            .instance
            .content
            .iter()
            .filter(|item| {
                self.instance.kind.is_none_or(|kind| item.kind == kind)
                    && item
                        .title
                        .to_lowercase()
                        .contains(&self.instance.query.to_lowercase())
            })
            .collect::<Vec<_>>();
        items.sort_by(|first, second| {
            if self.instance.updates_first {
                let order = self
                    .instance_item_has_update(second)
                    .cmp(&self.instance_item_has_update(first));
                if !order.is_eq() {
                    return order;
                }
            }
            first.title.to_lowercase().cmp(&second.title.to_lowercase())
        });
        let count = items.len();
        let mut page = div().flex().flex_col().gap(px(14.));
        if self.instance.id.imported_slug().is_some() {
            page=page.child(text(theme::CAPTION).text_color(t.muted).child("Imported for Horizon. Supported content is carried over; unsupported Java behaviour stays off.")).child(self.instance_port_controls(cx));
        }
        page = page.child(self.instance_content_toolbar(cx)).child(
            text(theme::CAPTION)
                .text_color(t.muted)
                .child(format!("{count} projects · changes apply on next launch")),
        );
        if !self.instance.selected.is_empty() {
            page = page.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        text(theme::CAPTION)
                            .child(format!("{} selected", self.instance.selected.len())),
                    )
                    .child(
                        button("instance-enable-selected", "Enable", Kind::Ghost, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.instance_bulk_enabled(true);
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button("instance-disable-selected", "Disable", Kind::Ghost, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.instance_bulk_enabled(false);
                                cx.notify();
                            }),
                        ),
                    ),
            );
        }
        let header = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(14.))
            .py(px(10.))
            .border_b_1()
            .border_color(t.bevel)
            .child(
                text(theme::CAPTION)
                    .flex_1()
                    .text_color(t.muted)
                    .child("Project"),
            )
            .child(
                text(theme::CAPTION)
                    .w(px(130.))
                    .text_color(t.muted)
                    .child("Version"),
            )
            .child(
                text(theme::CAPTION)
                    .w(px(145.))
                    .text_color(t.muted)
                    .child("Actions"),
            );
        let mut table = blocks::panel(&t).flex().flex_col().child(header);
        for item in items {
            table = table.child(self.instance_content_row(item, cx));
        }
        if count == 0 {
            table = table.child(centered(
                "No content matches. Browse to add projects, or change the search.",
                &t,
            ));
        }
        page.child(table)
    }

    fn instance_content_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let sort = if self.instance.updates_first {
            "Updates first"
        } else {
            "Name (A–Z)"
        };
        let kind = self.instance.kind.map_or("All content", ContentKind::label);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(self.instance_field(instance_page::Field::Search, cx))
                    .child(
                        button("instance-add-files", "Add files", Kind::Secondary, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.add_instance_files(cx);
                            }),
                        ),
                    )
                    .child(
                        button("instance-browse", "Browse content", Kind::Secondary, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                if this.set_mods_target(
                                    this.instance.home.clone(),
                                    this.instance.title.clone(),
                                ) {
                                    this.mods.ui.view = browser_controls::LibraryView::Browse;
                                    this.mods.browse.open = None;
                                    this.go(Page::Mods);
                                }
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(self.browser_menu_button(Popup::InstanceSort, sort.into(), cx))
                    .child(self.browser_menu_button(Popup::InstanceKinds, kind.into(), cx))
                    .child(
                        button("instance-update-all", "Update all", Kind::Ghost, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.update_instance_content();
                                cx.notify();
                            }),
                        ),
                    ),
            )
    }

    fn instance_content_row(&self, item: &ContentItem, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let key = item.key.clone();
        let selected = self.instance.selected.contains(&key);
        let checkbox = keyboard(div().id(SharedString::from(format!("select-{key}"))), &t)
            .flex_none()
            .size(px(17.))
            .border_1()
            .border_color(t.muted)
            .cursor_pointer()
            .when(selected, |box_| {
                box_.bg(t.raised).child(blocks::icon("check", 14., t.ink))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !this.instance.selected.remove(&key) {
                    this.instance.selected.insert(key.clone());
                }
                cx.notify();
            }));
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(14.))
            .py(px(14.))
            .border_b_1()
            .border_color(t.bevel)
            .child(checkbox)
            .child(blocks::slot(None, &item.title, 38., &t))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        blocks::pixel(16., FontWeight::MEDIUM)
                            .text_color(t.ink)
                            .child(item.title.clone()),
                    )
                    .child(
                        text(theme::CAPTION)
                            .text_color(t.muted)
                            .child(item.detail.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .w(px(130.))
                    .flex_none()
                    .child(
                        text(theme::CAPTION)
                            .text_color(t.ink_soft)
                            .child(item.version.clone()),
                    )
                    .child(
                        text(theme::MICRO)
                            .font_family(theme::MONO)
                            .text_color(t.muted)
                            .child(item.file.clone()),
                    ),
            )
            .child(self.instance_content_actions(item, cx))
    }

    fn instance_content_actions(&self, item: &ContentItem, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut actions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .w(px(145.))
            .flex_none();
        let taken_down = match &item.origin {
            Origin::Managed(entry) => instance_profile::taken_down(entry).is_some(),
            _ => false,
        };
        if let Some(enabled) = item.enabled.filter(|enabled| !taken_down || *enabled) {
            let item = item.clone();
            actions = actions.child(
                switch(format!("enabled-{}", item.key), enabled, &t).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.instance.message =
                            Some(match this.set_instance_item_enabled(&item, !enabled) {
                                Ok(()) => {
                                    "Content preference saved. Applies on the next launch.".into()
                                }
                                Err(error) => error,
                            });
                        this.reload_instance();
                        cx.notify();
                    },
                )),
            );
        } else {
            actions = actions.child(text(theme::MICRO).text_color(t.muted).child(if taken_down {
                "Taken down"
            } else {
                "Skipped"
            }));
        }
        match &item.origin {
            Origin::Managed(entry) => {
                if self.instance_item_has_update(item) {
                    let entry = entry.clone();
                    actions = actions.child(
                        button(
                            format!("instance-update-{}", item.key),
                            "Update",
                            Kind::Ghost,
                            &t,
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                if this.set_mods_target(
                                    this.instance.home.clone(),
                                    this.instance.title.clone(),
                                ) {
                                    this.mods_install(entry.source, entry.project.clone(), None);
                                }
                                cx.notify();
                            },
                        )),
                    );
                }
                let key = item.key.clone();
                let entry = entry.clone();
                let confirming = self.instance.confirm_remove.as_ref() == Some(&key);
                actions = actions.child(
                    button(
                        format!("instance-remove-{key}"),
                        if confirming { "Remove?" } else { "Remove" },
                        Kind::Ghost,
                        &t,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            if this.instance.confirm_remove.as_ref() == Some(&key) {
                                this.instance.message = Some(
                                    match installed::remove(&this.instance.home, &entry.project) {
                                        Ok(entry) => format!("Removed {}.", entry.title),
                                        Err(error) => error,
                                    },
                                );
                                this.instance.confirm_remove = None;
                                this.reload_instance();
                            } else {
                                this.instance.confirm_remove = Some(key.clone());
                            }
                            cx.notify();
                        },
                    )),
                );
            }
            Origin::Ported { path, .. } if item.enabled.is_some() => {
                let path = path.clone();
                actions = actions.child(
                    button(
                        format!("instance-content-folder-{}", item.key),
                        "Folder",
                        Kind::Ghost,
                        &t,
                    )
                    .on_click(move |_, _, cx| cx.open_with_system(&path)),
                );
            }
            Origin::LocalPack { path, .. } => {
                let path = path.clone();
                actions = actions.child(
                    button(
                        format!("instance-local-folder-{}", item.key),
                        "Folder",
                        Kind::Ghost,
                        &t,
                    )
                    .on_click(move |_, _, cx| {
                        if let Some(folder) = path.parent() {
                            cx.open_with_system(folder);
                        }
                    }),
                );
            }
            _ => {}
        }
        actions
    }

    fn set_instance_item_enabled(
        &mut self,
        item: &ContentItem,
        enabled: bool,
    ) -> Result<(), String> {
        match &item.origin {
            Origin::Managed(entry) => {
                instance_profile::set_enabled(&self.instance.home, entry, enabled)
            }
            Origin::LocalPack { name, .. } => {
                instance_profile::set_resource_pack_enabled(&self.instance.home, name, enabled)
            }
            Origin::Ported { key, .. } => {
                if enabled {
                    self.instance.preferences.disabled_ported.remove(key);
                } else {
                    self.instance
                        .preferences
                        .disabled_ported
                        .insert(key.clone());
                }
                self.instance.preferences.save(&self.instance.home)
            }
        }
    }

    fn instance_bulk_enabled(&mut self, enabled: bool) {
        let items = self
            .instance
            .content
            .iter()
            .filter(|item| self.instance.selected.contains(&item.key) && item.enabled.is_some())
            .cloned()
            .collect::<Vec<_>>();
        let mut failures = vec![];
        for item in items {
            if let Err(error) = self.set_instance_item_enabled(&item, enabled) {
                failures.push(error);
            }
        }
        self.instance.message = Some(if failures.is_empty() {
            "Content preferences saved. Changes apply on next launch.".into()
        } else {
            failures.join("\n")
        });
        self.reload_instance();
    }

    fn update_instance_content(&mut self) {
        let updates = self
            .instance
            .content
            .iter()
            .filter_map(|item| match &item.origin {
                Origin::Managed(entry) if self.instance_item_has_update(item) => {
                    Some((entry.source, entry.project.clone()))
                }
                _ => None,
            })
            .collect::<VecDeque<_>>();
        if self.mods.busy.is_some() {
            self.instance.message = Some("Wait for the current install to finish.".into());
            return;
        }
        if updates.is_empty() {
            self.instance.message =
                Some("No content updates are currently listed. Refresh to check again.".into());
            return;
        }
        if !self.set_mods_target(self.instance.home.clone(), self.instance.title.clone()) {
            return;
        }
        self.mods.pending_updates = updates;
        if let Some((source, project)) = self.mods.pending_updates.pop_front() {
            self.mods_install(source, project, None);
        }
    }

    fn instance_port_controls(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let Some(pack) = self
            .instance
            .id
            .imported_slug()
            .and_then(|slug| self.packs.iter().find(|pack| &pack.slug == slug))
        else {
            return div();
        };
        let port = button(
            "instance-port",
            format!("Port {} supported mods", pack.portable),
            Kind::Secondary,
            &t,
        );
        let jars = button(
            "instance-jars",
            "Use jars from my instance…",
            Kind::Ghost,
            &t,
        );
        let controls = if self.pack_busy || pack.portable == 0 {
            div()
                .flex()
                .gap(px(8.))
                .child(disabled(port))
                .child(disabled(jars))
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .child(port.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.port_pack(false, cx);
                    cx.notify();
                })))
                .child(jars.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.port_pack(true, cx);
                    cx.notify();
                })))
        };
        controls
            .children(self.pack_error.clone().map(|error| notice(error, &t)))
            .children(pack.left_off.iter().map(|line| {
                text(theme::CAPTION)
                    .text_color(t.muted)
                    .child(format!("Left off: {line}"))
            }))
            .when(self.pack_busy || !self.pack_log.is_empty(), |controls| {
                controls.child(self.pack_log_plate(&t))
            })
    }

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
            Popup::InstanceSort => {
                for (updated, label) in [(false, "Name (A–Z)"), (true, "Updates first")] {
                    menu = menu.child(
                        button(format!("instance-sort-{updated}"), label, Kind::Ghost, &t)
                            .justify_start()
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.instance.updates_first = updated;
                                this.close_browser_popup(window);
                                cx.notify();
                            })),
                    );
                }
            }
            Popup::InstanceKinds => {
                for (kind, label) in [
                    (None, "All content"),
                    (Some(ContentKind::Mod), "Mods"),
                    (Some(ContentKind::ResourcePack), "Resource packs"),
                ] {
                    menu = menu.child(
                        button(format!("instance-kind-{label}"), label, Kind::Ghost, &t)
                            .justify_start()
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.instance.kind = kind;
                                this.close_browser_popup(window);
                                cx.notify();
                            })),
                    );
                }
            }
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

    fn add_instance_files(&mut self, cx: &mut Context<Self>) {
        let home = self.instance.home.clone();
        let title = self.instance.title.clone();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add resource packs".into()),
        });
        cx.spawn(async move |this, cx| {
            let paths = match paths.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) => return,
                error => {
                    let _ = this.update(cx, |this, cx| {
                        this.client_error =
                            Some(format!("Could not choose resource packs: {error:?}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let destination = home.clone();
            let result = cx
                .background_executor()
                .spawn(async move { instance_profile::add_resource_packs(&destination, &paths) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let message = match result {
                    Ok(count) => format!(
                        "Added {count} resource packs to {title}. Enable them in its Content tab."
                    ),
                    Err(error) => error,
                };
                this.instance.message = Some(message);
                if this.instance.home == home {
                    this.reload_instance();
                }
                cx.notify();
            });
        })
        .detach();
    }
}
