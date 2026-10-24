//! The sidebar's + opens a focused dialog and creates a persistent local instance.
use super::*;

pub(super) struct CreateUi {
    pub(super) open: bool,
    name: String,
    selected_name: bool,
    build: Option<String>,
    build_open: bool,
    error: Option<String>,
    name_focus: FocusHandle,
    build_focus: FocusHandle,
    cancel_focus: FocusHandle,
    create_focus: FocusHandle,
}

impl CreateUi {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            open: false,
            name: String::new(),
            selected_name: false,
            build: None,
            build_open: false,
            error: None,
            name_focus: cx.focus_handle(),
            build_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            create_focus: cx.focus_handle(),
        }
    }
}

impl Launcher {
    pub(super) fn open_create_instance(&mut self, window: &mut Window) {
        self.mods.ui.popup = None;
        self.instance_creation.open = true;
        self.instance_creation.name.clear();
        self.instance_creation.selected_name = false;
        self.instance_creation.build = self.selected().cloned();
        self.instance_creation.build_open = false;
        self.instance_creation.error = None;
        window.focus(&self.instance_creation.name_focus);
    }

    pub(super) fn close_create_instance(&mut self, window: &mut Window) -> bool {
        if !self.instance_creation.open {
            return false;
        }
        if self.instance_creation.build_open {
            self.instance_creation.build_open = false;
            window.focus(&self.instance_creation.build_focus);
        } else {
            self.instance_creation.open = false;
            window.focus(&self.root_focus);
        }
        true
    }

    pub(super) fn cycle_create_focus(&self, reverse: bool, window: &mut Window) {
        let ui = &self.instance_creation;
        let handles = [
            &ui.name_focus,
            &ui.build_focus,
            &ui.cancel_focus,
            &ui.create_focus,
        ];
        let current = handles
            .iter()
            .position(|handle| handle.is_focused(window))
            .unwrap_or(0);
        let next = if reverse {
            (current + handles.len() - 1) % handles.len()
        } else {
            (current + 1) % handles.len()
        };
        window.focus(handles[next]);
    }

    fn create_local_instance(&mut self, window: &mut Window) {
        let ui = &self.instance_creation;
        if ui
            .build
            .as_ref()
            .is_some_and(|build| !self.installed.contains(build))
        {
            self.instance_creation.error =
                Some("Choose an installed build, or choose later.".into());
            return;
        }
        match instances::create(&backend::game_home(), &ui.name, ui.build.clone()) {
            Ok(instance) => {
                let id = InstanceId::Local(instance.slug.clone());
                self.local_instances.push(instance);
                self.home_ui.id = id.clone();
                self.home_ui.loaded = false;
                self.instance_creation.open = false;
                self.open_instance(id);
                window.focus(&self.root_focus);
            }
            Err(error) => self.instance_creation.error = Some(error),
        }
    }

