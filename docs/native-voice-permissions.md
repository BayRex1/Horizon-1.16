# Design proposal: microphone and scoped networking for native voice mods

> Status: proposal for discussion; these services and permissions are not implemented.

Following the feedback on [the experimental WebAssembly audio/networking PR](https://github.com/horizon-rust/horizon/pull/3276#issuecomment-6488969381), this proposal targets native mods and the stable hook points introduced in [#3345](https://github.com/horizon-rust/horizon/pull/3345), as described in [native-mods.md](native-mods.md).

The goal is reusable audio and networking support that a separate mod could use to implement Simple Voice Chat compatibility. This proposal does not implement that protocol or ask Horizon to include voice chat itself.

## Trust boundary

Native mods can modify compiled game code and run with the player's OS rights. They can bypass Horizon helpers and open sockets or microphone devices directly. The controls below therefore govern Horizon-managed services for cooperating, reviewed mods; they are not an enforceable sandbox for arbitrary native code.

The existing native-code trust warning remains necessary, including at build time: runtime permission prompts do not constrain build scripts or other native code. The UI must explain this distinction. If enforceable per-mod restrictions are required, a separately isolated process and an appropriate OS sandbox would need a separate design before making that guarantee.

## What the player approves

Installing, enabling, or building a native mod does not grant access through these services. A bundled server message cannot grant permission either. The player approves each mod separately, in each instance.

| Capability | Prompt and scope | Default |
| --- | --- | --- |
| Microphone | Mod name, author, version, purpose; capture from the selected input device | Denied |
| Voice UDP | Mod identity, current Minecraft server, destination hostname/IP and exact UDP port, public or local network classification | Denied |
| External voice server | Separate approval when the destination is outside the approved Minecraft server address set | Denied |
| Local-network UDP | Explicit approval for specific private, loopback or link-local endpoints; public-server approval never includes them | Denied |
| DNS | Resolution only for the exact hostname in an approved endpoint request; show that resolution discloses that name to the configured resolver | Denied with the endpoint |

Present microphone and network choices independently, with “Allow this session”, “Remember for this instance”, and “Deny”. Explain that microphone approval exposes raw audio to the mod. PTT controls when approved capture runs; it does not replace permission. Voice activation is a separate opt-in setting, with PTT as the proposed default.

A trusted Horizon indicator shows which mods currently capture audio, with an immediate stop control. The permissions page lists remembered destinations and allows revocation. No prompts or device work run on the audio callback thread.

Horizon approval does not override OS microphone privacy settings. Device enumeration and capture
require both grants; an OS denial produces a permission error, without repeatedly opening prompts.
The selected device is an instance setting; unplugging it stops capture rather than silently switching
to another input. Input gain changes only the mod's PCM stream, not the system microphone volume.
PTT releases on focus loss and disconnect. Playback has separate per-mod and per-source volume
controls and respects the game's master volume.

## Destination rules

- A Minecraft custom payload may propose a voice destination, but is untrusted input. It cannot silently expand an approval, change its port, or enable a microphone.
- An endpoint request contains one exact hostname or numeric IP and one UDP port. No wildcard hostname, arbitrary DNS-query API, port range, listening socket, multicast or broadcast is exposed initially.
- Endpoint approval allows the hostname to be resolved. Before opening a socket, validate and pin the resulting numeric address. Display the numeric destination in the permission UI; any newly resolved address requires session approval before use. Private, loopback, link-local and IPv4-mapped IPv6 forms are classified consistently and require the explicit local-network grant.
- “Current server” means the validated numeric peer address set of the Minecraft connection. Another hostname, an SRV result, a server-advertised voice host or a different port does not inherit that connection's approval automatically. The exact proposed voice port is still shown for approval.
- DNS uses the system resolver on a worker, with timeouts and bounded outstanding requests. Denied requests perform no DNS lookup. Revalidation happens before every new socket, including reconnects, to prevent a remembered hostname from silently reaching another address.
- UDP sockets are connected to a validated peer; receiving is limited to that peer. Disconnect closes sockets and clears queues. An independently approved external endpoint still needs an active session in this initial design.

## Permission identity and revocation

Remembered grants are keyed by instance, mod ID, reviewed bundle digest, requested capability and endpoint scope. A changed bundle requires renewed consent; matching only a mod ID is insufficient. An expanded declaration always requires approval. The build records which bundles are present so the UI can show the requesting identity; this attribution still depends on cooperating native code.

Revocation immediately stops Horizon-managed capture, closes that mod's sockets and cancels pending DNS work. Completion callbacks check a permission/session generation so stale worker results cannot reopen resources. Buffered PCM and received packets are discarded. Already delivered audio or transmitted packets cannot be recalled.

Leaving the server releases session resources and session-only grants. Persistent approval permits a later request within the same scope, but does not start capture automatically. Disabling or removing a mod clears its grants; an already compiled native build requires a rebuild to remove the code itself.

## Native hooks and helpers

Extend the additive native hook API rather than the WIT world. Exact signatures should be agreed in follow-up PRs. Proposed responsibilities are:

- Connection and disconnection events with a session ID, server identity and local player UUID.
- Registration and dispatch of exact namespaced Minecraft custom payload channels, with owned binary data and existing protocol size checks; outgoing payloads use the active connection.
- Permission requests and state-change events, attributed to a registered native mod declaration.
- Bounded PCM capture and playback helpers, input/output device selection, per-source gain and optional positional playback using existing game audio infrastructure.
- Asynchronous endpoint approval, DNS, connected UDP send/receive and explicit close helpers.

Use tick/frame hooks to deliver bounded owned events on the game thread. Device and network workers must not call mod hooks with borrowed game state. Audio callbacks consume bounded queues, without invoking mod callbacks, prompting, or waiting on the game thread. Overflow drops data with a visible diagnostic counter rather than accumulating latency.

Use finite per-mod and global limits for sockets, DNS jobs, packet size/rate, event queues and PCM buffers. Proposed starting limits for discussion are four sockets and four pending DNS jobs per mod, 1,200-byte UDP payloads and at most 200 ms of queued PCM per source. Packet/byte rate limits and global budgets need agreement and interoperability measurements before implementation. These are helper safeguards, not limits on arbitrary native code.

All pending requests are deduplicated by mod, session and scope, so a tick hook cannot generate a
prompt every frame. A denial lasts for the session unless the player changes it in settings. Helpers
return explicit errors for denied permission, unsupported audio builds, a disconnected session,
invalid endpoints and exhausted quotas; they do not retry with broader access. Playback never feeds
back into captured input through the helper itself; any echo cancellation is a separate feature.

The audio, permission and worker implementation can live outside `native_hooks.rs`. Hook bodies
must preserve the existing no-macro/no-`cfg` requirement used by horizon-mixins. Every additive hook
change must update `NATIVE_HOOKS_API` and continue to support builds without the WebAssembly runtime.

## Delivery and acceptance

1. Agree on the trust statement, prompt wording, grant identity and endpoint rules in this issue.
2. Add the smallest native hooks and permission state/UI changes, independently of audio or networking backends.
3. Add microphone/playback helpers and a local PTT monitor example.
4. Add scoped DNS/UDP helpers and an endpoint diagnostic example.
5. Add custom payload hooks and a separate voice compatibility mod; evaluate protocol compatibility, authentication, encryption and codecs there.

Focused validation should cover denial without DNS/device access, per-mod and per-instance isolation of helper grants, changed bundle reapproval, endpoint changes, rebinding and local-address classification, disconnect/revoke during pending work, stale callback suppression, bounded queues, multiple mods, and builds with and without optional audio support.

The existing WebAssembly implementation can serve as a reference for requirements, but should not be rebased wholesale over the native-mod transition.

## Decisions before implementation

1. Is a clearly documented cooperative permission model acceptable for reviewed native mods, or must these services wait for enforceable OS isolation?
2. Are exact endpoint approval, renewed approval for changed resolved addresses, and separate local-network consent the desired defaults?
3. Should the first implementation expose managed native helpers, or only the connection/payload hooks while the permission UI is agreed?

