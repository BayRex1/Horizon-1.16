fn services(mods: &mut Vec<horizon_mods::native::Declaration>) {
    mods.push(horizon_mods::native::Declaration {
        id: "native_voice_lab".into(), title: "Native Voice Lab".into(),
        author: "dan4ikx".into(), version: "0.1.0".into(),
        purpose: "Monitor microphone audio locally while PTT is held; test binary transport only on endpoints you approve".into(),
        microphone: true, audio: true,
    });
}
