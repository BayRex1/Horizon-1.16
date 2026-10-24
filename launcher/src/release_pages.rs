//! Installed/available builds and a release reader with a prominent sponsorship action.
use super::*;

#[derive(Default)]
pub(super) struct ReleaseUi {
    pub(super) installed: cull::Cull,
    pub(super) available: cull::Cull,
    note: Option<String>,
    notes_scroll: Scrollbar,
}

impl Launcher {
    pub(super) fn versions_page(&mut self, cx: &mut Context<Self>) -> Div {
        let remote = match &self.releases {
            Load::Ready(releases) => releases.as_slice(),
            _ => &[],
        };
        let installed = self
            .installed
            .iter()
            .map(|tag| {
                remote
                    .iter()
                    .find(|release| &release.tag == tag)
                    .cloned()
                    .unwrap_or_else(|| Release {
                        tag: tag.clone(),
                        name: "Installed build".into(),
                        body: String::new(),
                        date: String::new(),
                        asset: None,
                        prerelease: false,
                    })
            })
            .collect::<Vec<_>>();
        let available = remote
            .iter()
            .filter(|release| !self.installed.contains(&release.tag))
            .cloned()
            .collect::<Vec<_>>();
        let mut page = div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(self.version_group(
                "Installed",
                &installed,
                self.release_ui.installed.clone(),
                cx,
            ))
            .child(self.version_group(
                "Available to install",
                &available,
                self.release_ui.available.clone(),
                cx,
            ));
        match &self.releases {
            Load::Loading => page = page.child(centered("Loading available builds…", &self.theme)),
            Load::Failed(error) => {
                page = page.child(notice(error.clone(), &self.theme)).child(
                    button("retry-releases", "Try again", Kind::Secondary, &self.theme).on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.fetch_releases();
                            cx.notify();
                        }),
                    ),
                )
            }
            Load::Ready(_) => {}
        }
        page
    }

    fn version_group(
        &self,
        title: &'static str,
        releases: &[Release],
        list: cull::Cull,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let rows = list.column(
            &self.content_scroll.handle,
            releases.len(),
            0.,
            |index| self.confirm_remove.as_ref() == Some(&releases[index].tag),
            |index| self.version_row(&releases[index], cx),
        );
        blocks::panel(&t)
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .child(
                blocks::pixel(24., FontWeight::MEDIUM)
                    .text_color(t.ink)
                    .child(title),
            )
            .when(releases.is_empty(), |group| {
                group.child(text(theme::CAPTION).text_color(t.muted).child(
                    if title == "Installed" {
                        "No build installed yet."
                    } else {
                        "No other builds are currently listed."
                    },
                ))
            })
            .child(rows)
    }

    fn version_row(&self, release: &Release, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut metadata = vec![release.date.clone()];
        if let Some((_, size)) = release.asset {
            metadata.push(megabytes(size));
        }
        metadata.retain(|part| !part.is_empty());
        let summary = release
            .name
            .split_once(": ")
            .map_or(release.name.as_str(), |(_, summary)| summary);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .py(px(16.))
            .border_b_1()
            .border_color(t.bevel)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .min_w_0()
                            .flex_1()
                            .child(version_tag(&release.tag).text_color(t.ink))
                            .child(
                                text(theme::CAPTION)
                                    .text_color(t.muted)
                                    .child(summary.to_owned()),
                            )
                            .child(
                                text(theme::CAPTION)
                                    .text_color(t.muted)
                                    .child(metadata.join(" · ")),
                            ),
                    )
                    .child(self.version_actions(release, cx)),
            )
            .children(
                self.install_error
                    .as_ref()
                    .filter(|(tag, _)| tag == &release.tag)
                    .map(|(_, error)| notice(error.clone(), &t)),
            )
    }

    fn version_actions(&self, release: &Release, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let tag = release.tag.clone();
        if let Some((_, progress)) = self
            .installing
            .as_ref()
            .filter(|(installing, _)| installing == &tag)
        {
            return div()
                .flex()
                .items_center()
                .gap(px(10.))
                .w(px(180.))
                .child(progress_bar(Some(self.bar), &t))
                .child(text(theme::MICRO).child(format!("{:.0}%", progress * 100.)))
                .into_any_element();
        }
        if self.installed.contains(&tag) {
            let selected = self.selected() == Some(&tag);
            let confirming = self.confirm_remove.as_ref() == Some(&tag);
            let use_tag = tag.clone();
            let use_build = button(
                format!("use-{tag}"),
                "Use this version",
                Kind::Secondary,
                &t,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings.selected = Some(use_tag.clone());
                this.settings.save();
                cx.notify();
            }));
            let remove = button(
                format!("remove-{tag}"),
                if confirming {
                    "Remove for good"
                } else {
                    "Remove"
                },
                Kind::Ghost,
                &t,
            )
            .when(confirming, |button| button.text_color(t.danger))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if this.confirm_remove.as_ref() == Some(&tag) {
                    this.remove(&tag);
                } else {
                    this.confirm_remove = Some(tag.clone());
                }
                this.release_ui.installed.reset();
                this.release_ui.available.reset();
                cx.notify();
            }));
            return div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .when(selected, |actions| {
                    actions.child(
                        text(theme::CAPTION)
                            .text_color(t.ink_soft)
                            .child("✓ Selected for launch"),
                    )
                })
                .when(!selected, |actions| actions.child(use_build))
                .child(remove)
                .into_any_element();
        }
        if release.asset.is_none() {
            return text(theme::CAPTION)
                .text_color(t.muted)
                .child("No build for this platform")
                .into_any_element();
        }
        let release = release.clone();
        let install = button(
            format!("install-{tag}"),
            "Install build",
            Kind::Secondary,
            &t,
        );
        if self.installing.is_some() {
            return disabled(install).into_any_element();
        }
        install
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.install(&release);
                cx.notify();
            }))
            .into_any_element()
    }

    fn sponsor_spotlight(&self) -> Option<Div> {
        let url = self.links.sponsor.clone();
        if url.is_empty() {
            return None;
        }
        let t = self.theme;
        Some(blocks::panel(&t).bg(t.raised).flex().flex_wrap().items_center().justify_between().gap(px(20.)).p(px(24.))
            .child(div().flex().flex_col().gap(px(8.)).flex_1().min_w(px(220.))
                .child(blocks::pixel(30., FontWeight::MEDIUM).text_color(t.ink).child("Support the work"))
                .child(text(theme::UI).text_color(t.ink_soft).child("Horizon is built in the open. Sponsorship helps fund ongoing development.")))
            .child(button("sponsor", "Sponsor on GitHub", Kind::Primary, &t).h(px(46.)).px(px(22.))
                .on_click(move |_, _, cx| cx.open_url(&url))))
    }

    pub(super) fn notes_page(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let page = div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .children(self.sponsor_spotlight());
        let releases = match &self.releases {
            Load::Ready(releases) if !releases.is_empty() => releases,
            Load::Ready(_) => return page.child(centered("No releases yet.", &t)),
            Load::Loading => return page.child(centered("Loading patch notes…", &t)),
            Load::Failed(error) => {
                return page.child(notice(error.clone(), &t)).child(
                    button("retry-notes", "Try again", Kind::Secondary, &t).on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.fetch_releases();
                            cx.notify();
                        },
                    )),
                )
            }
        };
        let selected = releases
            .iter()
            .find(|release| self.release_ui.note.as_ref() == Some(&release.tag))
            .unwrap_or(&releases[0]);
        let index = div()
            .id("release-index")
            .flex()
            .flex_col()
            .gap(px(6.))
            .w(px(150.))
            .flex_none()
            .max_h(px(500.))
            .overflow_y_scroll()
            .children(releases.iter().map(|release| {
                let tag = release.tag.clone();
                button(format!("note-{tag}"), tag.clone(), Kind::Ghost, &t)
                    .justify_start()
                    .when(release.tag == selected.tag, |button| {
                        button.bg(t.raised).text_color(t.ink)
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.release_ui.note = Some(tag.clone());
                        cx.notify();
                    }))
            }));
        let article = blocks::panel(&t)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(16.))
            .p(px(20.))
            .child(version_tag(&selected.tag).text_color(t.ink))
            .child(
                text(theme::CAPTION)
                    .text_color(t.muted)
                    .child(selected.date.clone()),
            )
            .child(if selected.body.trim().is_empty() {
                text(theme::UI)
                    .child("No notes for this release.")
                    .into_any_element()
            } else {
                markdown::render(&selected.body, &t)
            });
        page.child(
            div()
                .flex()
                .items_start()
                .gap(px(18.))
                .child(self.release_ui.notes_scroll.wrap(index, &t).flex_none())
                .child(article),
        )
    }
}
