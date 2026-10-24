//! The approved Content Workspace: one profile, a persistent Play action, and local detail tabs.
use super::*;
use horizon_launcher::{browser::installed, instance_profile::Preferences, modpacks};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Content,
    Worlds,
    Files,
    Logs,
    Settings,
}
const TABS: [(Tab, &str, &str); 5] = [
    (Tab::Content, "Content", "package"),
    (Tab::Worlds, "Worlds", "globe"),
    (Tab::Files, "Files", "folder"),
    (Tab::Logs, "Logs", "terminal"),
    (Tab::Settings, "Settings", "sliders"),
];
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Search,
    Name,
    Arguments,
}

pub(super) struct Target {
    pub(super) id: InstanceId,
    pub(super) name: String,
    pub(super) summary: String,
    pub(super) home: PathBuf,
}

pub(super) struct InstanceUi {
    pub(super) id: InstanceId,
    pub(super) title: String,
    pub(super) home: PathBuf,
    pub(super) preferences: Preferences,
    pub(super) names: HashMap<InstanceId, String>,
    pub(super) tab: Tab,
    pub(super) content: Vec<super::instance_content::ContentItem>,
    pub(super) query: String,
    pub(super) kind: Option<horizon_launcher::browser::Kind>,
    pub(super) updates_first: bool,
    pub(super) selected: std::collections::BTreeSet<String>,
    pub(super) confirm_remove: Option<String>,
    pub(super) message: Option<String>,
    pub(super) loaded: bool,
    files: Vec<(String, PathBuf, bool)>,
    worlds: Vec<(String, PathBuf)>,
    log: String,
    name_input: String,
    args_input: String,
    search_focus: FocusHandle,
    name_focus: FocusHandle,
    args_focus: FocusHandle,
}

impl InstanceUi {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            id: std::env::var("HORIZON_LAUNCHER_INSTANCE")
                .map(InstanceId::Imported)
                .unwrap_or_default(),
            title: "Horizon".into(),
            home: backend::game_home(),
            preferences: Preferences::default(),
            names: HashMap::new(),
            tab: Tab::Content,
            content: vec![],
            query: String::new(),
            kind: None,
            updates_first: false,
            selected: Default::default(),
            confirm_remove: None,
            message: None,
            loaded: false,
            files: vec![],
            worlds: vec![],
            log: String::new(),
            name_input: String::new(),
            args_input: String::new(),
            search_focus: cx.focus_handle(),
            name_focus: cx.focus_handle(),
            args_focus: cx.focus_handle(),
        }
    }
}

impl Launcher {
    pub(super) fn instance_targets(&self) -> Vec<Target> {
        let mut targets = vec![Target {
            id: InstanceId::Horizon,
            name: "Horizon".into(),
            summary: "Vanilla".into(),
            home: backend::game_home(),
        }];
        targets.extend(self.local_instances.iter().map(|instance| {
            Target {
                id: InstanceId::Local(instance.slug.clone()),
                name: instance
                    .preferences
                    .name
                    .clone()
                    .unwrap_or_else(|| "Instance".into()),
                summary: instance
                    .preferences
                    .build
                    .clone()
                    .unwrap_or_else(|| "Vanilla · choose a build".into()),
                home: instance.home.clone(),
            }
        }));
        targets.extend(self.packs.iter().map(|pack| Target {
            id: InstanceId::Imported(pack.slug.clone()),
            name: pack.title().to_owned(),
            summary: pack.summary(),
            home: modpacks::profile_home(&pack.work),
        }));
        targets.extend(self.registry_packs.iter().map(|pack| Target {
            id: InstanceId::Registry(pack.id.clone()),
            name: pack.name.clone(),
            summary: format!(
                "{} · {} mods · Horizon registry",
                pack.version,
                pack.mods.len()
            ),
            home: horizon_launcher::browser::horizon::pack::home(&pack.work),
        }));
        for target in &mut targets {
            if let Some(name) = self.instance.names.get(&target.id) {
                target.name = name.clone();
            }
        }
        targets
    }

