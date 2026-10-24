//! Section navigation for the launcher's existing settings and account/agent flows.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Section {
    Account,
    #[default]
    Launcher,
    Updates,
    Game,
    Ai,
    Data,
    About,
}

impl Section {
    const ALL: [(Self, &'static str, &'static str); 7] = [
        (Self::Account, "Account", "user"),
        (Self::Launcher, "Launcher", "monitor"),
        (Self::Updates, "Updates", "reload"),
        (Self::Game, "Game", "gamepad"),
        (Self::Ai, "AI", "terminal"),
        (Self::Data, "Data collection", "archive"),
        (Self::About, "About", "info-box"),
    ];
}

#[derive(Default)]
pub(super) struct SettingsUi {
    pub(super) section: Section,
    error: Option<String>,
}

impl Launcher {
    pub(super) fn settings_page(&mut self, cx: &mut Context<Self>) -> Div {
        let navigation = self.settings_navigation(cx);
        let content = match self.settings_ui.section {
            Section::Account => self.account_rows(cx).0,
            Section::Launcher => self.launcher_settings(cx),
            Section::Updates => self.update_settings(cx),
            Section::Game => div().child(self.game_args_row(1, cx)),
            Section::Ai => div()
                .flex()
                .flex_col()
                .gap(px(24.))
                .child(self.no_ai_row(1, cx))
                .children(self.agents_section(cx)),
            Section::Data => self.collection_section(cx),
            Section::About => self.about_settings(cx),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(navigation)
            .child(content)
            .children(
                self.settings_ui
                    .error
                    .clone()
                    .map(|error| notice(error, &self.theme)),
            )
    }

    fn settings_navigation(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        div()
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .border_b_2()
            .border_color(t.outline)
            .children(Section::ALL.map(|(section, label, icon)| {
                let active = self.settings_ui.section == section;
                keyboard(
                    div().id(SharedString::from(format!("settings-{label}"))),
                    &t,
                )
                .flex()
                .items_center()
                .gap(px(7.))
                .px(px(12.))
                .py(px(10.))
                .border_b_2()
                .border_color(if active { t.ink } else { t.outline })
                .when(active, |item| item.bg(t.raised))
                .cursor_pointer()
                .hover(move |item| item.bg(t.raised))
                .child(blocks::icon(
                    icon,
                    18.,
                    if active { t.ink } else { t.muted },
                ))
                .child(
                    blocks::pixel(16., FontWeight::MEDIUM)
                        .text_color(if active { t.ink } else { t.muted })
                        .child(label),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.settings_ui.section = section;
                    this.settings_ui.error = None;
                    this.content_scroll = Scrollbar::default();
                    cx.notify();
                }))
            }))
    }

    fn launcher_settings(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let entity = cx.entity().downgrade();
        let appearance = segments(
            "appearance",
            &[
                (backend::Appearance::Dark, "Dark"),
                (backend::Appearance::Light, "Light"),
                (backend::Appearance::System, "System"),
            ],
            self.settings.appearance,
            &t,
            move |choice, window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.settings.appearance = choice;
                    this.settings.save();
                    this.theme = Theme::for_appearance(choice, window.appearance());
                    cx.notify();
                });
            },
        );
        let opening = switch("opening", !self.settings.skip_opening, &t).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.settings.skip_opening = !this.settings.skip_opening;
                this.settings.save();
                cx.notify();
            },
        ));
        div().flex().flex_col().gap(px(8.))
            .child(index_row(1, "Appearance", Some(div().child("Dark by default. System follows your computer’s light or dark appearance.").into_any_element()), Some(appearance.into_any_element()), &t))
            .child(index_row(2, "Show the opening", Some(div().child("Show the wordmark while the release list loads.").into_any_element()), Some(opening.into_any_element()), &t))
            .when(cfg!(target_os = "linux"), |page| page.child(self.desktop_entry_setting(cx)))
    }

    fn desktop_entry_setting(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let enabled = self.settings.desktop_entry();
        let toggle = switch("desktop-entry", enabled, &t).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                let enabled = !this.settings.desktop_entry();
                let result = if enabled {
                    horizon_launcher::desktop::install(backend::exe_path()).map(|_| ())
                } else {
                    horizon_launcher::desktop::remove()
                };
                match result {
                    Ok(()) => {
                        this.settings.desktop_entry = Some(enabled);
                        this.settings.save();
                    }
                    Err(error) => this.settings_ui.error = Some(error),
                }
                cx.notify();
            },
        ));
        index_row(
            3,
            "Applications menu",
            Some(
                div()
                    .child(if enabled {
                        "Horizon and its icon are in your desktop’s menu."
                    } else {
                        "Horizon is not in your desktop’s menu."
                    })
                    .into_any_element(),
            ),
            Some(toggle.into_any_element()),
            &t,
        )
    }

    fn update_settings(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let toggle = switch("auto-install", self.settings.auto_install(), &t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.settings.install_new_versions = Some(!this.settings.auto_install());
                this.settings.save();
                if this.settings.auto_install() {
                    this.install_update();
                }
                cx.notify();
            }),
        );
        let status = match (&self.updates.horizon, &self.update_error) {
            (Some(release), _) => format!("{} available", release.tag),
            (_, Some(error)) => error.clone(),
            _ if self.checked.is_some() => "Up to date".into(),
            _ => "Checking…".into(),
        };
        let install = self
            .updates
            .horizon
            .as_ref()
            .filter(|_| self.installing.is_none())
            .map(|_| {
                button("settings-install", "Install", Kind::Secondary, &t)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.install_update();
                        cx.notify();
                    }))
                    .into_any_element()
            });
        div().flex().flex_col().gap(px(8.))
            .child(index_row(1, "Install new versions automatically", Some(div().child("Your selected version only changes if you were playing the newest one.").into_any_element()), Some(toggle.into_any_element()), &t))
            .child(index_row(2, "Horizon", Some(div().child(status).into_any_element()), install, &t))
            .child(self.launcher_update_setting(cx))
    }

    fn launcher_update_setting(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let status = match &self.self_update {
            SelfUpdate::Downloading(version, progress) => format!(
                "{} · downloading {version}, {:.0}%",
                backend::VERSION,
                progress * 100.
            ),
            SelfUpdate::Ready(version) => {
                format!("Updated to {version}. Restart the launcher to use it.")
            }
            SelfUpdate::Failed(error) => format!("{} · update failed: {error}", backend::VERSION),
            SelfUpdate::Idle => match &self.updates.launcher {
                Some(release) => format!(
                    "{} · {} available",
                    backend::VERSION,
                    backend::launcher_version(&release.tag).unwrap_or_default()
                ),
                None if self.checked.is_some() && self.update_error.is_none() => {
                    format!("{} · up to date", backend::VERSION)
                }
                None => backend::VERSION.into(),
            },
        };
        let action = match self.self_update {
            SelfUpdate::Downloading(..) => None,
            SelfUpdate::Ready(_) => Some(
                button("restart-settings", "Restart", Kind::Secondary, &t)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.self_update = SelfUpdate::Failed(backend::restart());
                        cx.notify();
                    }))
                    .into_any_element(),
            ),
            _ => Some(
                button("check", "Check now", Kind::Ghost, &t)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.check_updates();
                        cx.notify();
                    }))
                    .into_any_element(),
            ),
        };
        index_row(
            3,
            "Launcher",
            Some(div().child(status).into_any_element()),
            action,
            &t,
        )
    }

    fn about_settings(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let open_logs = button("open-logs", "Open", Kind::Secondary, &t).on_click(|_, _, cx| {
            let directory = backend::logs_dir();
            cx.open_with_system(&directory);
        });
        let notes = button("about-notes", "View patch notes", Kind::Ghost, &t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.go(Page::Notes);
                cx.notify();
            }),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(index_row(
                1,
                "Data folder",
                Some(
                    div()
                        .font_family(theme::MONO)
                        .child(short(&backend::data_dir()))
                        .into_any_element(),
                ),
                None,
                &t,
            ))
            .child(index_row(
                2,
                "Logs",
                Some(
                    div()
                        .font_family(theme::MONO)
                        .child(format!(
                            "{} · crash reports beside it",
                            short(&backend::logs_dir())
                        ))
                        .into_any_element(),
                ),
                Some(open_logs.into_any_element()),
                &t,
            ))
            .child(index_row(
                3,
                "Launcher version",
                Some(div().child(backend::VERSION).into_any_element()),
                Some(notes.into_any_element()),
                &t,
            ))
    }
}
