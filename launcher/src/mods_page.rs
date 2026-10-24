//! The Mods page: search Horizon's own registry for mods made for Horizon, or Modrinth
//! (CurseForge: coming soon) for Java mods, modpacks and resource packs, see where each comes
//! from and how well it runs (a Java mod's jar is checked first; a Horizon mod says whether it is
//! signed and what it may do), install it with its checksum checked, turn Horizon mods on and
//! off, and update or remove what was installed (`horizon_launcher::browser`). Imported modpacks
//! are listed on its Modpacks tab.
use super::servers_page::{type_key, Key};
use super::*;
use horizon_launcher::browser::{
    self, horizon, installed, java, Kind as PackKind, Project, Query, SourceId, Support,
};
use horizon_launcher::modpacks;
use horizon_port_pack::check::Check;
use std::collections::HashMap;

/// The kinds, as the Mods page's tabs: name and icon.
const KINDS: [(PackKind, &str, &str); 3] = [
    (PackKind::Mod, "Mods", "box"),
    (PackKind::Modpack, "Modpacks", "archive"),
    (PackKind::ResourcePack, "Resource packs", "image"),
];
const PAGE: usize = 20;

pub(super) enum Event {
    /// A search's answer, for the search it was (stale answers are dropped) at that offset.
    Found(u64, usize, Result<Vec<Project>, String>),
    Progress(String, f32),
    Done(Result<String, String>, bool),
    Updates(std::path::PathBuf, HashMap<(SourceId, String), String>),
    /// A Java mod's check: its jar read, or why it couldn't be.
    Checked(u64, (SourceId, String), Result<Box<Check>, String>),
    /// What a Horizon registry project says before install: signed or not, what it may do.
    About(u64, HashMap<String, Vec<String>>),
    /// Registry files taken down that the launcher turned off.
    TakenDown(std::path::PathBuf, Vec<String>),
    /// A project page's part, an image or a filter's values (`mods_project`).
    Loaded(super::mods_project::Loaded),
}

/// Where a Java mod's check is.
pub(super) enum Checking {
    Running,
    Done(Box<Check>),
    Failed(String),
}