    pub(super) fn instance_target(&self, id: &InstanceId) -> Option<Target> {
        self.instance_targets()
            .into_iter()
            .find(|target| &target.id == id)
    }

    pub(super) fn instance_work(&self, id: &InstanceId) -> Option<PathBuf> {
        match id {
            InstanceId::Imported(slug) => self
                .packs
                .iter()
                .find(|pack| &pack.slug == slug)
                .map(|pack| pack.work.clone()),
            InstanceId::Registry(id) => self
                .registry_packs
                .iter()
                .find(|pack| &pack.id == id)
                .map(|pack| pack.work.clone()),
            InstanceId::Horizon | InstanceId::Local(_) => None,
        }
    }

    pub(super) fn open_instance(&mut self, id: InstanceId) {
        self.load_packs();
        if self.instance_target(&id).is_none() {
            return;
        }
        self.instance.id = id.clone();
        self.pack_open = id.imported_slug().cloned();
        self.instance.tab = Tab::Content;
        self.instance.query.clear();
        self.instance.kind = None;
        self.instance.selected.clear();
        self.instance.message = None;
        self.go(Page::Instance);
        self.reload_instance();
        self.set_mods_target(self.instance.home.clone(), self.instance.title.clone());
    }

    pub(super) fn reload_instance(&mut self) {
        self.load_packs();
        self.pack_open = self.instance.id.imported_slug().cloned();
        let Some(target) = self.instance_target(&self.instance.id) else {
            self.instance.message = Some("This instance folder could not be found.".into());
            return;
        };
        self.instance.home = target.home;
        let original_name = target.name;
        match Preferences::load(&self.instance.home) {
            Ok(preferences) => self.instance.preferences = preferences,
            Err(error) => {
                self.instance.preferences = Preferences::default();
                self.instance.message = Some(error);
            }
        }
        self.instance.title = self
            .instance
            .preferences
            .name
            .clone()
            .unwrap_or(original_name);
        self.instance
            .names
            .insert(self.instance.id.clone(), self.instance.title.clone());
        self.instance.name_input = self.instance.title.clone();
        self.instance.args_input = self.instance.preferences.arguments.clone();
        self.instance.content = self.instance_content_snapshot();
        self.instance.files =
            Self::instance_directory(&self.instance.home).unwrap_or_else(|error| {
                self.instance.message = Some(error);
                vec![]
            });
        self.instance.worlds = Self::instance_directory(&self.instance.home.join("saves"))
            .unwrap_or_else(|error| {
                self.instance.message = Some(error);
                vec![]
            })
            .into_iter()
            .filter(|(_, path, directory)| *directory && path.join("level.dat").is_file())
            .map(|(name, path, _)| (name, path))
            .collect();
        self.instance.log = horizon_launcher::instance_profile::latest_log(&self.instance.home)
            .unwrap_or_else(|error| error);
        self.instance.loaded = true;
    }

