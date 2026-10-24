//! The Mods page's project page, search filters and project icons (`horizon_launcher::browser`:
//! `details` for the data, `images` for the icons and gallery). Drawn only with the launcher's
//! shared components, so a restyle of those restyles this.
//!
//! - A result's title opens its page inside the launcher: icon, author, downloads, followers,
//!   licence, how well it runs and Install, then Description (Markdown), Gallery, Versions and
//!   the project's links.
//! - Filters: sort, loader, game version, environment and categories, with the values the
//!   source lists for the kind.
//! - Icons and gallery images come from the source's image servers at run time, cached on disk;
//!   a placeholder stands in until one arrives.
use super::mods_page::{Checking, Event};
use super::*;
use gpui::img;
use horizon_launcher::browser::{
    self,
    details::{Details, Filters, Tags},
    images, java, Kind as PackKind, Project, SourceId, Support, Version,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

/// A project page's tabs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Description,
    Gallery,
    Versions,
}

const TABS: [(Tab, &str); 3] = [
    (Tab::Description, "Description"),
    (Tab::Gallery, "Gallery"),
    (Tab::Versions, "Versions"),
];

/// The open project page.
pub(super) struct Opened {
    pub(super) project: Project,
    tab: Tab,
    details: Option<Result<Arc<Details>, String>>,
    versions: Option<Result<Arc<[ProjectVersion]>, String>>,
    version_list: cull::Cull,
    checked_versions: HashMap<String, horizon_port_pack::check::Verdict>,
}

/// Cached verdicts are read with the version request, never while drawing a frame.
#[derive(Clone)]
pub(super) struct ProjectVersion {
    version: Version,
    verdict: Option<horizon_port_pack::check::Verdict>,
}

/// The project page, filters and images' share of the Mods page's state.
#[derive(Default)]
pub(super) struct Browse {
    pub(super) open: Option<Opened>,
    pub(super) filters: Filters,
    tags: HashMap<(SourceId, PackKind), Tags>,
    tags_asked: HashSet<(SourceId, PackKind)>,
    pub(super) tag_errors: HashMap<(SourceId, PackKind), String>,
}

/// What the background sends this module.
pub(super) enum Loaded {
    Details((SourceId, String), Result<Details, String>),
    Versions((SourceId, String), Result<Vec<ProjectVersion>, String>),
    Checked(
        (SourceId, String),
        String,
        horizon_port_pack::check::Verdict,
    ),
    Tags((SourceId, PackKind), Result<Tags, String>),
}