pub(super) struct Mods {
    pub(super) ui: browser_controls::BrowserUi,
    pub(super) home: std::path::PathBuf,
    pub(super) target: String,
    pub(super) kind: PackKind,
    pub(super) source: SourceId,
    text: String,
    focus: FocusHandle,
    pub(super) focused: bool,
    pub(super) everything: bool,
    /// Which search is the current one; answers for older ones are dropped.
    pub(super) search: u64,
    pub(super) check_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub(super) check_jobs: Option<Sender<content_checks::Request>>,
    searching: bool,
    searched: bool,
    results: Vec<Project>,
    results_list: cull::Cull,
    installed_list: cull::Cull,
    pub(super) report_list: cull::Cull,
    more: bool,
    error: Option<String>,
    pub(super) installed: Vec<installed::Entry>,
    loaded: bool,
    /// The project being installed, and the line and fraction of its download.
    pub(super) busy: Option<(String, String, f32)>,
    message: Option<Result<String, String>>,
    pub(super) updates: HashMap<(SourceId, String), String>,
    pub(super) pending_updates: VecDeque<(SourceId, String)>,
    confirm_remove: Option<String>,
    /// Java mods' checks, by source and project.
    pub(super) checks: HashMap<(SourceId, String), Checking>,
    /// Horizon registry projects' trust and capabilities, by id ([`horizon::trust_and_capabilities`]).
    pub(super) about: HashMap<String, Vec<String>>,
    /// The project page, filters and images.
    pub(super) browse: super::mods_project::Browse,
    pub(super) tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Mods {
    pub(super) fn reset_lists(&self) {
        self.results_list.reset();
        self.installed_list.reset();
        self.report_list.reset();
    }

    pub(super) fn new(cx: &mut App) -> Self {
        let (tx, rx) = channel();
        Self {
            ui: browser_controls::BrowserUi::new(cx),
            home: backend::game_home(),
            target: "Horizon".into(),
            kind: if std::env::var("HORIZON_LAUNCHER_PAGE").as_deref() == Ok("modpacks") {
                PackKind::Modpack
            } else {
                PackKind::Mod
            },
            // `HORIZON_LAUNCHER_SOURCE=horizon` opens on Horizon's registry, for screenshots.
            source: match std::env::var("HORIZON_LAUNCHER_SOURCE").as_deref() {
                Ok("horizon") => SourceId::Horizon,
                _ => SourceId::Modrinth,
            },
            text: String::new(),
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            focused: false,
            everything: true,
            search: 0,
            check_generation: Default::default(),
            check_jobs: None,
            searching: false,
            searched: false,
            results: Vec::new(),
            results_list: cull::Cull::default(),
            installed_list: cull::Cull::default(),
            report_list: cull::Cull::default(),
            more: false,
            error: None,
            installed: Vec::new(),
            loaded: false,
            busy: None,
            message: None,
            updates: HashMap::new(),
            pending_updates: VecDeque::new(),
            confirm_remove: None,
            checks: HashMap::new(),
            about: HashMap::new(),
            browse: Default::default(),
            tx,
            rx,
        }
    }

    pub(super) fn note_focus(&mut self, window: &Window) {
        self.focused = self.focus.is_focused(window);
    }

    pub(super) fn entry(&self, p: &Project) -> Option<&installed::Entry> {
        self.installed
            .iter()
            .find(|e| e.source == p.source && e.project == p.id)
    }

    /// How well a result runs: a Java mod's from its check once it has one.
    pub(super) fn support(&self, p: &Project) -> Support {
        let metadata = p.support();
        if p.kind != PackKind::Mod || p.source == SourceId::Horizon || !metadata.installable() {
            return metadata;
        }
        match self.checks.get(&(p.source, p.id.clone())) {
            Some(Checking::Done(c)) => java::support_of(c),
            Some(Checking::Failed(e)) => Support::Partial(format!("Not checked: {e}")),
            _ => Support::Partial("Checking its jar…".into()),
        }
    }
}

/// `51404735` → `51.4M`.
pub(super) fn count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

/// A checksum as the page shows it: the algorithm and the first and last hex digits.
fn short_sum(checksum: &str) -> String {
    let (algo, hex) = checksum.split_once(':').unwrap_or(("", checksum));
    if hex.len() <= 20 {
        return checksum.to_string();
    }
    format!("{algo} {}…{}", &hex[..12], &hex[hex.len() - 8..])
}

impl Launcher {
    pub(super) fn content_is_installed(&self, project: &Project) -> bool {
        if project.source == SourceId::Horizon && project.kind == PackKind::Modpack {
            return self.registry_packs.iter().any(|pack| pack.id == project.id);
        }
        self.mods.entry(project).is_some()
    }

