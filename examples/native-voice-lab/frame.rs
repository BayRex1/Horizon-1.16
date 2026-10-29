fn frame(game: &mut crate::game::Game<'_>) {
    use horizon_mods::native::Event;
    // Only the window calls this hook. A server Game never shares its device state.
    static SOURCE: std::sync::Mutex<Option<(u32, u32)>> = std::sync::Mutex::new(None);
    if !game.native_window() || !game.native_on("native_voice_lab") {
        return;
    }
    let Ok(voice) = game.native_voice("native_voice_lab") else {
        return;
    };
    let mut source = SOURCE.lock().unwrap();
    for event in voice.poll() {
        if matches!(event, Event::Connected(_) | Event::Disconnected) {
            *source = None;
        }
    }
    let Ok(frames) = voice.read_capture() else {
        return;
    };
    for frame in frames {
        if source.is_none_or(|(_, rate)| rate != frame.sample_rate) {
            if let Some((id, _)) = source.take() {
                voice.close_source(id);
            }
            let Ok(id) = voice.create_source(frame.sample_rate) else {
                continue;
            };
            *source = Some((id, frame.sample_rate));
        }
        if let Some((id, _)) = *source
            && voice.write_audio(id, frame.samples).is_err()
        {
            voice.close_source(id);
            *source = None;
        }
    }
}
