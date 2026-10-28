# Ely.by accounts

Horizon's launcher signs in with an [Ely.by](https://ely.by) account beside a Microsoft one, and the
game joins servers that check players against Ely.by's session server. It reuses the Microsoft
sign-in's device-code flow, keychain and session file ([docs/microsoft-account.md](microsoft-account.md));
this note lists what differs. The way ElyPrismLauncher signs in is the reference for the endpoints.

## What the maintainer registers (needs their account)

An agent can't do this: the application belongs to a person's Ely.by account.

1. Register an application in Ely.by's developer area (https://account.ely.by/dev), a public client
   with no secret, and copy its **client ID**.
2. Give the ID to the launcher: the GitHub repository variable `HORIZON_ELY_CLIENT_ID` (a client ID
   is public, not a secret; the build reads it with `option_env!`), or at run time the environment
   variable `HORIZON_ELY_CLIENT_ID`, which overrides it (empty or unset = not configured). A build
   without it shows no Ely.by sign-in.

Never use another application's client ID (ElyPrismLauncher's `elyprism-launcher` included): there
is no default.

## Flow (launcher)

OAuth 2.0 device code at `https://account.ely.by/api/oauth2/v1`, scopes
`account_info offline_access minecraft_server_session`:

1. `POST …/devicecode` (`client_id`, `scope`): the launcher shows the `user_code` and the
   `verification_uri` (https://account.ely.by/code).
2. Poll `POST …/token` (`grant_type=urn:ietf:params:oauth:grant-type:device_code`), as for Microsoft.
   Result: the access token (24 hours) and a refresh token.
3. `GET https://account.ely.by/api/mojang/services/minecraft/profile` with the access token: the
   account's UUID and name. That is the whole sign-in: there is no Xbox Live, no entitlements.

The `minecraft_server_session` scope makes the access token a Minecraft session, so it is both what
the launcher keeps and what the game sends. Renewal is the same as Microsoft's: when less than an
hour is left, `POST …/token` with `grant_type=refresh_token`; Ely.by's refresh tokens don't expire
and the answer may carry no new one, in which case the old one stays. A refused refresh token
(`invalid_grant`) asks the player to sign in again.

The device-code endpoints are not in Ely.by's public documentation; they are the ones
ElyPrismLauncher uses.

## The owner gate and the download

Decided 2026-10-28 with the contributor adding it: an Ely.by account passes the owner gate. There is no
ownership to ask Ely.by about, so the check at every Play is that the token (renewed if due) still
gets the account's own profile; no network keeps the last answer, as for Microsoft.

What an Ely.by account doesn't do is download the game: "Download and prepare" fetches from Mojang
only for a Microsoft account that owns Minecraft (`launcher/src/backend.rs`, `download_for_owner`
asks only those). With only Ely.by accounts, Home offers **Prepare** from the player's own install.
Offline names play beside an Ely.by account, as beside a Microsoft one.

## The game

The session file says `"user_type": "ely"` (`launcher/src/account.rs`, `write_session_file`; no
`xuid`). The game (`crates/horizon-client/src/auth.rs`) then joins an online-mode server with
`POST https://authserver.ely.by/session/join` and the same body as Mojang's (`accessToken`,
`selectedProfile`, `serverId`) instead of the services discovery document. Its refusals read like
Mojang's (`ForbiddenOperationException`). `HORIZON_ELY_JOIN` points the request at another URL (a
mock in harnesses).

## What works and what doesn't

- A server must check players against Ely.by (authlib-injector with Ely.by, or Ely.by's own authlib
  patch) to take an Ely.by account; a server that checks Mojang's session server doesn't, and an
  Ely.by account can't join it. Offline-mode servers and LAN take any name.
- Skin and cape of an Ely.by account are not shown yet: the launcher doesn't fetch from Ely's texture
  server, and the game looks players up through Mojang's.
- `horizon-client`'s server side (`has_joined`) still asks Mojang: a Horizon server in online mode
  doesn't take Ely.by players.
- Not checked against a live Ely.by account yet: the device-code endpoints and the join request were
  written from ElyPrismLauncher's source and Ely.by's documentation, and tested against local mocks
  (`launcher/src/account/tests.rs`, `crates/horizon-client/src/auth.rs`).

## Tokens

The same as for Microsoft: the keychain (service `dev.playhorizon.launcher`, one item per account,
the account's UUID), `accounts.json` keeps names, UUIDs and, for Ely.by accounts only, `"provider":
"ely"`. Tokens never reach logs, errors, CLI output or the command line. Sign out removes them.

## Commands

`horizon-launcher account sign-in --ely`, then `account list`, `select` and `sign-out` as for any
account. MCP: `account_sign_in` with `provider: "ely"`, `account_list` (each account's `provider`).
