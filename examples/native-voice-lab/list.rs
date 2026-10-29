fn list(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: "native_voice_lab".into(),
        title: "Native Voice Lab 0.1.0".into(),
        description: "Local PTT microphone monitor and consent-gated transport diagnostics.".into(),
        keys: vec![[
            "ptt".into(),
            "Microphone monitor (hold)".into(),
            "key.keyboard.v".into(),
        ]],
    });
}
