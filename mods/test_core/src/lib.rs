use horizon_mod_sdk::{export_mod, chat, hud, Mod, Registration};

struct TestCore;

impl Mod for TestCore {
    fn init() -> Result<(Self, Registration), String> {
        horizon_mod_sdk::info("TestCore initializing with ModContainer support!");
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
}
export_mod!(TestCore);