impl Launcher {
    pub(super) fn mods_loaded(&mut self, loaded: Loaded) {
        let b = &mut self.mods.browse;
        fn open<'a>(b: &'a mut Browse, key: &(SourceId, String)) -> Option<&'a mut Opened> {
            b.open
                .as_mut()
                .filter(|o| (o.project.source, &o.project.id) == (key.0, &key.1))
        }
        match loaded {
            Loaded::Details(key, d) => {
                if let Some(o) = open(b, &key) {
                    o.details = Some(d.map(Arc::new));
                }
            }
            Loaded::Versions(key, v) => {
                if let Some(o) = open(b, &key) {
                    o.versions = Some(v.map(Arc::from));
                    o.apply_checks();
                    o.version_list.reset();
                }
            }
            Loaded::Checked(key, version, verdict) => {
                if let Some(o) = open(b, &key) {
                    o.checked_versions.insert(version, verdict);
                    o.apply_checks();
                }
            }
            Loaded::Tags(key, Ok(tags)) => {
                b.tag_errors.remove(&key);
                b.tags.insert(key, tags);
            }
            Loaded::Tags(key, Err(error)) => {
                b.tag_errors.insert(key, error);
            }
        }
    }

    /// Opens a project's page and asks its source for the rest of it.
    pub(super) fn mods_open(&mut self, project: Project) {
        let key = (project.source, project.id.clone());
        let tx = self.mods.tx.clone();
        let p = project.clone();
        let search = self.mods.search;
        std::thread::spawn(move || {
            if p.source == SourceId::Horizon {
                let about = browser::horizon::trust_and_capabilities(&p.id);
                let _ = tx.send(Event::About(search, HashMap::from([(p.id.clone(), about)])));
            }
            let Some(source) = browser::source(p.source) else {
                return;
            };
            let d = source.details(&p);
            let _ = tx.send(Event::Loaded(Loaded::Details(key.clone(), d)));
            let v = source.versions(&p).map(|versions| {
                let home = backend::game_home();
                versions
                    .into_iter()
                    .map(|version| {
                        let verdict = version.file().and_then(|file| {
                            (p.kind == PackKind::Mod)
                                .then(|| java::cached(&home, file))
                                .flatten()
                                .map(|check| check.verdict)
                        });
                        ProjectVersion { version, verdict }
                    })
                    .collect()
            });
            let _ = tx.send(Event::Loaded(Loaded::Versions(key, v)));
        });
        self.mods.browse.open = Some(Opened {
            project,
            tab: Tab::Description,
            details: None,
            versions: None,
            version_list: cull::Cull::default(),
            checked_versions: HashMap::new(),
        });
    }

    fn mods_image(&mut self, url: &str, size: u32) -> Option<PathBuf> {
        self.remote_art(url, size)
    }

    /// A project's icon, `side` pixels: its image, or its first letter on a plate until then.
    pub(super) fn mods_icon(&mut self, p: &Project, side: f32) -> AnyElement {
        let t = self.theme;
        let path = p
            .icon
            .clone()
            .and_then(|url| self.mods_image(&url, images::ICON));
        blocks::slot(path.map(gpui::ImageSource::from), &p.title, side, &t).into_any_element()
    }

    /// The values the filters can take for the current kind and source, asked for once.
    pub(super) fn mods_tags(&mut self) -> Option<&Tags> {
        let key = (self.mods.source, self.mods.kind);
        if self.mods.browse.tags_asked.insert(key) {
            let tx = self.mods.tx.clone();
            std::thread::spawn(move || {
                let tags = browser::source(key.0)
                    .ok_or_else(|| "not set up".to_string())
                    .and_then(|s| s.tags(key.1));
                let _ = tx.send(Event::Loaded(Loaded::Tags(key, tags)));
            });
        }
        self.mods.browse.tags.get(&key)
    }

    pub(super) fn retry_mods_tags(&mut self) {
        let key = (self.mods.source, self.mods.kind);
        self.mods.browse.tags_asked.remove(&key);
        self.mods.browse.tag_errors.remove(&key);
    }

    /// The open project's page.
    pub(super) fn mods_project_page(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.theme;
        let o = self.mods.browse.open.as_ref()?;
        let p = o.project.clone();
        let tab = o.tab;
        let support = self.mods.support(&p);
        let color = match support {
            Support::Works => t.success,
            Support::Partial(_) => t.warn,
            Support::No(_) => t.muted,
        };
        let back = button("mods-back", "Back to results", Kind::Ghost, &t).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.mods.browse.open = None;
                cx.notify();
            },
        ));
        let details = o.details.clone();
        let versions = o.versions.clone();
        let version_list = o.version_list.clone();
        let mut meta = vec![];
        if p.source != SourceId::Horizon {
            meta.push(format!("{} downloads", thousands(p.downloads)));
        }
        if let Some(Ok(d)) = &details {
            if p.source != SourceId::Horizon {
                meta.push(format!("{} followers", thousands(d.followers)));
            }
            if let Some(day) = d.updated.get(..10) {
                meta.push(format!("updated {day}"));
            }
        }
        if !p.license.is_empty() {
            meta.push(p.license.clone());
        }
        meta.push(format!("from {}", p.source.label()));
        let authors = match &details {
            Some(Ok(d)) if !d.authors.is_empty() => d.authors.join(", "),
            _ => p.author.clone(),
        };
        let icon = self.mods_icon(&p, 72.);
        let busy = self.mods.busy.as_ref().map(|b| b.0.clone());
        let installed = self.content_is_installed(&p);
        let checking = matches!(
            self.mods.checks.get(&(p.source, p.id.clone())),
            None | Some(Checking::Running)
        ) && p.kind == PackKind::Mod
            && p.source != SourceId::Horizon
            && p.support().installable();
        let mut action = div().flex().items_center().gap(px(8.)).child(
            caps(if checking {
                "Checking"
            } else {
                support.label()
            })
            .flex_none()
            .text_color(color),
        );
        if busy.as_deref() == Some(p.id.as_str()) {
            action = action.child(caps("Installing").text_color(t.accent));
        } else if installed {
            action = action.child(caps("Installed").text_color(t.muted));
        } else if support.installable() && !checking {
            let label = if p.kind == PackKind::Modpack && p.source != SourceId::Horizon {
                "Import"
            } else {
                "Install"
            };
            let mut install = button("mods-page-install", label, Kind::Primary, &t);
            if busy.is_some()
                || (p.source == SourceId::Horizon && !self.mods.about.contains_key(&p.id))
            {
                install = disabled(install);
            } else {
                let p = p.clone();
                install = install.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.mods_install(p.source, p.id.clone(), Some(p.clone()));
                    cx.notify();
                }));
            }
            action = action.child(install);
        }
        let head = div()
            .flex()
            .items_center()
            .gap(px(20.))
            .child(icon)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(text(theme::TITLE).text_color(t.ink).child(p.title.clone()))
                    .child(
                        text(theme::UI)
                            .text_color(t.muted)
                            .child(if authors.is_empty() {
                                p.summary.clone()
                            } else {
                                format!("by {authors} · {}", p.summary)
                            }),
                    )
                    .child(
                        text(theme::CAPTION)
                            .text_color(t.muted)
                            .child(meta.join(" · ")),
                    ),
            )
            .child(action);
        let mut page = div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(div().flex().child(back))
            .child(head);
        if p.source == SourceId::Horizon {
            let lines = self
                .mods
                .about
                .get(&p.id)
                .cloned()
                .unwrap_or_else(|| vec!["Reading registry trust and capabilities…".into()]);
            page = page.child(
                plate(&t).p(px(12.)).child(
                    text(theme::CAPTION)
                        .text_color(t.ink_soft)
                        .child(lines.join(" · ")),
                ),
            );
        }
        if let Some(why) = support.reason() {
            page = page.child(
                text(theme::UI)
                    .text_color(t.ink_soft)
                    .child(why.to_string()),
            );
        }
        if let Some(Checking::Done(c)) = self.mods.checks.get(&(p.source, p.id.clone())) {
            let lines = |title: &str, list: &[String]| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(caps(title).text_color(t.muted))
                    .children(
                        list.iter()
                            .map(|l| text(theme::CAPTION).text_color(t.ink_soft).child(l.clone())),
                    )
            };
            let mut what = div().flex().gap(px(32.));
            if !c.ports.is_empty() {
                what = what.child(lines("Ports", &c.ports));
            }
            if !c.skipped.is_empty() {
                what = what.child(lines("Doesn't port", &c.skipped));
            }
            page = page.child(what);
        }
        if let Some(Ok(d)) = &details {
            let mut links = div().flex().flex_wrap().gap(px(8.));
            let page_url = p.page.clone();
            if !page_url.is_empty() {
                links = links.child(
                    button(
                        "mods-link-page",
                        format!("{} page", p.source.label()),
                        Kind::Ghost,
                        &t,
                    )
                    .on_click(move |_, _, cx| cx.open_url(&page_url)),
                );
            }
            for (i, l) in d.links.iter().enumerate() {
                let url = l.url.clone();
                links = links.child(
                    button(format!("mods-link-{i}"), l.label.clone(), Kind::Ghost, &t)
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            page = page.child(links);
        }
        let this = cx.entity().downgrade();
        page = page.child(div().flex().child(segments("mods-tab", &TABS, tab, &t, {
            move |value, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    if let Some(o) = &mut this.mods.browse.open {
                        o.tab = value;
                    }
                    cx.notify();
                });
            }
        })));
        let body = match tab {
            Tab::Description => match &details {
                None => centered("Loading…", &t),
                Some(Err(e)) => notice(e.clone(), &t),
                Some(Ok(d)) => div().max_w(px(760.)).child(markdown::render(&d.body, &t)),
            },
            Tab::Gallery => match &details {
                None => centered("Loading…", &t),
                Some(Err(e)) => notice(e.clone(), &t),
                Some(Ok(d)) if d.gallery.is_empty() => centered("No gallery images.", &t),
                Some(Ok(d)) => {
                    let mut grid = div().flex().flex_wrap().gap(px(16.));
                    for g in &d.gallery {
                        let shot = match self.mods_image(&g.url, images::GALLERY) {
                            Some(path) => img(path).w(px(360.)).h(px(203.)).into_any_element(),
                            None => plate(&t).w(px(360.)).h(px(203.)).into_any_element(),
                        };
                        grid = grid.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .w(px(360.))
                                .child(plate(&t).overflow_hidden().child(shot))
                                .when(!g.title.is_empty(), |d| {
                                    d.child(
                                        text(theme::CAPTION)
                                            .text_color(t.ink_soft)
                                            .child(g.title.clone()),
                                    )
                                }),
                        );
                    }
                    grid
                }
            },
            Tab::Versions => match &versions {
                None => centered("Loading…", &t),
                Some(Err(e)) => notice(e.clone(), &t),
                Some(Ok(list)) => version_list.column(
                    &self.content_scroll.handle,
                    list.len().min(50),
                    8.,
                    |_| false,
                    |i| {
                        let v = &list[i].version;
                        let file = v.file();
                        let mut detail = vec![
                            v.published.get(..10).unwrap_or_default().to_string(),
                            v.loaders.join(", "),
                        ];
                        let games: Vec<&str> = v
                            .game_versions
                            .iter()
                            .rev()
                            .take(4)
                            .map(String::as_str)
                            .collect();
                        detail.push(games.join(", "));
                        if let Some(f) = file {
                            detail.push(format!("{} · {:.1} MB", f.name, f.size as f64 / 1e6));
                        }
                        index_row(
                            i + 1,
                            v.number.clone(),
                            Some(detail.join(" · ").into_any_element()),
                            list[i].verdict.as_ref().map(|verdict| {
                                caps(verdict.label()).text_color(t.muted).into_any_element()
                            }),
                            &t,
                        )
                    },
                ),
            },
        };
        Some(page.child(body))
    }
}

impl Opened {
    /// A check can finish before or after the version request; retain either event order.
    fn apply_checks(&mut self) {
        let Some(Ok(versions)) = &mut self.versions else {
            return;
        };
        for row in Arc::make_mut(versions) {
            if let Some(verdict) = self.checked_versions.get(&row.version.id) {
                row.verdict = Some(*verdict);
            }
        }
    }
}
