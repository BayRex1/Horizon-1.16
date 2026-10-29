fn key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    if !game.native_window() || !game.native_on("native_voice_lab") {
        return;
    }
    if key != game.native_mod_key("native_voice_lab", "ptt", "key.keyboard.v") {
        return;
    }
    if pressed && !game.native_playing() {
        return;
    }
    let result = game.native_voice("native_voice_lab").and_then(|voice| {
        if pressed {
            voice.start_capture(String::new(), 1.0)
        } else {
            voice.stop_capture();
            Ok(())
        }
    });
    if let Err(error) = result {
        game.native_chat(&error);
    }
}
