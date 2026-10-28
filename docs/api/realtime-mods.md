# Real-time audio and binary networking (experimental, API level 8)

These APIs provide building blocks for voice mods. Horizon does not implement
Simple Voice Chat, its wire protocol, Opus, echo cancellation or a jitter buffer.
Mods may implement codecs in their Wasm component. PCM is normalized `f32`,
interleaved, with an explicit sample rate and channel count.

## Permissions

```toml
capabilities = ["microphone.capture", "audio.playback", "network.custom", "network.udp", "input"]
experimental = ["microphone.capture", "audio.playback", "network.custom", "network.udp", "input"]
```

Install a mod, inspect its requested capabilities in Options > Mods, then enable
it to grant permission. Sensitive capabilities are persisted individually. An
update adding a sensitive capability requires disabling and enabling the mod
again. Development mods do not automatically receive these permissions. The OS
may require an additional microphone permission; device failures are returned as
errors. `microphone::request_permission()` reports Horizon's grant without opening
a device. Turning off a mod releases its sockets and audio streams.

Server operators grant networking permission through the corresponding
`permission.<mod-id>.network.custom:true` and
`permission.<mod-id>.network.udp:true` lines in `horizon-mods.txt`, alongside
`mod.<mod-id>:true`. A server mod uses `side = "server"`, `api = "0.3"`.

## Capture and Push-to-Talk

Use the existing `input::register_key` and `input::poll` APIs. On `KeyDown`,
call `microphone::start(None)` (or pass a device id from `devices()`), retrying
while held if setup is pending. Its result
is `(sample_rate, channels)` for the native input format. Poll `read()` on frame
hooks for lower latency; on `KeyUp`, call `stop()`. Opening a menu generates
key-up events through the existing input API. `stop()` discards buffered PCM.
There is one capture stream per mod; restarting replaces it.

Capture and playback require the game's `audio` feature (enabled in normal
client builds). Headless builds return an error. `HORIZON_NO_AUDIO` disables
native devices. Audio device work runs on a dedicated thread; audio callbacks
never call Wasm. Device enumeration and setup are asynchronous: `devices`, `start` and `create`
return an error containing `pending` while the worker is preparing the device.
Retry on the next frame (with the same selected input device). `stop` cancels
pending capture. No game hook waits on an OS audio service.

## Playback

`audio::create(rate, channels)` returns a mod-local handle. Submit PCM with
`push`, set per-source volume and world coordinates with `configure`, and delete
it with `remove`. Listener position and yaw follow the local player; output respects the game master volume. Positioned
sources use linear distance attenuation and stereo panning; stereo input is
mixed to mono before positioning. Non-positioned stereo preserves its channels.
The output converts sample rates; input capture does not resample. Spatial audio
has no occlusion, reverb or HRTF. An exhausted source plays silence.

There are at most 16 playback sources and four UDP sockets per mod. Audio queues
hold 200 ms, each PCM call accepts at most 8192 samples, and invalid/nonfinite
samples, incomplete interleaved frames and invalid coordinates are rejected.
Microphone overflow discards oldest samples; playback overflow returns an error.
Real-time I/O has a shared 128 KiB budget per 50 ms, including polling costs.
Dropping a mod closes native streams asynchronously on their owning thread.

## External voice servers

`network::connect("127.0.0.1:24454")` opens a nonblocking connected UDP socket;
IPv6 uses `[::1]:24454`. It requires **both** networking capabilities. Only a
numeric address is accepted so DNS cannot block a game hook. `send` accepts at
most 1200 bytes; `poll` returns up to 32 packets only from that peer. A successful
send is not an acknowledgement. Oversized incoming datagrams are discarded;
packet loss, duplication and reordering must be handled by the mod. `close`
releases the socket. No raw sockets or WASI networking are exposed.

## Client/server mod channels

Client and server components belonging to the same mod register the same
`<mod-id>:<channel>` with `messaging::register`. Both need `network.custom`.
Clients call `send("", channel, data)` to reach their game server; server mods
send to a player's UUID, obtained from `players::online` (capability `players`).
Only registered channels in the receiving mod's namespace receive data.

The server gets the sender UUID from the authenticated game connection, not
from packet bytes. A server mod can choose nearby recipients using the players'
positions/dimensions and forward voice frames. Connections must be established
before sending. Messages use Minecraft custom payload packets, up to 8192 bytes,
and arrive via `messaging::poll` (32 per call, bounded queue of 64).

These channels use the game's **reliable, ordered transport**. They do not offer
unreliable delivery; use the external UDP API for latency-sensitive voice.
Horizon does not advertise compatibility with an existing voice server protocol.
The normal server-thread, LAN and dedicated connection paths carry the packets;
in-frame single player has no client/server loopback channel. Disconnect clears
client packets, UDP sockets and active audio. A mod reload starts with no handles.

See [the generated reference](mods-reference.md) for exact signatures.
