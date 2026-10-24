//! Compact browser navigation and anchored menus. Filter edits commit only on Apply.
use super::*;
use horizon_launcher::browser::details::{Environment, Filters, Sort};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Popup {
    Source,
    Sort,
    Filters,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum LibraryView {
    Installed,
    #[default]
    Browse,
}

pub(super) struct BrowserUi {
    pub(super) view: LibraryView,
    pub(super) popup: Option<Popup>,
    draft: Filters,
    expanded_filter: Option<FilterKind>,
    everything: bool,
    scroll: Scrollbar,
    focus: FocusHandle,
    source_focus: FocusHandle,
    sort_focus: FocusHandle,
    filter_focus: FocusHandle,
    source_bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
    sort_bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
    filter_bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
    pub(super) root_bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
}

impl BrowserUi {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            view: LibraryView::Browse,
            popup: None,
            draft: Filters::default(),
            expanded_filter: None,
            everything: true,
            scroll: Scrollbar::default(),
            focus: cx.focus_handle(),
            source_focus: cx.focus_handle(),
            sort_focus: cx.focus_handle(),
            filter_focus: cx.focus_handle(),
            source_bounds: Default::default(),
            sort_bounds: Default::default(),
            filter_bounds: Default::default(),
            root_bounds: Default::default(),
        }
    }
}

fn filter_count(filters: &Filters, everything: bool) -> usize {
    filters.categories.len()
        + usize::from(filters.loader.is_some())
        + usize::from(filters.game_version.is_some())
        + usize::from(filters.environment.is_some())
        + usize::from(!everything)
}