    pub(super) fn instance_creation_dialog(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.instance_creation.open {
            return None;
        }
        let t = self.theme;
        let ui = &self.instance_creation;
        let body = div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .p(px(24.))
            .child(section_title("Create instance", "plus", &t))
            .child(
                text(theme::CAPTION)
                    .text_color(t.muted)
                    .child("Each instance keeps its own content, worlds and settings."),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(caps("Name").text_color(t.muted))
                    .child(self.create_instance_name_field(cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(caps("Horizon build").text_color(t.muted))
                    .child(self.create_instance_build_picker(cx)),
            )
            .children(ui.error.clone().map(|error| notice(error, &t)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.))
                    .child(
                        button("new-instance-cancel", "Cancel", Kind::Secondary, &t)
                            .track_focus(&ui.cancel_focus)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.instance_creation.build_open = false;
                                this.close_create_instance(window);
                                cx.notify();
                            })),
                    )
                    .child(
                        button("new-instance-create", "Create instance", Kind::Primary, &t)
                            .track_focus(&ui.create_focus)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.create_local_instance(window);
                                cx.notify();
                            })),
                    ),
            );
        let card = blocks::panel(&t)
            .w_full()
            .max_w(px(520.))
            .max_h_full()
            .child(body);
        Some(
            div()
                .id("new-instance-overlay")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.))
                .bg(t.page.opacity(0.92))
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(card),
        )
    }

    fn create_instance_name_field(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let ui = &self.instance_creation;
        let focus = ui.name_focus.clone();
        let label = if ui.name.is_empty() {
            "My instance".to_owned()
        } else {
            ui.name.clone()
        };
        keyboard(div().id("new-instance-name"), &t)
            .track_focus(&focus)
            .w_full()
            .h(px(40.))
            .px(px(12.))
            .flex()
            .items_center()
            .overflow_hidden()
            .bg(t.well)
            .border_2()
            .border_color(t.outline)
            .cursor_text()
            .child(
                text(theme::UI)
                    .truncate()
                    .text_color(if ui.name.is_empty() { t.muted } else { t.ink })
                    .child(label),
            )
            .on_click(move |_, window, _| window.focus(&focus))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                let command = key.modifiers.control || key.modifiers.platform;
                if command && key.key == "a" {
                    this.instance_creation.selected_name = true;
                } else {
                    let ui = &mut this.instance_creation;
                    ui.error = None;
                    match servers_page::type_key(
                        &mut ui.name,
                        ui.selected_name,
                        &key.key,
                        key.key_char.as_deref(),
                        command,
                    ) {
                        servers_page::Key::Enter => this.create_local_instance(window),
                        servers_page::Key::Escape => {
                            this.close_create_instance(window);
                        }
                        servers_page::Key::Paste => {
                            if let Some(text) =
                                cx.read_from_clipboard().and_then(|item| item.text())
                            {
                                ui.name.extend(
                                    text.lines()
                                        .next()
                                        .unwrap_or_default()
                                        .chars()
                                        .take(instances::MAX_NAME_LENGTH),
                                );
                            }
                            ui.selected_name = false;
                        }
                        servers_page::Key::Edited => {
                            ui.selected_name = false;
                        }
                        servers_page::Key::Pass => return,
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    fn create_instance_build_picker(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let ui = &self.instance_creation;
        let label = ui
            .build
            .clone()
            .unwrap_or_else(|| "Choose after creating".into());
        let trigger = button("new-instance-build", label, Kind::Secondary, &t)
            .w_full()
            .justify_between()
            .track_focus(&ui.build_focus)
            .child(blocks::icon("chevron-down", 16., t.ink_soft))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let direction = match event.keystroke.key.as_str() {
                    "down" => 1,
                    "up" => -1,
                    _ => return,
                };
                let options = std::iter::once(None)
                    .chain(this.installed.iter().cloned().map(Some))
                    .collect::<Vec<_>>();
                let current = options
                    .iter()
                    .position(|build| build == &this.instance_creation.build)
                    .unwrap_or(0);
                let next =
                    (current as isize + direction).rem_euclid(options.len() as isize) as usize;
                this.instance_creation.build = options[next].clone();
                this.instance_creation.build_open = true;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.instance_creation.build_open = !this.instance_creation.build_open;
                cx.notify();
            }));
        let mut picker = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(trigger);
        if ui.build_open {
            let options = std::iter::once(None).chain(self.installed.iter().cloned().map(Some));
            let mut list = div()
                .id("new-instance-build-list")
                .flex()
                .flex_col()
                .max_h(px(170.))
                .overflow_y_scroll();
            for build in options {
                let label = build
                    .clone()
                    .unwrap_or_else(|| "Choose after creating".into());
                list = list.child(
                    button(
                        format!("new-instance-build-{label}"),
                        label,
                        Kind::Ghost,
                        &t,
                    )
                    .justify_start()
                    .when(ui.build == build, |button| button.bg(t.raised))
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, window, cx| {
                            this.instance_creation.build = build.clone();
                            this.instance_creation.build_open = false;
                            window.focus(&this.instance_creation.build_focus);
                            cx.notify();
                        },
                    )),
                );
            }
            let menu = blocks::panel(&t)
                .id("new-instance-build-popover")
                .absolute()
                .top(px(theme::CONTROL + 6.))
                .left_0()
                .w_full()
                .p(px(4.))
                .occlude()
                .child(list);
            picker = picker.child(gpui::deferred(menu).with_priority(2));
        }
        if self.installed.is_empty() {
            picker = picker.child(text(theme::MICRO).text_color(t.muted).child(
                "No build installed yet. You can install one from Versions after creating.",
            ));
        }
        picker
    }
}
