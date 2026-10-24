//! Imported modpacks, on the Mods page's Modpacks tab: import a pack, read its coverage report,
//! port what this computer can and play it as its own profile (`horizon_launcher::modpacks`).
use super::*;
use horizon_launcher::browser::horizon::pack as horizon_pack;
use horizon_launcher::modpacks::{self, Imported};

/// Lines of the pipeline's log the page shows while it works.
const LOG_SHOWN: usize = 6;

impl Launcher {
    fn open_pack(&self) -> Option<&Imported> {
        let slug = self.pack_open.as_ref()?;
        self.packs.iter().find(|p| &p.slug == slug)
    }

    /// Runs a pipeline step on a thread; its lines and its end arrive as messages.
    fn pack_job(
        &mut self,
        job: impl FnOnce(&dyn Fn(String)) -> Result<Imported, String> + Send + 'static,
    ) {
        if self.pack_busy {
            return;
        }
        self.pack_busy = true;
        self.pack_log.clear();
        self.pack_error = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let lines = tx.clone();
            let log = move |line: String| {
                let _ = lines.send(Msg::PackLine(line));
            };
            let result = job(&log);
            let _ = tx.send(Msg::PackDone(result.map(Box::new)));
        });
    }

    /// A finished step: the pack's new state, opened.
    pub(super) fn pack_done(&mut self, result: Result<Box<Imported>, String>) {
        self.pack_busy = false;
        match result {
            Ok(pack) => {
                self.mods.report_list.reset();
                self.pack_open = Some(pack.slug.clone());
                self.packs.retain(|p| p.slug != pack.slug);
                self.packs.push(*pack);
                self.packs.sort_by_key(|p| p.title().to_ascii_lowercase());
            }
            Err(e) => self.pack_error = Some(e),
        }
        if self.page == Page::Instance {
            self.reload_instance();
        }
    }

    fn choose_pack(&mut self, folder: bool, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = paths.await {
                if let Some(path) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        this.pack_job(move |log| modpacks::import(&path, log));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Ports the open pack; `from` is a folder of the player's jars, chosen when `ask`.
    pub(super) fn port_pack(&mut self, ask: bool, cx: &mut Context<Self>) {
        let Some(work) = self.open_pack().map(|p| p.work.clone()) else {
            return;
        };
        let tag = if self.page == Page::Instance {
            self.instance_build()
        } else {
            self.selected().cloned()
        };
        let resources = tag
            .filter(|tag| backend::is_prepared(tag))
            .map(|tag| backend::resources(&tag));
        if !ask {
            self.pack_job(move |log| modpacks::port(&work, resources.as_deref(), None, log));
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Use these jars".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = paths.await {
                if let Some(from) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        this.pack_job(move |log| {
                            modpacks::port(&work, resources.as_deref(), Some(&from), log)
                        });
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Reads the imported packs, once (the sidebar and the Modpacks tab both show them).
    pub(super) fn load_packs(&mut self) {
        if self.packs_loaded.is_none() {
            self.mods.report_list.reset();
            self.packs = modpacks::list();
            self.registry_packs = horizon_pack::list();
            match instances::list(&backend::game_home()) {
                Ok(local) => self.local_instances = local,
                Err(error) => self.client_error = Some(error),
            }
            if self.pack_open.is_none() {
                self.pack_open = self.packs.first().map(|p| p.slug.clone());
            }
            self.packs_loaded = Some(Instant::now());
        }
    }

    /// The imported packs, above the Modpacks tab's search.
    pub(super) fn imported_packs(&mut self, cx: &mut Context<Self>) -> Div {
        self.load_packs();
        let t = self.theme;
        let mut page = div().flex().flex_col().gap(px(16.));
        let mut actions = div().flex().flex_wrap().gap(px(8.));
        for (id, label, folder) in [
            ("import-file", "Import a pack file…", false),
            ("import-folder", "Import an instance folder…", true),
        ] {
            let action = button(id, label, Kind::Secondary, &t);
            actions = actions.child(if self.pack_busy {
                disabled(action)
            } else {
                action.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.choose_pack(folder, cx);
                    cx.notify();
                }))
            });
        }
        page=page.child(actions).child(text(theme::CAPTION).text_color(t.muted).child("Import a Modrinth .mrpack, CurseForge .zip, or an existing instance folder. Each pack keeps its own worlds and options."));
        if self.pack_busy || !self.pack_log.is_empty() {
            page = page.child(self.pack_log_plate(&t));
        }
        if let Some(error) = &self.pack_error {
            page = page.child(notice(error.clone(), &t));
        }
        for pack in &self.packs {
            let slug = pack.slug.clone();
            let open = button(
                format!("open-pack-{slug}"),
                "Open instance",
                Kind::Secondary,
                &t,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_instance(InstanceId::Imported(slug.clone()));
                cx.notify();
            }));
            page = page.child(index_row(
                0,
                pack.title().to_owned(),
                Some(div().child(pack.summary()).into_any_element()),
                Some(open.into_any_element()),
                &t,
            ));
        }
        if !self.registry_packs.is_empty() {
            page = page.child(self.registry_pack_rows(cx));
        }
        if self.packs.is_empty() && !self.registry_packs.is_empty() {
            return page;
        }
        if self.packs.is_empty() {
            page = page.child(centered(
                "No modpack imported yet. Import one here or use Browse.",
                &t,
            ));
        }
        page
    }

    /// The modpacks installed from Horizon's registry: each plays its own profile.
    fn registry_pack_rows(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut list = div().flex().flex_col();
        for (i, p) in self.registry_packs.iter().enumerate() {
            let remove = p.work.clone();
            let id = p.id.clone();
            let open_id = id.clone();
            let key = format!("horizon:{id}");
            let confirming = self.confirm_remove.as_deref() == Some(key.as_str());
            let right = div()
                .flex()
                .gap(px(8.))
                .child(
                    button(format!("play-pack-{id}"), "Play", Kind::Primary, &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_instance(InstanceId::Registry(open_id.clone()));
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button(
                        format!("remove-pack-{id}"),
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
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            if this.confirm_remove.as_ref() == Some(&key) {
                                match horizon_pack::remove(&remove) {
                                    Ok(()) => this.packs_loaded = None,
                                    Err(e) => this.pack_error = Some(e),
                                }
                                this.confirm_remove = None;
                            } else {
                                this.confirm_remove = Some(key.clone());
                            }
                            cx.notify();
                        },
                    )),
                );
            let detail = format!(
                "{} from Horizon's registry, its own profile: {}",
                p.version,
                p.mods.join(", ")
            );
            list = list.child(index_row(
                i + 1,
                p.name.clone(),
                Some(
                    text(theme::CAPTION)
                        .text_color(t.muted)
                        .child(detail)
                        .into_any_element(),
                ),
                Some(right.into_any_element()),
                &t,
            ));
        }
        list
    }

    pub(super) fn pack_log_plate(&self, t: &Theme) -> Div {
        let start = self.pack_log.len().saturating_sub(LOG_SHOWN);
        plate(t)
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(16.))
            .child(
                caps(if self.pack_busy { "Working" } else { "Done" })
                    .text_color(if self.pack_busy { t.accent } else { t.muted }),
            )
            .children(self.pack_log.iter().skip(start).map(|line| {
                text(theme::CAPTION)
                    .font_family(theme::MONO)
                    .text_color(t.ink_soft)
                    .truncate()
                    .child(line.clone())
            }))
    }
}
