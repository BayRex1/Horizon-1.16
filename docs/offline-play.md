# Local offline profiles

Horizon can run a local single-player world with a launcher profile that does not sign in to Microsoft. This is a local game identity, not a Microsoft or Minecraft account. It has no online authentication token.

## Set up and play

1. Install Horizon and prepare the Minecraft version using an existing, legitimate Minecraft installation. In the launcher, choose **From my install** and point it at that installation if Horizon has not prepared the version yet.
2. Open **Settings → Accounts** and add an **Offline name**, or choose an existing offline name from the account switcher at the top of the launcher.
3. Select the Horizon instance and press **Play**. Create a world or open one of your local saves.

The command-line equivalent is:

```sh
horizon-launcher account offline Alex
horizon-launcher play
```

To use the game's generated local name instead, omit `Alex`:

```sh
horizon-launcher account offline
```

## Limits

Local profiles are single-player-only. Horizon disables Multiplayer and Friends on the title screen, rejects server joins, and blocks LAN hosting and invite sharing for these profiles. To use online servers or multiplayer features, switch to a signed-in Microsoft account that owns Minecraft: Java Edition.

Worlds and profile data remain on this computer. A local offline name is not verified and should not be treated as an authenticated online identity.