    /// Called with the launcher's other work: what the background sent.
    pub(super) fn mods_tick(&mut self) -> bool {
        let mut changed = false;
        let mut instance_changed = false;
        let mut to_check = Vec::new();
        let mut loaded = Vec::new();
        while let Ok(event) = self.mods.rx.try_recv() {
            changed = true;
            let m = &mut self.mods;
            match event {
                Event::Found(search, offset, result) if search == m.search => {
                    m.searching = false;
                    m.searched = true;
                    match result {
                        Ok(found) => {
                            m.more = found.len() == PAGE;
                            m.results.truncate(offset);
                            m.results.extend(found.iter().cloned());
                            m.error = None;
                            to_check.extend(found);
                        }
                        Err(e) => m.error = Some(e),
                    }
                }
                Event::Found(..) => {}
                Event::Progress(line, f) => {
                    if let Some(busy) = &mut m.busy {
                        busy.1 = line;
                        busy.2 = f;
                    }
                }
                Event::Done(result, modpack) => {
                    instance_changed = true;
                    if result.is_err() {
                        m.pending_updates.clear();
                    }
                    m.busy = None;
                    m.message = Some(result);
                    m.installed = installed::Installed::load(&m.home).entries;
                    m.installed_list.reset();
                    if modpack {
                        // The Modpacks page reads its list again.
                        self.packs_loaded = None;
                    }
                }
                Event::Updates(home, found) if home == m.home => {
                    m.updates = found;
                    instance_changed = true;
                }
                Event::Updates(..) => {}
                Event::Checked(generation, key, result) if generation == m.search => {
                    let state = match result {
                        Ok(c) => Checking::Done(c),
                        Err(e) => Checking::Failed(e),
                    };
                    m.checks.insert(key, state);
                }
                Event::Checked(..) => {}
                Event::Loaded(l) => loaded.push(l),
                Event::About(search, about) if search == m.search => {
                    // Rows grow by a line: measure them again (`cull`).
                    m.about.extend(about);
                    m.results_list.reset();
                }
                Event::About(..) => {}
                Event::TakenDown(home, lines) if home == m.home => {
                    if !lines.is_empty() {
                        m.message = Some(Ok(lines.join("\n")));
                    }
                    m.installed = installed::Installed::load(&m.home).entries;
                    m.installed_list.reset();
                    instance_changed = true;
                }
                Event::TakenDown(..) => {}
            }
        }
        // `HORIZON_LAUNCHER_PROJECT=<slug>` (with `HORIZON_LAUNCHER_PAGE=mods`): opens that
        // project's page once a search finds it, for screenshots.
        if self.mods.browse.open.is_none() {
            if let Ok(want) = std::env::var("HORIZON_LAUNCHER_PROJECT") {
                if let Some(p) = self
                    .mods
                    .results
                    .iter()
                    .find(|p| p.id == want || p.slug == want)
                    .cloned()
                {
                    std::env::remove_var("HORIZON_LAUNCHER_PROJECT");
                    self.mods_open(p);
                }
            }
        }
        self.mods_check(to_check);
        for l in loaded {
            self.mods_loaded(l);
        }
        if instance_changed {
            if self.page == Page::Instance {
                self.reload_instance();
            }
            if self.mods.busy.is_none() {
                if let Some((source, project)) = self.mods.pending_updates.pop_front() {
                    self.mods_install(source, project, None);
                }
            }
        }
        changed
    }

    pub(super) fn mods_search(&mut self, offset: usize) {
        let m = &mut self.mods;
        m.search += 1;
        m.check_generation.store(m.search, Ordering::Relaxed);
        m.checks
            .retain(|_, state| !matches!(state, Checking::Running));
        m.searching = true;
        if offset == 0 {
            m.results.clear();
            m.more = false;
            m.results_list.reset();
            m.installed_list.reset();
        }
        let query = Query {
            everything: m.everything,
            offset,
            limit: PAGE,
            filters: m.browse.filters.clone(),
            ..Query::new(&m.text, m.kind)
        };
        let (tx, search, source) = (m.tx.clone(), m.search, m.source);
        std::thread::spawn(move || {
            let result = browser::source(source)
                .ok_or_else(|| format!("{} isn't set up.", source.label()))
                .and_then(|s| s.search(&query));
            let about: HashMap<String, Vec<String>> = match (&result, source) {
                (Ok(found), SourceId::Horizon) => found
                    .iter()
                    .map(|p| (p.id.clone(), horizon::trust_and_capabilities(&p.id)))
                    .collect(),
                _ => HashMap::new(),
            };
            let _ = tx.send(Event::Found(search, offset, result));
            if !about.is_empty() {
                let _ = tx.send(Event::About(search, about));
            }
        });
    }

