//! Home keeps its original composition and chooses a launch profile independently of the workspace.
use super::browser_controls::Popup;
use super::*;
use horizon_launcher::instance_profile::Preferences;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct HomeUi {
    pub(super) id: InstanceId,
    preferences: Preferences,
    pub(super) loaded: bool,
}

impl HomeUi {
    fn build<'a>(
        &'a self,
        installed: &[String],
        fallback: Option<&'a String>,
    ) -> Option<&'a String> {
        if self.id == InstanceId::Horizon {
            return fallback;
        }
        self.preferences
            .build
            .as_ref()
            .filter(|tag| installed.contains(tag))
            .or(fallback)
    }
}

impl Launcher {
    pub(super) fn refresh_home_instance(&mut self) {
        self.load_packs();
        match Preferences::load(&self.home_instance_folder()) {
            Ok(preferences) => {
                if let Some(name) = &preferences.name {
                    self.instance
                        .names
                        .insert(self.home_ui.id.clone(), name.clone());
                }
                self.home_ui.preferences = preferences;
            }
            Err(error) => {
                self.home_ui.preferences = Preferences::default();
                self.client_error = Some(error);
            }
        }
        self.home_ui.loaded = true;
    }

    pub(super) fn home_instance_folder(&self) -> PathBuf {
        self.instance_target(&self.home_ui.id)
            .map_or_else(backend::game_home, |target| target.home)
    }

    pub(super) fn home_instance_work(&self) -> Option<PathBuf> {
        self.instance_work(&self.home_ui.id)
    }

    pub(super) fn home_build(&self) -> Option<&String> {
        self.home_ui.build(&self.installed, self.selected())
    }

    pub(super) fn home_needs_port(&self) -> bool {
        self.home_ui.id.imported_slug().is_some_and(|slug| {
            self.packs
                .iter()
                .any(|pack| &pack.slug == slug && pack.mods.is_empty())
        })
    }

    pub(super) fn home_selector(&self, popup: Popup, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let (caption, label, icon) = match popup {
            Popup::HomeInstances => ("INSTANCE", self.home_instance_name(), "package"),
            Popup::HomeBuild => (
                "VERSION",
                self.home_build()
                    .cloned()
                    .unwrap_or_else(|| "No version".into()),
                "archive",
            ),
            _ => unreachable!("Home selectors only choose an instance or build"),
        };
        let trigger = keyboard(div().id(SharedString::from(format!("home-{caption}"))), &t)
            .relative()
            .flex()
            .items_center()
            .gap(px(12.))
            .h(px(60.))
            .px(px(12.))
            .bg(t.panel)
            .border_2()
            .border_color(t.outline)
            .children(blocks::edges(t.bevel, t.lip, theme::BEVEL))
            .cursor_pointer()
            .hover(move |style| style.bg(t.raised))
            .when(popup == Popup::HomeInstances, |button| {
                button.child(blocks::icon(icon, 24., t.accent))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .min_w_0()
                    .child(
                        blocks::pixel(11., FontWeight::MEDIUM)
                            .text_color(t.muted)
                            .child(caption),
                    )
                    .child(
                        div()
                            .max_w(px(170.))
                            .truncate()
                            .font_family(theme::MONO)
                            .text_size(px(15.))
                            .text_color(t.ink)
                            .child(label),
                    ),
            )
            .child(blocks::icon("chevron-down", 20., t.muted));
        self.browser_menu_trigger(popup, trigger, cx)
    }

    fn home_instance_name(&self) -> String {
        if let Some(name) = self.instance.names.get(&self.home_ui.id) {
            return name.clone();
        }
        self.instance_target(&self.home_ui.id)
            .map_or_else(|| "Horizon".into(), |target| target.name)
    }

    pub(super) fn home_menu(&self, popup: Popup, height: f32, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut menu = div()
            .id("home-menu-body")
            .flex()
            .flex_col()
            .gap(px(2.))
            .max_h(px((height - 12.).max(30.)))
            .overflow_y_scroll();
        if popup == Popup::HomeBuild {
            for tag in &self.installed {
                let tag = tag.clone();
                menu = menu.child(
                    button(format!("home-build-{tag}"), tag.clone(), Kind::Ghost, &t)
                        .justify_start()
                        .when(self.home_build() == Some(&tag), |button| {
                            button.bg(t.raised)
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.select_home_build(tag.clone());
                            this.close_browser_popup(window);
                            cx.notify();
                        })),
                );
            }
            menu = menu.child(
                button("home-manage-builds", "Manage versions", Kind::Ghost, &t)
                    .justify_start()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.go(Page::Versions);
                        cx.notify();
                    })),
            );
        } else {
            for target in self.instance_targets() {
                let id = target.id;
                let name = target.name;
                let detail = target.summary;
                menu = menu.child(
                    button(format!("home-instance-{}", id.key()), "", Kind::Ghost, &t)
                        .h_auto()
                        .min_h(px(62.))
                        .justify_start()
                        .gap(px(10.))
                        .when(self.home_ui.id == id, |button| button.bg(t.raised))
                        .child(blocks::icon(
                            if id == InstanceId::Horizon {
                                "home"
                            } else {
                                "package"
                            },
                            24.,
                            t.accent,
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .items_start()
                                .gap(px(3.))
                                .child(
                                    blocks::pixel(17., FontWeight::MEDIUM)
                                        .truncate()
                                        .child(name),
                                )
                                .child(
                                    text(theme::MICRO)
                                        .text_color(t.muted)
                                        .truncate()
                                        .child(detail),
                                ),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.home_ui.id = id.clone();
                            this.refresh_home_instance();
                            this.close_browser_popup(window);
                            cx.notify();
                        })),
                );
            }
        }
        blocks::panel(&t).p(px(6.)).child(menu)
    }

    fn select_home_build(&mut self, tag: String) {
        if self.home_ui.id == InstanceId::Horizon {
            self.settings.selected = Some(tag);
            self.settings.save();
            return;
        }
        let mut preferences = self.home_ui.preferences.clone();
        preferences.build = Some(tag);
        match preferences.save(&self.home_instance_folder()) {
            Ok(()) => self.home_ui.preferences = preferences,
            Err(error) => self.client_error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_uses_the_instances_installed_build_and_falls_back_when_removed() {
        let fallback = "v2".to_owned();
        let mut home = HomeUi {
            id: InstanceId::Imported("weekend".into()),
            preferences: Preferences {
                build: Some("v1".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            home.build(&["v1".into(), "v2".into()], Some(&fallback))
                .map(String::as_str),
            Some("v1")
        );
        assert_eq!(home.build(&["v2".into()], Some(&fallback)), Some(&fallback));
        assert!(home.build(&[], None).is_none());
        home.id = InstanceId::Horizon;
        assert_eq!(
            home.build(&["v1".into(), "v2".into()], Some(&fallback)),
            Some(&fallback)
        );
    }
}
