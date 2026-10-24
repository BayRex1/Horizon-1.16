//! Browser cards use the source's artwork and metadata, with Horizon's square Blocks surfaces.
use super::*;
use horizon_launcher::browser::{Kind as ContentKind, Project, Support};

impl Launcher {
    pub(super) fn content_browser_card(
        &mut self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let art = self.mods_icon(project, 64.);
        let image_project = project.clone();
        let icon = keyboard(
            div().id(SharedString::from(format!("content-icon-{}", project.id))),
            &t,
        )
        .flex_none()
        .cursor_pointer()
        .child(art)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.mods_open(image_project.clone());
            cx.notify();
        }));
        let open = project.clone();
        let name = keyboard(
            div().id(SharedString::from(format!("content-open-{}", project.id))),
            &t,
        )
        .min_w_0()
        .cursor_pointer()
        .hover(|style| style.underline())
        .child(
            blocks::pixel(20., FontWeight::MEDIUM)
                .truncate()
                .text_color(t.ink)
                .child(project.title.clone()),
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.mods_open(open.clone());
            cx.notify();
        }));
        let heading = div()
            .flex()
            .items_baseline()
            .gap(px(8.))
            .min_w_0()
            .child(name)
            .when(!project.author.is_empty(), |heading| {
                heading.child(
                    text(theme::CAPTION)
                        .text_color(t.muted)
                        .truncate()
                        .child(format!("by {}", project.author)),
                )
            });
        let support = self.mods.support(project);
        let mut copy = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(heading)
            .child(
                text(theme::UI)
                    .text_color(t.ink_soft)
                    .truncate()
                    .child(project.summary.clone()),
            )
            .child(self.content_project_tags(project));
        if let Some(reason) = support.reason() {
            copy = copy.child(
                text(theme::CAPTION)
                    .text_color(t.muted)
                    .truncate()
                    .child(reason.to_owned()),
            );
        }
        if project.source == horizon_launcher::browser::SourceId::Horizon {
            if let Some(lines) = self.mods.about.get(&project.id) {
                copy = copy.child(
                    text(theme::CAPTION)
                        .text_color(t.ink_soft)
                        .child(lines.join(" · ")),
                );
            }
        }
        let statistics = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(12.))
            .child(card_metric(
                "download",
                super::mods_page::count(project.downloads),
                &t,
            ))
            .child(card_metric(
                "heart",
                super::mods_page::count(project.metadata.followers),
                &t,
            ));
        let mut right = div()
            .flex_none()
            .w(px(235.))
            .flex()
            .flex_col()
            .items_end()
            .gap(px(10.))
            .when(
                project.source != horizon_launcher::browser::SourceId::Horizon,
                |right| right.child(statistics),
            );
        if let Some(day) = project.metadata.updated.get(..10) {
            right = right.child(card_metric("clock", format!("Updated {day}"), &t));
        }
        right = right.child(self.content_project_actions(project, support, cx));
        blocks::panel(&t)
            .flex()
            .items_start()
            .gap(px(16.))
            .p(px(16.))
            .mb(px(8.))
            .child(icon)
            .child(copy)
            .child(right)
    }

    fn content_project_tags(&self, project: &Project) -> Div {
        let t = self.theme;
        let mut labels = vec![];
        if project.metadata.client {
            labels.push("Client".to_owned());
        }
        if project.metadata.server {
            labels.push("Server".to_owned());
        }
        labels.extend(project.loaders.iter().take(4).map(|category| {
            let mut text = category.replace('-', " ");
            if let Some(first) = text.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            text
        }));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .children(labels.into_iter().map(|label| {
                text(theme::CAPTION)
                    .text_color(t.ink_soft)
                    .px(px(7.))
                    .py(px(2.))
                    .bg(t.raised)
                    .border_1()
                    .border_color(t.bevel)
                    .child(label)
            }))
    }

    fn content_project_actions(
        &self,
        project: &Project,
        support: Support,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let color = match &support {
            Support::Works => t.success,
            Support::Partial(_) => t.warn,
            Support::No(_) => t.muted,
        };
        let mut actions = div().flex().items_center().gap(px(6.)).child(blocks::badge(
            support.label(),
            color,
            &t,
        ));
        if !project.page.is_empty() {
            let url = project.page.clone();
            actions = actions.child(
                button(
                    format!("content-page-{}", project.id),
                    "Page",
                    Kind::Ghost,
                    &t,
                )
                .on_click(move |_, _, cx| cx.open_url(&url)),
            );
        }
        if self
            .mods
            .busy
            .as_ref()
            .is_some_and(|busy| busy.0 == project.id)
        {
            return actions.child(caps("Installing").text_color(t.accent));
        }
        if self.content_is_installed(project) {
            return actions.child(caps("Installed").text_color(t.muted));
        }
        let checking = project.source != horizon_launcher::browser::SourceId::Horizon
            && project.kind == ContentKind::Mod
            && project.support().installable()
            && matches!(
                self.mods.checks.get(&(project.source, project.id.clone())),
                None | Some(mods_page::Checking::Running)
            );
        if checking {
            return actions.child(caps("Checking").text_color(t.muted));
        }
        if !support.installable() {
            return actions;
        }
        let label = if project.kind == ContentKind::Modpack
            && project.source != horizon_launcher::browser::SourceId::Horizon
        {
            "Import"
        } else {
            "Install"
        };
        let install = button(
            format!("content-install-{}", project.id),
            label,
            Kind::Secondary,
            &t,
        );
        if self.mods.busy.is_some()
            || (project.source == horizon_launcher::browser::SourceId::Horizon
                && !self.mods.about.contains_key(&project.id))
        {
            return actions.child(disabled(install));
        }
        let project = project.clone();
        actions.child(
            install.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.mods_install(project.source, project.id.clone(), Some(project.clone()));
                cx.notify();
            })),
        )
    }
}

fn card_metric(icon: &'static str, value: String, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(blocks::icon(icon, 15., theme.muted))
        .child(text(theme::CAPTION).text_color(theme.muted).child(value))
}