    /// Reads the registry's index again when something installed came from it, and turns off
    /// what it took down since (`horizon::turn_off_revoked`).
    pub(super) fn mods_taken_down(&mut self) {
        if !self
            .mods
            .installed
            .iter()
            .any(|e| e.source == SourceId::Horizon)
        {
            return;
        }
        let tx = self.mods.tx.clone();
        let home = self.mods.home.clone();
        std::thread::spawn(move || {
            let Ok(index) = horizon::index() else {
                return;
            };
            let lines = horizon::turn_off_revoked(&index, &home);
            let _ = tx.send(Event::TakenDown(home, lines));
        });
    }

    /// Turns a Horizon mod the browser installed on or off, as the game's Options > Mods does.
    fn mods_switch(&mut self, e: &installed::Entry) {
        let home = self.mods.home.clone();
        let Some(id) = horizon::mod_id(e) else {
            return;
        };
        let on = !java::is_enabled(&home, id);
        let revoked = horizon::known_index().and_then(|index| {
            let hash = e.checksum.trim_start_matches("SHA-256:");
            index.revoked(&e.project, hash).map(|r| r.reason.clone())
        });
        self.mods.message = Some(match revoked.filter(|_| on) {
            Some(reason) => Err(format!("{} was taken down: {reason}", e.title)),
            None => java::set_enabled(&home, id, on).map(|()| {
                format!(
                    "{} is {}. The game reads this when it starts.",
                    e.title,
                    if on { "on" } else { "off" }
                )
            }),
        });
    }

    /// Looks for newer versions of what the browser installed.
    pub(super) fn mods_check_updates(&mut self) {
        let entries = self.mods.installed.clone();
        if entries.is_empty() {
            return;
        }
        let tx = self.mods.tx.clone();
        let home = self.mods.home.clone();
        std::thread::spawn(move || {
            let mut found = HashMap::new();
            for e in entries {
                let Some(source) = browser::source(e.source) else {
                    continue;
                };
                if let Ok(Some(v)) = installed::update_for(&*source, &e) {
                    found.insert((e.source, e.project.clone()), v.number);
                }
            }
            let _ = tx.send(Event::Updates(home, found));
        });
    }