    fn instance_directory(
        directory: &std::path::Path,
    ) -> Result<Vec<(String, PathBuf, bool)>, String> {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(format!("Could not read {}: {error}", directory.display())),
        };
        let mut files = vec![];
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let is_directory = entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir();
            files.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
                is_directory,
            ));
        }
        files.sort_by_key(|(name, _, directory)| (!directory, name.to_lowercase()));
        Ok(files)
    }

    pub(super) fn instance_build(&self) -> Option<String> {
        if self.instance.id == InstanceId::Horizon {
            return self.selected().cloned();
        }
        self.instance
            .preferences
            .build
            .as_ref()
            .filter(|tag| self.installed.contains(tag))
            .cloned()
            .or_else(|| self.selected().cloned())
    }

    fn play_instance(&mut self, cx: &mut Context<Self>) {
        let Some(tag) = self.instance_build() else {
            self.go(Page::Versions);
            return;
        };
        if self.preparing || self.starting_client.is_some() {
            return;
        }
        if !backend::is_prepared(&tag) {
            self.prepare_version(tag);
            return;
        }
        if let Some(client) = &self.client {
            if self.client_home.as_ref() == Some(&self.instance.home) {
                client.kill();
            }
            return;
        }
        if self
            .instance
            .id
            .imported_slug()
            .and_then(|slug| self.packs.iter().find(|pack| &pack.slug == slug))
            .is_some_and(|pack| pack.mods.is_empty())
        {
            self.port_pack(false, cx);
            return;
        }
        self.starting_pack = self.instance_work(&self.instance.id);
        self.starting_home = Some(self.instance.home.clone());
        self.start_client(tag);
    }

    pub(super) fn set_mods_target(&mut self, home: PathBuf, title: String) -> bool {
        if self.mods.busy.is_some() || !self.mods.pending_updates.is_empty() {
            self.instance.message = Some(
                "Wait for the current content operation before changing its destination.".into(),
            );
            return false;
        }
        self.mods.home = home;
        self.mods.target = title;
        self.mods.updates.clear();
        self.mods.installed = installed::Installed::load(&self.mods.home).entries;
        self.mods.reset_lists();
        self.mods_check_updates();
        self.mods_taken_down();
        true
    }

    pub(super) fn instance_page(&mut self, cx: &mut Context<Self>) -> Div {
        if !self.instance.loaded {
            self.reload_instance();
        }
        let header = self.instance_header(cx);
        let tabs = self.instance_tabs(cx);
        let body = match self.instance.tab {
            Tab::Content => self.instance_content(cx),
            Tab::Worlds => self.instance_worlds(cx),
            Tab::Files => self.instance_files(cx),
            Tab::Logs => self.instance_logs(cx),
            Tab::Settings => self.instance_settings(cx),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(header)
            .child(tabs)
            .child(body)
            .children(self.instance.message.clone().map(|message| {
                text(theme::CAPTION)
                    .text_color(self.theme.ink_soft)
                    .child(message)
            }))
            .children(
                self.client_error
                    .clone()
                    .map(|error| notice(error, &self.theme)),
            )
            .children(
                self.prepare_error
                    .clone()
                    .map(|error| notice(error, &self.theme)),
            )
    }

    fn instance_header(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let icon = self.instance_art(&self.instance.id.clone(), 60.);
        let tag = self.instance_build();
        let prepared = tag.as_deref().is_some_and(backend::is_prepared);
        let running =
            self.client.is_some() && self.client_home.as_ref() == Some(&self.instance.home);
        let pack = self
            .instance
            .id
            .imported_slug()
            .and_then(|slug| self.packs.iter().find(|pack| &pack.slug == slug));
        let needs_port = pack.is_some_and(|pack| pack.mods.is_empty());
        let no_supported_content =
            pack.is_some_and(|pack| pack.mods.is_empty() && pack.portable == 0);
        let label = if running {
            "Stop"
        } else if self.preparing {
            "Preparing…"
        } else if self.starting_client.is_some() {
            "Starting…"
        } else if tag.is_none() {
            "Install a build"
        } else if !prepared {
            "Set up files"
        } else if no_supported_content {
            "No supported content"
        } else if needs_port {
            "Port content"
        } else {
            "Play"
        };
        let metadata = tag.as_ref().map_or_else(
            || "No build installed".into(),
            |tag| {
                format!(
                    "Minecraft {} · Horizon {tag} · {}",
                    backend::game_version(tag),
                    if running {
                        "Running"
                    } else if prepared {
                        "Ready to play"
                    } else {
                        "Needs game files"
                    }
                )
            },
        );
        let can_start = !self.preparing
            && self.starting_client.is_none()
            && !self.pack_busy
            && (self.client.is_none() || running)
            && (!no_supported_content || running || !prepared);
        let play = button("instance-play", label, Kind::Primary, &t)
            .h(px(44.))
            .min_w(px(108.))
            .child(blocks::icon("play", 18., t.on_face));
        let play = if can_start {
            play.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.play_instance(cx);
                cx.notify();
            }))
        } else {
            disabled(play)
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .pb(px(20.))
            .border_b_1()
            .border_color(t.bevel)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .flex_1()
                    .min_w(px(230.))
                    .child(icon)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .min_w_0()
                            .child(
                                blocks::pixel(30., FontWeight::MEDIUM)
                                    .text_color(t.ink)
                                    .child(self.instance.title.clone()),
                            )
                            .child(text(theme::CAPTION).text_color(t.muted).child(metadata)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(play)
                    .child(
                        button("instance-settings", "Settings", Kind::Secondary, &t)
                            .child(blocks::icon("sliders", 16., t.ink_soft))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.instance.tab = Tab::Settings;
                                cx.notify();
                            })),
                    )
                    .child(self.browser_menu_button(
                        browser_controls::Popup::InstanceMore,
                        "More".into(),
                        cx,
                    )),
            )
    }

    fn instance_tabs(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        div()
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .border_b_2()
            .border_color(t.outline)
            .children(TABS.map(|(tab, label, icon)| {
                let active = self.instance.tab == tab;
                keyboard(
                    div().id(SharedString::from(format!("instance-tab-{label}"))),
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
                    this.instance.tab = tab;
                    this.mods.ui.popup = None;
                    this.instance.message = None;
                    this.reload_instance();
                    cx.notify();
                }))
            }))
    }

    fn instance_worlds(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut list = div().flex().flex_col().gap(px(8.));
        if self.instance.worlds.is_empty() {
            return list.child(centered(
                "No worlds in this instance yet. Play to create one.",
                &t,
            ));
        }
        for (name, path) in &self.instance.worlds {
            let path = path.clone();
            list = list.child(index_row(
                0,
                name.clone(),
                Some(
                    div()
                        .font_family(theme::MONO)
                        .child(short(&path))
                        .into_any_element(),
                ),
                Some(
                    button(
                        format!("world-folder-{name}"),
                        "Open folder",
                        Kind::Secondary,
                        &t,
                    )
                    .on_click(move |_, _, cx| cx.open_with_system(&path))
                    .into_any_element(),
                ),
                &t,
            ));
        }
        let _ = cx;
        list
    }

    fn instance_files(&self, _cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut list = div().flex().flex_col().gap(px(8.)).child(
            text(theme::CAPTION)
                .font_family(theme::MONO)
                .text_color(t.muted)
                .child(short(&self.instance.home)),
        );
        if self.instance.files.is_empty() {
            return list.child(centered(
                "This profile folder is created when you first play or add content.",
                &t,
            ));
        }
        for (name, path, directory) in &self.instance.files {
            let path = path.clone();
            list = list.child(index_row(
                0,
                name.clone(),
                Some(
                    div()
                        .child(if *directory { "Folder" } else { "File" })
                        .into_any_element(),
                ),
                Some(
                    button(format!("instance-file-{name}"), "Open", Kind::Ghost, &t)
                        .on_click(move |_, _, cx| cx.open_with_system(&path))
                        .into_any_element(),
                ),
                &t,
            ));
        }
        list
    }

    fn instance_logs(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let folder = self.instance.home.join("logs");
        let log = self.instance.log.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button(
                            "instance-log-folder",
                            "Open logs folder",
                            Kind::Secondary,
                            &t,
                        )
                        .on_click(move |_, _, cx| cx.open_with_system(&folder)),
                    )
                    .child(
                        button("instance-log-copy", "Copy log", Kind::Ghost, &t).on_click(
                            move |_, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(log.clone()))
                            },
                        ),
                    )
                    .child(
                        button("instance-log-refresh", "Refresh", Kind::Ghost, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.reload_instance();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                blocks::panel(&t)
                    .bg(t.well)
                    .p(px(16.))
                    .font_family(theme::MONO)
                    .text_size(px(12.))
                    .line_height(px(19.))
                    .text_color(t.ink_soft)
                    .children(
                        self.instance
                            .log
                            .lines()
                            .map(|line| div().child(line.to_owned())),
                    ),
            )
    }

    fn instance_settings(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let folder = self.instance.home.clone();
        let build = self
            .instance_build()
            .unwrap_or_else(|| "Choose a build".into());
        div().flex().flex_col().gap(px(10.))
            .child(index_row(1,"Name",Some(div().child("The name shown in the instance sidebar. Press Enter to save.").into_any_element()),Some(self.instance_field(Field::Name,cx).into_any_element()),&t))
            .child(index_row(2,"Horizon build",Some(div().child("Used on this instance’s next launch.").into_any_element()),Some(self.browser_menu_button(browser_controls::Popup::InstanceBuild,build,cx).into_any_element()),&t))
            .child(index_row(3,"Game arguments",Some(div().child("Additional arguments for this instance. Quotes group words with spaces. Press Enter to save.").into_any_element()),Some(self.instance_field(Field::Arguments,cx).into_any_element()),&t))
            .child(index_row(4,"Instance folder",Some(div().font_family(theme::MONO).child(short(&folder)).into_any_element()),Some(button("instance-open-folder","Open",Kind::Secondary,&t).on_click(move|_,_,cx|cx.open_with_system(&folder)).into_any_element()),&t))
    }

    pub(super) fn instance_field(&self, field: Field, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let (id, focus, value, placeholder) = match field {
            Field::Search => (
                "instance-search",
                &self.instance.search_focus,
                &self.instance.query,
                "Search content…",
            ),
            Field::Name => (
                "instance-name",
                &self.instance.name_focus,
                &self.instance.name_input,
                "Instance name",
            ),
            Field::Arguments => (
                "instance-arguments",
                &self.instance.args_focus,
                &self.instance.args_input,
                "For example --server-thread",
            ),
        };
        let focus = focus.clone();
        div()
            .id(id)
            .track_focus(&focus)
            .flex()
            .items_center()
            .w(px(if field == Field::Search { 180. } else { 250. }))
            .h(px(36.))
            .px(px(10.))
            .border_2()
            .border_color(t.outline)
            .bg(t.well)
            .overflow_hidden()
            .cursor_text()
            .text_size(px(12.))
            .text_color(t.ink)
            .child(if value.is_empty() {
                placeholder.to_owned()
            } else {
                value
                    .chars()
                    .rev()
                    .take(40)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .on_click(move |_, window, _| window.focus(&focus))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let input = match field {
                    Field::Search => &mut this.instance.query,
                    Field::Name => &mut this.instance.name_input,
                    Field::Arguments => &mut this.instance.args_input,
                };
                let key = &event.keystroke;
                match servers_page::type_key(
                    input,
                    false,
                    &key.key,
                    key.key_char.as_deref(),
                    key.modifiers.control || key.modifiers.platform,
                ) {
                    servers_page::Key::Enter => {
                        if field != Field::Search {
                            this.save_instance_preferences();
                        }
                    }
                    servers_page::Key::Escape => window.blur(),
                    servers_page::Key::Paste => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            input.push_str(text.lines().next().unwrap_or_default());
                        }
                    }
                    servers_page::Key::Edited => {}
                    servers_page::Key::Pass => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    pub(super) fn save_instance_preferences(&mut self) {
        let name = self.instance.name_input.trim();
        if name.is_empty() {
            self.instance.message = Some("Enter an instance name before saving.".into());
            return;
        }
        let mut preferences = self.instance.preferences.clone();
        preferences.name = Some(name.to_owned());
        preferences.arguments = self.instance.args_input.clone();
        match preferences.save(&self.instance.home) {
            Ok(()) => {
                self.instance.preferences = preferences;
                self.reload_instance();
                self.instance.message =
                    Some("Instance preferences saved. Changes apply on the next launch.".into());
            }
            Err(error) => self.instance.message = Some(error),
        }
    }
}
