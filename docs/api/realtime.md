# Experimental realtime mod APIs

API level **8** adds the `microphone`, `audio`, `udp` and `networking`
capabilities to Rust/WebAssembly mods. They are reusable transports and PCM
interfaces. Horizon does not implement Simple Voice Chat, Opus or its encrypted
wire protocol through these APIs. A compatible mod must implement and test that
protocol itself. Minecraft protocol/version compatibility is still required:
audio APIs do not let Horizon join a server using a different Minecraft version.

The complete contract is in [mods-reference.md](mods-reference.md). The working
[voice-lab example](../../crates/horizon-mod-sdk/examples/voice-lab/) demonstrates
local Push-to-Talk monitoring, binary UDP and a versioned custom payload.

## Permissions

Mods declare each capability they use in both `capabilities` and `experimental`:

```toml
capabilities = ["microphone", "audio", "udp", "networking", "input"]
experimental = ["microphone", "audio", "udp", "networking", "input"]
```

The Mods page describes microphone recording separately from playback and
network access before the player enables the mod. The manifest is checked
against actual component imports; importing a microphone without declaring
`microphone` refuses the entire mod. Mods remain off by default. Recording also needs the separate **Microphone: Allowed** switch for that mod
in Options > Mods, subject to OS device permissions. It defaults to Blocked,
even for enabled mods and development mods. Upgrading an enabled mod to add
microphone imports does not grant recording. Revoking this switch stops capture
immediately; grants are stored in `horizon-mods.txt`.
There is no automatic permission grant merely because another mod uses audio.
Server mods may use `networking`, but cannot import microphone, playback or UDP.

## Capture and playback

`microphone::devices` and `audio::devices` discover devices asynchronously;
the first call can return an empty list. An empty device ID selects the system
default. IDs are enumeration indices valid for this mod instance; save a device
name as a preference and resolve it again after reloading. Discovery alone opens
no input or output stream. `microphone::start` queues device opening on a worker;
`read` reports startup/device errors. `stop` immediately mutes and discards queued
PCM, and queues closing the physical stream on its owning worker. Unloading,
trapping or disconnecting also stops capture and clears playback sources.

Input is mono float PCM at the device's default sample rate, normalized to
`[-1, 1]`, split into 20 ms frames. Up to ten frames are retained; overflow
replaces the oldest. Gain is finite, 0..4, with clipping. A mod can use RMS or
peak energy to implement voice activation, and `input` key-down/key-up events
for Push-to-Talk. Poll on frame hooks for lower latency than 20 Hz tick hooks.
The host does not implement noise suppression, echo cancellation or an encoder.

Each `audio::create(rate)` allocates an independent mono source (8–96 kHz).
The device worker mixes up to 32 sources to the selected output, resampling
and clipping the final mix. `configure` sets gain (including mute), optional
world position, and linear attenuation to silence at the specified distance.
Positions must be in the listener's dimension; mods should mute players in
other dimensions. The listener follows the local player's position and yaw.
Non-positional sources play centered. Positional panning is stereo, without
HRTF or obstruction simulation. Master/category volume is not applied to these
independent streams; the mod should expose its own speaker volume.

Each source holds at most 200 ms PCM; a full buffer returns an error without
blocking. An underrun emits silence. `select-device` clears sources and opens
the chosen output asynchronously. `write` reports output errors. Audio is
unavailable in builds without `horizon-mods/audio`, or with `HORIZON_NO_AUDIO`
set. Builds without the client's `audio` feature keep the server free of ALSA.
Hardware capture, latency and output-device behavior need testing on each OS.

## UDP