    /// Installs (or updates) a project on a thread: a pack into its folder, a modpack into
    /// Modpacks. `known` is the project when the page has it; else it is asked for first.
    pub(super) fn mods_install(&mut self, source: SourceId, id: String, known: Option<Project>) {
        if self.mods.busy.is_some() {
            return;
        }
        self.mods.message = None;
        self.mods.busy = Some((id.clone(), format!("Asking {}…", source.label()), 0.));
        self.mods.updates.remove(&(source, id.clone()));
        let everything = self.mods.everything || self.mods.browse.filters.other_version();
        // A modpack is ported for the selected version as soon as it is imported.
        let resources = self
            .selected()
            .filter(|tag| backend::is_prepared(tag))
            .map(|tag| backend::resources(tag));
        let tx = self.mods.tx.clone();
        let home = self.mods.home.clone();
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(-1);
            let progress = |line: &str, f: f32| {
                let pct = (f * 100.) as i32;
                if last.replace(pct) != pct {
                    let _ = tx.send(Event::Progress(line.to_string(), f));
                }
            };
            let mut modpack = false;
            let result = browser::source(source)
                .ok_or_else(|| format!("{} isn't set up.", source.label()))
                .and_then(|source| {
                    let p = match known {
                        Some(p) => p,
                        None => source.project(&id)?,
                    };
                    modpack = p.kind == PackKind::Modpack;
                    if modpack && p.source == SourceId::Horizon {
                        let pack = horizon::pack::install(&p.id, &progress)?;
                        horizon_launcher::content_icons::remember_project_icon(
                            &pack.work,
                            p.icon.as_deref(),
                        )?;
                        return Ok(format!(
                            "Installed {} {} as its own profile with {}, each checked. Play it \
                             from the list above.",
                            pack.name,
                            pack.version,
                            pack.mods.join(", ")
                        ));
                    }
                    if modpack {
                        let folder = modpacks::ports_dir().join("downloads");
                        let file = installed::fetch_modpack(&*source, &p, &folder, &|f| {
                            progress(&p.title, f)
                        })?;
                        let log = |line: String| progress(&line, 1.);
                        let pack = modpacks::import_and_port(&file, resources.as_deref(), &log)?;
                        horizon_launcher::content_icons::remember_project_icon(
                            &pack.work,
                            p.icon.as_deref(),
                        )?;
                        return Ok(format!(
                            "{} Play it from the list above.",
                            modpacks::outcome(&pack)
                        ));
                    }
                    let done = installed::install(&*source, &p, everything, &home, &progress)?;
                    let names: Vec<String> = done
                        .iter()
                        .map(|e| format!("{} {}", e.title, e.number))
                        .collect();
                    if p.source == SourceId::Horizon && p.kind == PackKind::Mod {
                        return Ok(format!(
                            "Installed {} into horizon-mods and turned it on, checksum checked \
                             (Options > Mods in the game).",
                            names.join(", ")
                        ));
                    }
                    if p.kind == PackKind::Mod {
                        let skipped = done.last().map_or(0, |e| e.skipped.len());
                        return Ok(format!(
                            "Ported {} into horizon-mods and turned on (Options > Mods in the \
                             game).{}",
                            names.join(", "),
                            if skipped > 0 {
                                " What didn't port is listed under Installed."
                            } else {
                                ""
                            }
                        ));
                    }
                    Ok(format!(
                        "Installed {}, checksums checked. Turn it on in the game: Options > \
                         Resource Packs.",
                        names.join(", ")
                    ))
                });
            let _ = tx.send(Event::Done(result, modpack));
        });
    }

    pub(super) fn mods_field(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let m = &self.mods;
        let focused = m.focused;
        let shown: String = {
            let chars: Vec<char> = m.text.chars().collect();
            chars[chars.len().saturating_sub(18)..].iter().collect()
        };
        div()
            .id("mods-search")
            .track_focus(&m.focus)
            .flex()
            .items_center()
            .gap(px(10.))
            .w(px(170.))
            .h(px(theme::FIELD + 4.))
            .px(px(12.))
            .overflow_hidden()
            .border_2()
            .border_color(if focused { t.accent } else { t.outline })
            .bg(t.well)
            .text_size(px(theme::UI.px))
            .cursor_text()
            .child(blocks::icon("search", 18., t.muted))
            .child(if m.text.is_empty() && !focused {
                div().text_color(t.muted).child("Search…")
            } else {
                div().text_color(t.ink).child(shown)
            })
            .when(focused, |d| {
                d.child(div().flex_none().w(px(1.)).h(px(16.)).bg(t.ink))
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.mods.focus);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                let command = k.modifiers.control || k.modifiers.platform;
                match type_key(
                    &mut this.mods.text,
                    false,
                    &k.key,
                    k.key_char.as_deref(),
                    command,
                ) {
                    Key::Enter => {
                        if this.mods.ui.view == browser_controls::LibraryView::Browse {
                            this.mods_search(0);
                        } else {
                            this.mods.reset_lists();
                        }
                    }
                    Key::Escape => window.blur(),
                    Key::Paste => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                            let line = text.lines().next().unwrap_or_default().trim().to_string();
                            this.mods.text.push_str(&line);
                        }
                    }
                    Key::Edited => {}
                    Key::Pass => return,
                }
                if let Some((at, _)) = this.mods.text.char_indices().nth(100) {
                    this.mods.text.truncate(at);
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    pub(super) fn mods_page(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        if !self.mods.loaded {
            self.mods.loaded = true;
            self.mods.installed = installed::Installed::load(&self.mods.home).entries;
            self.mods_check_updates();
            self.mods_taken_down();
            self.mods_search(0);
        }
        let entity = cx.entity().downgrade();
        let kinds = blocks::tabs(
            "mods-kind",
            &KINDS,
            self.mods.kind,
            &t,
            move |kind, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.mods.kind != kind {
                        this.mods.kind = kind;
                        this.mods.browse.filters = Default::default();
                        this.mods.browse.open = None;
                        this.mods.ui.popup = None;
                        this.mods_search(0);
                    }
                    cx.notify();
                });
            },
        );
        let mut body = blocks::panel(&t).flex().flex_col().gap(px(16.)).p(px(20.));
        if let Some(project) = self.mods_project_page(cx) {
            return div()
                .flex()
                .flex_col()
                .child(kinds)
                .child(body.children(self.mods_busy_plate()).child(project));
        }
        body = body
            .child(self.browser_navigation(cx))
            .children(self.mods_busy_plate());
        if self.mods.ui.view == browser_controls::LibraryView::Installed {
            if self.mods.kind == PackKind::Modpack {
                body = body.child(self.imported_packs(cx));
            } else {
                body = body.child(self.mods_field(cx));
                body = match self.mods_installed_list(cx) {
                    Some(list) => body.child(list),
                    None => body.child(centered(
                        "No installed content matches this view. Browse to add content.",
                        &t,
                    )),
                };
            }
        } else {
            body = body
                .child(self.browser_toolbar(cx))
                .child(self.mods_results(cx));
        }
        div().flex().flex_col().child(kinds).child(body)
    }

    /// The install under way and the last install's answer.
    pub(super) fn mods_busy_plate(&self) -> Vec<Div> {
        let t = self.theme;
        let mut out = Vec::new();
        if let Some((_, line, f)) = &self.mods.busy {
            out.push(
                plate(&t)
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(16.))
                    .child(caps("Installing").text_color(t.accent))
                    .child(
                        text(theme::CAPTION)
                            .font_family(theme::MONO)
                            .text_color(t.ink_soft)
                            .truncate()
                            .child(format!("{line}  {:.0}%", f * 100.)),
                    ),
            );
        }
        match &self.mods.message {
            Some(Ok(done)) => out.push(
                plate(&t)
                    .p(px(16.))
                    .text_size(px(theme::UI.px))
                    .text_color(t.ink)
                    .child(done.clone()),
            ),
            Some(Err(e)) => out.push(notice(e.clone(), &t)),
            None => {}
        }
        out
    }

    fn mods_installed_list(&self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.theme;
        let kind = self.mods.kind;
        let entries: Vec<&installed::Entry> = self
            .mods
            .installed
            .iter()
            .filter(|e| e.kind == kind)
            .collect();
        if entries.is_empty() {
            return None;
        }
        let busy = self.mods.busy.is_some();
        let row = |i: usize| {
            let e = entries[i];
            let key = (e.source, e.project.clone());
            let mut right = div().flex().items_center().gap(px(8.));
            if let Some(number) = self.mods.updates.get(&key) {
                let mut update = button(
                    SharedString::from(format!("mods-update-{}", e.project)),
                    format!("Update to {number}"),
                    Kind::Secondary,
                    &t,
                );
                if busy {
                    update = disabled(update);
                } else {
                    let (source, id) = key.clone();
                    update = update.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.mods_install(source, id.clone(), None);
                        cx.notify();
                    }));
                }
                right = right.child(update);
            }
            let confirming = self.mods.confirm_remove.as_deref() == Some(e.project.as_str());
            let id = e.project.clone();
            let remove = button(
                SharedString::from(format!("mods-remove-{}", e.project)),
                if confirming {
                    "Remove for good"
                } else {
                    "Remove"
                },
                if confirming {
                    Kind::Secondary
                } else {
                    Kind::Ghost
                },
                &t,
            )
            .when(confirming, |b| b.text_color(t.danger))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if this.mods.confirm_remove.as_ref() == Some(&id) {
                    let home = backend::game_home();
                    this.mods.message = Some(
                        installed::remove(&home, &id).map(|e| format!("Removed {}.", e.title)),
                    );
                    this.mods.installed = installed::Installed::load(&home).entries;
                    this.mods.installed_list.reset();
                    this.mods.confirm_remove = None;
                } else {
                    this.mods.confirm_remove = Some(id.clone());
                }
                cx.notify();
            }));
            if e.source == SourceId::Horizon && e.kind == PackKind::Mod {
                let on = horizon::mod_id(e)
                    .is_some_and(|id| java::is_enabled(&backend::game_home(), id));
                let entry = e.clone();
                right = right.child(
                    div()
                        .id(SharedString::from(format!("mods-switch-{}", e.project)))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        // Its id sits under the row's own, so one name serves every row.
                        .child(switch("mods-switch-knob", on, &t))
                        .child(text(theme::CAPTION).text_color(t.ink_soft).child(if on {
                            "On"
                        } else {
                            "Off"
                        }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.mods_switch(&entry);
                            cx.notify();
                        })),
                );
            }
            right = right.child(remove);
            let detail = div()
                .flex()
                .flex_col()
                .child(format!(
                    "{}{} · from {}{} · {}",
                    e.verdict
                        .as_ref()
                        .map(|v| format!("{v} · "))
                        .unwrap_or_default(),
                    e.number,
                    e.source.label(),
                    if e.dependency {
                        ", needed by another pack"
                    } else {
                        ""
                    },
                    e.file
                ))
                .children(e.skipped.iter().map(|s| {
                    div()
                        .text_color(t.ink_soft)
                        .child(format!("Not ported: {s}"))
                }))
                .child(
                    div()
                        .font_family(theme::MONO)
                        .child(format!("checked: {}", short_sum(&e.checksum))),
                );
            index_row(
                i + 1,
                e.title.clone(),
                Some(detail.into_any_element()),
                Some(right.into_any_element()),
                &t,
            )
        };
        let list = self.mods.installed_list.column(
            &self.content_scroll.handle,
            entries.len(),
            8.,
            |index| self.mods.confirm_remove.as_ref() == Some(&entries[index].project),
            row,
        );
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(caps("Installed").text_color(t.muted))
                .child(list),
        )
    }

    fn mods_results(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let result_count = self.mods.results.len();
        let rows = self.mods.results_list.clone();
        let scroll = self.content_scroll.handle.clone();
        let mut out = div().flex().flex_col().gap(px(12.));
        if let Some(error) = &self.mods.error {
            out = out.child(notice(error.clone(), &t));
        } else if result_count == 0 {
            let label = if self.mods.searching || !self.mods.searched {
                "Searching…"
            } else {
                "No projects match these filters."
            };
            out = out.child(centered(label, &t));
        } else {
            out = out.child(rows.column(
                &scroll,
                result_count,
                8.,
                |_| false,
                |index| {
                    let project = self.mods.results[index].clone();
                    self.content_browser_card(&project, cx)
                },
            ));
        }
        if self.mods.more {
            let mut more = button("mods-more", "More", Kind::Ghost, &t);
            if self.mods.searching {
                more = disabled(more);
            } else {
                let offset = self.mods.results.len();
                more = more.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.mods_search(offset);
                    cx.notify();
                }));
            }
            out = out.child(div().flex().pt(px(16.)).child(more));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_checksums_read_short() {
        assert_eq!(count(999), "999");
        assert_eq!(count(1_500), "1.5k");
        assert_eq!(count(51_404_735), "51.4M");
        let sum = format!("SHA-512:{}", "ab".repeat(64));
        assert_eq!(short_sum(&sum), "SHA-512 abababababab…abababab");
        assert_eq!(short_sum("SHA-1:abcd"), "SHA-1:abcd");
    }
}