impl Launcher {
    pub(super) fn browser_navigation(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(20.))
            .border_b_1()
            .border_color(t.bevel)
            .children(
                [
                    (LibraryView::Installed, "Installed", "archive"),
                    (LibraryView::Browse, "Browse", "search"),
                ]
                .map(|(view, label, icon)| {
                    let selected = self.mods.ui.view == view;
                    keyboard(
                        div().id(SharedString::from(format!("mods-view-{label}"))),
                        &t,
                    )
                    .flex()
                    .flex_none()
                    .when(view == LibraryView::Installed, |tab| tab.mr(px(12.)))
                    .items_center()
                    .gap(px(7.))
                    .py(px(10.))
                    .border_b_2()
                    .border_color(if selected { t.ink } else { t.panel })
                    .cursor_pointer()
                    .child(blocks::icon(
                        icon,
                        17.,
                        if selected { t.ink } else { t.muted },
                    ))
                    .child(
                        blocks::pixel(16., FontWeight::MEDIUM)
                            .text_color(if selected { t.ink } else { t.muted })
                            .child(label),
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.mods.ui.view = view;
                            this.mods.ui.popup = None;
                            this.mods.reset_lists();
                            cx.notify();
                        },
                    ))
                }),
            )
    }

    pub(super) fn browser_menu_button(
        &self,
        popup: Popup,
        label: String,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let trigger = button(
            format!("menu-{:?}", popup),
            label,
            Kind::Secondary,
            &self.theme,
        )
        .child(blocks::icon("chevron-down", 14., self.theme.ink_soft));
        self.browser_menu_trigger(popup, trigger, cx)
    }

    pub(super) fn browser_menu_trigger(
        &self,
        popup: Popup,
        trigger: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let ui = &self.mods.ui;
        let (focus, bounds) = match popup {
            Popup::Source => (&ui.source_focus, ui.source_bounds.clone()),
            Popup::Sort => (&ui.sort_focus, ui.sort_bounds.clone()),
            Popup::Filters => (&ui.filter_focus, ui.filter_bounds.clone()),
        };
        trigger
            .relative()
            .track_focus(focus)
            .child(
                gpui::canvas(
                    move |rectangle, _, _| bounds.set(Some(rectangle)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if this.mods.ui.popup == Some(popup) {
                    this.close_browser_popup(window);
                } else {
                    this.mods.ui.draft = this.mods.browse.filters.clone();
                    this.mods.ui.expanded_filter = None;
                    this.mods.ui.everything = this.mods.everything;
                    this.mods.ui.scroll = Scrollbar::default();
                    this.mods.ui.popup = Some(popup);
                    window.focus(&this.mods.ui.focus);
                }
                cx.notify();
            }))
    }

    pub(super) fn browser_toolbar(&mut self, cx: &mut Context<Self>) -> Div {
        let count = filter_count(&self.mods.browse.filters, self.mods.everything);
        let filters = if count == 0 {
            "Filters".into()
        } else {
            format!("Filters ({count})")
        };
        let sort = self
            .mods
            .browse
            .filters
            .sort
            .map_or("Best match", Sort::label)
            .to_string();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(self.mods_field(cx))
            .child(self.browser_menu_button(Popup::Source, self.mods.source.label().into(), cx))
            .when(
                self.mods.source != horizon_launcher::browser::SourceId::Horizon,
                |toolbar| {
                    toolbar
                        .child(self.browser_menu_button(Popup::Sort, sort, cx))
                        .child(self.browser_menu_button(Popup::Filters, filters, cx))
                },
            )
    }

    pub(super) fn close_browser_popup(&mut self, window: &mut Window) -> bool {
        let Some(popup) = self.mods.ui.popup.take() else {
            return false;
        };
        window.focus(match popup {
            Popup::Source => &self.mods.ui.source_focus,
            Popup::Sort => &self.mods.ui.sort_focus,
            Popup::Filters => &self.mods.ui.filter_focus,
        });
        true
    }

    pub(super) fn browser_popup(
        &mut self,
        _viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.page != Page::Mods {
            return None;
        }
        let popup = self.mods.ui.popup?;
        let anchor = match popup {
            Popup::Source => self.mods.ui.source_bounds.clone(),
            Popup::Sort => self.mods.ui.sort_bounds.clone(),
            Popup::Filters => self.mods.ui.filter_bounds.clone(),
        };
        let bounds = anchor.get()?;
        let root = self.mods.ui.root_bounds.get()?;
        let viewport = root.size;
        let margin = 12.;
        let gap = 8.;
        let width = (if popup == Popup::Filters {
            520_f32
        } else {
            224.
        })
        .min(f32::from(viewport.width) - margin * 2.);
        let below = (f32::from(root.bottom() - bounds.bottom()) - gap - margin).max(0.);
        let above = (f32::from(bounds.top() - root.top()) - gap - margin).max(0.);
        let desired_height: f32 = match popup {
            Popup::Source => 132.,
            Popup::Sort => 220.,
            Popup::Filters => 580.,
        };
        let use_below = below >= desired_height.min(260.) || below >= above;
        let height = (if use_below { below } else { above }).min(desired_height);
        let top = if use_below {
            f32::from(bounds.bottom()) + gap
        } else {
            f32::from(bounds.top()) - gap - height
        } - f32::from(root.top());
        let left = (f32::from(bounds.right() - root.left()) - width).clamp(
            margin,
            (f32::from(viewport.width) - width - margin).max(margin),
        );
        let body = match popup {
            Popup::Source => self.browser_source_menu(cx),
            Popup::Sort => self.browser_sort_menu(cx),
            Popup::Filters => self.browser_filter_menu(height, cx),
        };
        Some(
            body.id("browser-popover")
                .track_focus(&self.mods.ui.focus)
                .occlude()
                .absolute()
                .left(px(left))
                .top(px(top.max(margin)))
                .w(px(width))
                .max_h(px(height))
                .on_mouse_down_out(
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        // An invoker click reaches its toggle handler; it must not dismiss then reopen.
                        if anchor
                            .get()
                            .is_some_and(|bounds| bounds.contains(&event.position))
                        {
                            return;
                        }
                        this.mods.ui.popup = None;
                        cx.notify();
                    }),
                )
                .into_any_element(),
        )
    }

    fn browser_source_menu(&self, cx: &mut Context<Self>) -> Div {
        use horizon_launcher::browser::SourceId;
        let mut menu = blocks::panel(&self.theme).flex().flex_col().p(px(6.));
        for source in [SourceId::Modrinth, SourceId::Horizon] {
            menu = menu.child(
                button(
                    format!("content-source-{source:?}"),
                    source.label(),
                    Kind::Ghost,
                    &self.theme,
                )
                .justify_start()
                .when(self.mods.source == source, |button| {
                    button.bg(self.theme.raised)
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.mods.source = source;
                    this.mods.browse.filters = Default::default();
                    this.mods.browse.open = None;
                    this.close_browser_popup(window);
                    this.mods_search(0);
                    cx.notify();
                })),
            );
        }
        menu.child(disabled(button(
            "content-source-curseforge",
            "CurseForge · coming soon",
            Kind::Ghost,
            &self.theme,
        )))
    }

    fn browser_sort_menu(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let options = std::iter::once((None, "Best match"))
            .chain(Sort::ALL.into_iter().map(|sort| (Some(sort), sort.label())));
        blocks::panel(&t)
            .flex()
            .flex_col()
            .p(px(6.))
            .children(options.map(|(sort, label)| {
                button(format!("sort-choice-{label}"), label, Kind::Ghost, &t)
                    .w_full()
                    .justify_start()
                    .when(self.mods.browse.filters.sort == sort, |button| {
                        button.bg(t.raised).text_color(t.ink)
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.mods.browse.filters.sort = sort;
                        this.close_browser_popup(window);
                        this.mods_search(0);
                        cx.notify();
                    }))
            }))
    }

    fn browser_filter_menu(&mut self, height: f32, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let tags = self.mods_tags().cloned();
        let mut body = div()
            .id("browser-filter-body")
            .flex()
            .flex_col()
            .gap(px(18.))
            .p(px(18.))
            .max_h(px((height - 116.).max(40.)))
            .overflow_y_scroll();
        if let Some(tags) = tags {
            body = body
                .child(self.browser_filter_choices("Loader", tags.loaders, FilterKind::Loader, cx))
                .child(self.browser_filter_choices(
                    "Game version",
                    tags.game_versions,
                    FilterKind::Version,
                    cx,
                ))
                .child(
                    self.browser_filter_choices(
                        "Environment",
                        Environment::ALL
                            .map(|environment| environment.label().to_owned())
                            .to_vec(),
                        FilterKind::Environment,
                        cx,
                    ),
                )
                .child(
                    self.browser_filter_choices(
                        "Categories",
                        tags.categories
                            .into_iter()
                            .map(|category| category.name)
                            .collect(),
                        FilterKind::Category,
                        cx,
                    ),
                );
        } else if let Some(error) = self
            .mods
            .browse
            .tag_errors
            .get(&(self.mods.source, self.mods.kind))
        {
            body = body
                .child(notice(
                    format!("Could not load filter options: {error}"),
                    &t,
                ))
                .child(
                    button("filters-retry", "Try again", Kind::Secondary, &t).on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.retry_mods_tags();
                            cx.notify();
                        }),
                    ),
                );
        } else {
            body = body.child(
                text(theme::CAPTION)
                    .text_color(t.muted)
                    .child("Loading filter options…"),
            );
        }
        let supported = switch("filter-supported", !self.mods.ui.everything, &t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.mods.ui.everything = !this.mods.ui.everything;
                cx.notify();
            }),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(supported)
                .child(text(theme::CAPTION).child("Only projects Horizon can run")),
        );
        let actions = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .flex_none()
            .p(px(12.))
            .border_t_1()
            .border_color(t.bevel)
            .child(
                button("filter-clear", "Clear all", Kind::Ghost, &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        let sort = this.mods.ui.draft.sort;
                        this.mods.ui.draft = Filters {
                            sort,
                            ..Default::default()
                        };
                        this.mods.ui.everything = true;
                        cx.notify();
                    },
                )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("filter-cancel", "Cancel", Kind::Ghost, &t).on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.close_browser_popup(window);
                            cx.notify();
                        }),
                    ))
                    .child(
                        button("filter-apply", "Apply filters", Kind::Primary, &t).on_click(
                            cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.mods.browse.filters = this.mods.ui.draft.clone();
                                this.mods.everything = this.mods.ui.everything;
                                this.close_browser_popup(window);
                                this.mods_search(0);
                                cx.notify();
                            }),
                        ),
                    ),
            );
        blocks::panel(&t)
            .flex()
            .flex_col()
            .max_h(px(height))
            .child(
                section_title("Filters", "sliders", &t)
                    .p(px(16.))
                    .flex_none(),
            )
            .child(self.mods.ui.scroll.wrap(body, &t).min_h_0())
            .child(actions)
    }

    fn browser_filter_choices(
        &self,
        title: &'static str,
        values: Vec<String>,
        kind: FilterKind,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let expanded = self.mods.ui.expanded_filter == Some(kind);
        let heading = button(format!("filter-group-{title}"), title, Kind::Ghost, &t)
            .w_full()
            .justify_start()
            .gap(px(12.))
            .child(
                text(theme::CAPTION)
                    .ml_auto()
                    .text_color(t.muted)
                    .child(kind.summary(&self.mods.ui.draft)),
            )
            .child(blocks::icon(
                if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                16.,
                t.muted,
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.mods.ui.expanded_filter = if expanded { None } else { Some(kind) };
                cx.notify();
            }));
        let choices = values.into_iter().map(|value| {
            let selected = kind.selected(&self.mods.ui.draft, &value);
            button(
                format!("filter-{title}-{value}"),
                value.replace('-', " "),
                Kind::Ghost,
                &t,
            )
            .when(selected, |button| button.bg(t.raised).text_color(t.ink))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                kind.toggle(&mut this.mods.ui.draft, &value);
                cx.notify();
            }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(heading)
            .when(expanded, |group| {
                group.child(
                    div()
                        .id(SharedString::from(format!("filter-values-{title}")))
                        .flex()
                        .flex_wrap()
                        .gap(px(4.))
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .children(choices),
                )
            })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FilterKind {
    Loader,
    Version,
    Environment,
    Category,
}

impl FilterKind {
    fn summary(self, filters: &Filters) -> String {
        match self {
            Self::Loader => filters.loader.clone().unwrap_or_else(|| "Any".into()),
            Self::Version => filters.game_version.clone().unwrap_or_else(|| "Any".into()),
            Self::Environment => filters.environment.map_or("Any", Environment::label).into(),
            Self::Category => match filters.categories.as_slice() {
                [] => "Any".into(),
                [category] => category.replace('-', " "),
                categories => format!("{} selected", categories.len()),
            },
        }
    }

    fn selected(self, filters: &Filters, value: &str) -> bool {
        match self {
            Self::Loader => filters.loader.as_deref() == Some(value),
            Self::Version => filters.game_version.as_deref() == Some(value),
            Self::Environment => filters
                .environment
                .is_some_and(|environment| environment.label() == value),
            Self::Category => filters.categories.iter().any(|category| category == value),
        }
    }
    fn toggle(self, filters: &mut Filters, value: &str) {
        let selected = self.selected(filters, value);
        match self {
            Self::Loader => filters.loader = (!selected).then(|| value.to_owned()),
            Self::Version => filters.game_version = (!selected).then(|| value.to_owned()),
            Self::Environment => {
                filters.environment = if selected {
                    None
                } else {
                    Environment::ALL
                        .into_iter()
                        .find(|environment| environment.label() == value)
                }
            }
            Self::Category if selected => filters.categories.retain(|category| category != value),
            Self::Category => filters.categories.push(value.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_drafts_are_independent_and_each_category_toggles_once() {
        let applied = Filters::default();
        let mut draft = applied.clone();
        FilterKind::Category.toggle(&mut draft, "worldgen");
        FilterKind::Loader.toggle(&mut draft, "fabric");
        assert_eq!(filter_count(&draft, false), 3);
        assert!(applied.categories.is_empty());
        FilterKind::Category.toggle(&mut draft, "worldgen");
        assert!(draft.categories.is_empty());
        FilterKind::Loader.toggle(&mut draft, "fabric");
        assert_eq!(draft, applied);
    }
}