`udp::connect` accepts a numeric IPv4 endpoint or bracketed IPv6 endpoint.
It requires an active Minecraft play session, binds an ephemeral local port,
and connects to one peer, so received packets are restricted by the OS to that
peer. No WASI sockets, arbitrary bind/listen or raw networking is exposed.
For server-provided hostnames, `resolve` starts a bounded DNS worker and
`resolved` polls its result without blocking. Only permitted unicast IPs are
returned. There is one pending lookup per mod and at most four resolver workers
across the runtime; DNS results are consumed once, and disconnect discards them.
`networking::server_address` supplies the Minecraft address for protocols that
default to the same host. Pass resolved numeric endpoints to `connect` to avoid
DNS rebinding between validation and socket creation.
Unspecified, multicast, broadcast, link-local and zero-port endpoints are
refused. Loopback and private addresses are supported for local/LAN servers.
External unicast endpoints are allowed by the explicit `udp` permission.

Each mod owns at most four sockets and can attempt at most 16 opens per second. Datagrams are at most 1200 bytes; oversize
incoming packets are discarded. Shared send and receive limits are each 256
packets per second. `receive` drains up to 32 datagrams, returning immediately
with an empty list if none are waiting. Packet boundaries and binary bytes are
preserved. UDP remains unreliable and unencrypted. Mods must authenticate the
voice server and implement their protocol's encryption, replay protection,
keepalives, reordering and jitter buffer before sending voice. Endpoint strings
received from servers must still pass host validation. For nonce/key generation,
`networking::random_bytes` exposes OS cryptographic randomness (1024 bytes per
call, 4096 per hook), so mods need no WASI randomness or predictable time seed.

## Minecraft and Horizon mod networking

`networking::register` subscribes to an exact namespaced play-state custom
payload channel. Up to 32 channels per mod, each at most 128 bytes; `minecraft`
and `horizon` namespaces are reserved for host protocols. Client subscriptions
are announced using `minecraft:register` / `minecraft:unregister`, aggregated
across mods. Channel registration persists across connections and is announced
again after joining; queued packets are discarded on disconnect.

Clients send with `player = None`; servers send to a UUID from `players::online`.
The host attaches the authenticated connection's player UUID to inbound server
messages, rather than trusting bytes supplied by the sender. The Rust server SDK also needs the `players` capability for the player types
in its event exports (as declared by `voice-lab-server`). The existing server
`player_joined` / `player_left` hooks provide lifecycle events and identity.
Client `networking::poll` also returns connected/disconnected events and the
local UUID is available from `local_player`. The transport works on remote Java
servers, Horizon dedicated/LAN/integrated servers, and the in-frame local world.
It does not expose configuration or login packets.

Payloads are binary, up to 32767 bytes. Each hook can queue at most 32 messages
and 64 KiB; each mod retains at most 64 inbound messages and 256 KiB (oldest
packets drop on overflow). Sends are routed after the hook returns. Polling
keeps all guest code on the mod thread: device/network callbacks never call
WebAssembly directly. Handle empty polls and reconnects without busy loops.

For version negotiation, register a channel such as `your_mod:hello`. Have the
client send a protocol version and supported features; the server replies with
its selected version or an explicit incompatibility. Only start application
traffic after acceptance. Channel presence alone is not a compatibility check.
The host does not impose a wire format, so mods can also speak existing Fabric,
Forge or Paper plugin protocols. Simple Voice Chat's handshake, secret exchange
and UDP codec remain part of its compatibility mod.

## Building the example

From `crates/horizon-mod-sdk/examples/voice-lab`, run `cargo horizon-mod build`
and load the resulting `.hmod` through the normal mod development workflow.
Enable the mod and its separate Microphone switch in Options > Mods.
Hold V to monitor your microphone locally (headphones recommended).
`/voice_udp 127.0.0.1:24454` selects a diagnostic peer;
`/voice_ping hello` sends UTF-8 diagnostic bytes, and `/voice_udp close` closes it.
`/voice_hello text` sends protocol version bytes `[1, 0]` followed by the text
on `voice_lab:hello`. The companion `voice-lab-server` example acknowledges
version 1 and rejects other versions using the authenticated sender UUID.
The example never transmits microphone PCM over UDP.

The implementation covers experimental capture/playback/UDP and play-state
custom payload transport. Further work includes a complete interoperable voice
mod, cross-platform hardware verification, dynamic device hotplug, HRTF,
voice-specific processing and a standardized mod protocol negotiation scheme.
