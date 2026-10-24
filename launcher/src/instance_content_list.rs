//! Cached local filtering/order and viewport-only rows within the chosen page.
use super::*;
use horizon_launcher::{browser::Kind as ContentKind, pagination::Pagination};

#[derive(Default)]
pub(super) struct ContentList {
    pub(super) paging: Pagination,
    pub(super) rows: cull::Cull,
    pub(super) indices: Vec<usize>,
    criteria: Option<(String, Option<ContentKind>, bool)>,
    dirty: bool,
}

impl ContentList {
    pub(super) fn invalidate(&mut self) {
        self.dirty = true;
    }
}

impl ContentList {
    fn prepare(
        &mut self,
        content: &[instance_content::ContentItem],
        query: &str,
        kind: Option<ContentKind>,
        updates_first: bool,
        updated: impl Fn(&instance_content::ContentItem) -> bool,
    ) {
        let criteria = (query.to_lowercase(), kind, updates_first);
        if !self.dirty && self.criteria.as_ref() == Some(&criteria) {
            return;
        }
        if self.criteria.as_ref() != Some(&criteria) {
            self.paging.index = 0;
        }
        let mut indices = content
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                criteria.1.is_none_or(|kind| item.kind == kind)
                    && item.search_name.contains(&criteria.0)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        indices.sort_by_cached_key(|index| {
            let item = &content[*index];
            (updates_first && !updated(item), item.search_name.as_str())
        });
        self.paging.clamp(indices.len());
        self.indices = indices;
        self.criteria = Some(criteria);
        self.dirty = false;
        self.rows.reset();
    }
}

impl Launcher {
    fn prepare_instance_content_list(&mut self) {
        let instance = &mut self.instance;
        let mods = &self.mods;
        let same_home = mods.home == instance.home;
        instance.content_list.prepare(
            &instance.content,
            &instance.query,
            instance.kind,
            instance.updates_first,
            |item| match &item.origin {
                instance_content::Origin::Managed(entry) => {
                    same_home
                        && mods
                            .updates
                            .contains_key(&(entry.source, entry.project.clone()))
                }
                _ => false,
            },
        );
    }

    pub(super) fn instance_content(&mut self, cx: &mut Context<Self>) -> Div {
        self.prepare_instance_content_list();
        let t = self.theme;
        let total = self.instance.content_list.indices.len();
        let range = self.instance.content_list.paging.range(total);
        let indices = self.instance.content_list.indices[range.clone()].to_vec();
        let rows = self.instance.content_list.rows.clone();
        let scroll = self.content_scroll.handle.clone();
        let retained = indices.iter().position(|index| {
            self.instance.confirm_remove.as_ref() == Some(&self.instance.content[*index].key)
        });
        let list = rows.column(
            &scroll,
            indices.len(),
            0.,
            |index| retained == Some(index),
            |index| {
                let item = self.instance.content[indices[index]].clone();
                self.instance_content_row(&item, cx)
            },
        );
        let mut page = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(self.instance_content_toolbar(cx));
        if self.instance.id.imported_slug().is_some() {
            page = page.child(self.instance_port_controls(cx));
        }
        page = page.child(
            text(theme::CAPTION)
                .text_color(t.muted)
                .child(format!("{total} projects · changes apply on next launch")),
        );
        if !self.instance.selected.is_empty() {
            page = page.child(self.instance_selection_actions(cx));
        }
        let table = blocks::panel(&t)
            .flex()
            .flex_col()
            .child(self.instance_content_columns())
            .child(list)
            .when(total == 0, |table| {
                table.child(centered(
                    "No content matches. Browse to add projects, or change the search.",
                    &t,
                ))
            });
        page.child(table).child(self.content_pager(
            content_paging::Collection::Instance,
            Some(total),
            range.len(),
            range.end < total,
            false,
            cx,
        ))
    }

    fn instance_selection_actions(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(text(theme::CAPTION).child(format!("{} selected", self.instance.selected.len())))
            .children(
                [(true, "Enable"), (false, "Disable")].map(|(enabled, label)| {
                    button(
                        format!("instance-selection-{label}"),
                        label,
                        Kind::Ghost,
                        &t,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.instance_bulk_enabled(enabled);
                            cx.notify();
                        },
                    ))
                }),
            )
    }

    fn instance_content_columns(&self) -> Div {
        let t = self.theme;
        div()
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
                    .w(px(200.))
                    .text_color(t.muted)
                    .child("Actions"),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_launcher::pagination::PageSize;
    use instance_content::ContentItem;

    #[test]
    fn ten_thousand_projects_render_only_nearby_rows_from_the_selected_page() {
        let items = (0..10_000)
            .map(|index| ContentItem::fixture(index, ContentKind::Mod))
            .collect::<Vec<_>>();
        let mut list = ContentList::default();
        list.prepare(&items, "", None, false, |_| false);
        list.paging.set_size(PageSize::Hundred);
        list.paging.index = 50;
        let range = list.paging.range(list.indices.len());
        let mut drawn = vec![];
        list.rows.column(
            &gpui::ScrollHandle::new(),
            range.len(),
            0.,
            |_| false,
            |index| {
                drawn.push(list.indices[range.start + index]);
                div().h(px(88.))
            },
        );
        assert!(
            drawn.len() <= 17,
            "constructed {} rows instead of just the viewport",
            drawn.len()
        );
        assert!(drawn.iter().all(|index| (5_000..5_100).contains(index)));
    }

    #[test]
    fn redraws_reuse_the_projection_and_filtering_resets_the_page() {
        let items = (0..100)
            .map(|index| ContentItem::fixture(index, ContentKind::Mod))
            .collect::<Vec<_>>();
        let mut list = ContentList::default();
        list.prepare(&items, "", None, true, |item| item.key == "99");
        assert_eq!(list.indices[0], 99);
        list.paging.index = 4;
        list.prepare(&items, "", None, true, |_| {
            panic!("unchanged redraw recomputed ordering")
        });
        assert_eq!(list.paging.index, 4);
        list.prepare(&items, "Project 00042", None, true, |_| false);
        assert_eq!(list.indices, [42]);
        assert_eq!(list.paging.index, 0);
    }

    #[test]
    fn content_changes_preserve_a_valid_page_and_clamp_after_removal() {
        let items = (0..100)
            .map(|index| ContentItem::fixture(index, ContentKind::ResourcePack))
            .collect::<Vec<_>>();
        let mut list = ContentList::default();
        list.prepare(&items, "", Some(ContentKind::ResourcePack), false, |_| {
            false
        });
        list.paging.index = 3;
        list.invalidate();
        list.prepare(&items, "", Some(ContentKind::ResourcePack), false, |_| {
            false
        });
        assert_eq!(list.paging.index, 3);
        list.invalidate();
        list.prepare(
            &items[..21],
            "",
            Some(ContentKind::ResourcePack),
            false,
            |_| false,
        );
        assert_eq!(list.paging.index, 1);
        assert_eq!(list.paging.range(list.indices.len()), 20..21);
    }
}
