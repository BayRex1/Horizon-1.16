use horizon_mod_sdk::{
    Mod, Registration, chat, export_mod, hud,
    ui::{self, ButtonWidget, Screen, TextWidget, Widget},
};

struct TestCore;

/// The screen shown in the inventory's place (`ui.replace-screen`).
fn core_screen() -> Screen {
    Screen {
        title: "Test Core".into(),
        widgets: vec![
            Widget::Text(TextWidget {
                id: 1,
                text: "This screen opens instead of the inventory (E).".into(),
                color: 0,
            }),
            Widget::Button(ButtonWidget {
                id: 2,
                label: "Give the inventory back".into(),
            }),
        ],
    }
}

impl Mod for TestCore {
    fn init() -> Result<(Self, Registration), String> {
        horizon_mod_sdk::info("TestCore initializing with ModContainer support!");
        ui::replace_screen("minecraft:inventory", Some(&core_screen()))?;
        Ok((TestCore, Registration::new().on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        if tick % 100 == 0 {
            chat::show(&format!("TestCore tick: {}", tick));
            hud::set_panel(horizon_mod_sdk::hud::Corner::TopLeft, &[
                horizon_mod_sdk::hud::Line {
                    text: format!("TestCore active at tick {}", tick),
                    color: 0x00FF00,
                },
            ]);
        }
    }

    // The button on the replacement screen: E opens the vanilla inventory again.
    fn ui_select(&mut self, id: u32, _row: u32) {
        if id == 2 && ui::replace_screen("minecraft:inventory", None).is_ok() {
            ui::close_screen();
            chat::show("TestCore: the inventory is back");
        }
    }
}
export_mod!(TestCore);
