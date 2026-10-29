# Native voice services (experimental, hook API 3)

Native mods can implement audio and networking through `native_voice_mods` and
`native_voice` in `game/native_hooks.rs`. No WIT import, component or WebAssembly
runtime is involved. A build without the `mods` feature supports the same API;
without `audio`, device operations return an error while networking still works.

This API supplies transports and PCM. A separate mod must implement its voice
protocol, codec, authentication, encryption and compatibility negotiation.
Simple Voice Chat interoperability has not been verified.

## Declare a service user

A HEAD hook on `native_voice_mods` appends `horizon_mods::native::Declaration`:

```rust,ignore
mods.push(horizon_mods::native::Declaration {
    id: "my_voice".into(),
    title: "My Voice".into(),
    author: "Your name".into(),
    version: "0.1.0".into(),
    purpose: "Proximity voice with players on the connected server".into(),
    microphone: true,
    audio: true,
});
```

List the same ID using `native_mods` so the player sees its settings and can
turn it off. Declarations must be deterministic: changing one revokes its
session grants. No declaration creates a device stream, DNS lookup or socket.
The registry holds at most 32 users. An invalid declaration is not registered.

In a native frame, tick, key or command hook, call
`game.native_voice("my_voice")?` to obtain `&mut horizon_mods::native::Voice`.
The registry, connection snapshot and budgets are updated before tick/frame
hooks. Workers communicate through bounded owned queues; mod code runs on the
game thread. Pending payload output is routed after those hooks (other hooks'
output is flushed by the next frame/tick). Types can also be imported from
`horizon_mod_sdk::native` on native targets with the SDK's `native` feature.
Mixin helpers already have access to the game's `horizon_mods` dependency.

## Consent and lifecycle

Options > Mods > the mod's Settings lists separate microphone and destination
buttons. Hovering shows the full scope and the declaring mod's identity,
purpose and current server. All grants are session-only and blocked initially.
The microphone grant exposes raw captured audio to that mod. Turning it off
stops capture immediately; the HUD indicator remains visible even under F1 and
points to these controls. Focus loss stops capture and prevents another start
until the window regains focus. OS microphone permission is still required.

`request_endpoint(host, port)` only creates a deduplicated UI request, limited
to 16 per mod. Hostnames and numeric IPs are separate scopes. Approving DNS
allows only that exact hostname/port to be resolved with the system resolver.
The mod polls `resolved()` and each resulting numeric address creates a new
UDP consent request. Approving a hostname never approves its returned IPs.
The numeric peer is validated and pinned when connecting. A different IP or
port needs its own consent. Loopback/private endpoints say LOCAL NETWORK;
public peers say PUBLIC UDP. No server advertisement can grant consent.

Revoking a network scope closes the mod's sockets and cancels its pending
resolver receiver, so stale results cannot reopen resources. Disconnect,
reconnect (including the same server), disabling the mod, changing its
declaration or removing it clears all grants and device/network queues.
Permission files are not shared or persisted between instances.

These are **cooperative controls**, not a sandbox. A native bundle can bypass
helpers, forge attribution or access the OS directly. The native-code trust
warning and source review remain necessary. See the
[permission design and remaining decisions](native-voice-permissions.md).

## Audio

- `input_devices`, `start_capture(device, gain)`, `stop_capture`, `input_gain`,
  `read_capture`: mono normalized `f32` PCM frames with their sample rate.
  Enumeration/opening happens on a worker. Empty device IDs select the default;
  IDs apply to the current enumeration. At most ten 20 ms frames are queued.
- `output_devices`, `select_output`, `create_source(rate)`, `write_audio`,
  `configure_source`, `close_source`: up to 32 independently buffered mono
  sources, resampled and mixed to stereo, with 200 ms per source.
- `audio::Settings { gain, at, distance }`: independent player volume/muting,
  optional 3D position and linear distance attenuation. The game updates the
  listener position/yaw and master volume automatically. Positions must refer
  to the listener's dimension; the mod handles cross-dimension muting.

PTT uses `native_key`; voice activation can inspect PCM amplitude. Neither
implies consent. Noise suppression, echo cancellation, HRTF and encoding are
outside this API. The default device is not silently reopened on failure;
worker errors surface through read/write calls.

## UDP and DNS

`resolve(host, port)` / `resolved()`, `connect_udp(numeric_address)`,
`send_udp(handle, bytes)`, `receive_udp(handle)` and `close_udp(handle)` use the
same bounded transport as WASM mods. There are four sockets per mod, 1,200-byte
datagrams, 256 sent/received datagrams per second, and 32 packets per receive
call. DNS allows one pending lookup per mod and four globally, with a ten-second
consumer timeout. It never runs synchronously on the game thread.

Connected sockets accept only their peer. No listening sockets, multicast,
broadcast, unspecified or link-local destinations are exposed. UDP has no
built-in encryption or reliability. `random_bytes(length)` provides OS entropy,
limited to 1,024 bytes per call and 4,096 per game hook budget.

## Minecraft payloads and player events

`register_channel`, `unregister_channel`, `send_payload(player, channel, bytes)`
and `poll()` provide binary messages. Register an exact namespaced channel
first; `minecraft:*` and `horizon:*` are reserved. Up to 32 channels per mod are
allowed. The game announces the union of native and WASM subscriptions so one
runtime cannot unregister another's channel.

Clients pass `None` for the target; server mods pass an online player's UUID
from `voice.session().players`. In-frame hosts can also target their local
player. `Event::Packet` contains a verified sender UUID on the server and
`None` for the connected server on a client. Native handlers do not consume
vanilla channels or another mod's messages.

`poll()` drains `Connected`, `Disconnected`, `PlayerJoined`, `PlayerLeft`,
`PermissionsChanged` and `Packet` events. `session()` exposes server address,
local UUID, role, online UUIDs and an opaque connection generation.
`game.native_voice_players()` supplies UUIDs, names and optional tracked positions
with dimensions for per-player positional playback; untracked remote players have
no position, rather than an invented location. Server-side
hooks run on the integrated or dedicated server's own Game. Voice devices/UDP
are client services; server custom payloads do not require them.

Payloads are limited to 32,767 bytes each and 32 messages / 64 KiB per budget;
the inbox holds at most 64 events / 256 KiB of packet data and drops oldest
entries on overflow. Mods choose their own version handshake in the bytes.

## Example

[`examples/native-voice-lab`](../examples/native-voice-lab/) is a source mixin
bundle with five HEAD hooks. It lists itself, declares services, monitors the
microphone locally while V is held and offers explicit binary diagnostics:

- `/voice_request HOST PORT` creates a consent request.
- `/voice_resolve HOST PORT` starts approved DNS; `/voice_receive dns` polls it.
- `/voice_connect IP:PORT` opens an approved numeric endpoint and prints a handle.
- `/voice_ping HANDLE` sends diagnostic bytes; `/voice_receive HANDLE` polls them.
- `/voice_hello` sends version 1 on `voice_lab:hello`.

Build it with the [native mod workflow](native-mods.md) on a revision supporting
hook API 3. Trust/build approval is the player's decision. Use headphones for
the local microphone monitor. The example never transmits captured PCM.
Its helper files are type-checked against the real Game by the binary's test
configuration. Applying the bundle with horizon-mixins, physical devices and
end-to-end protocol interoperability still require integration testing.
