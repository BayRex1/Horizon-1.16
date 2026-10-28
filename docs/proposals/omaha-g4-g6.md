# Proposal: G4–G6 server mod APIs (Omaha)

Companion to G1–G3 (see PR #3315). Omaha Voxel Edition currently works around
these with mod-side DDA, health polling, and storage-backed HUD sync.

## G4 — `world.raycast`

Server mods need a single ray through blocks (and optionally entities) for
hitscan weapons. Client already has `game.raycast` (blocks only).

See `omaha-g4-g6.wit.fragment` for the proposed WIT surface.

## G5 — player hurt / died hooks

`player-hurt` / `player-died` so battle mods can queue respawn waves without
polling health every tick.

## G6 — lightweight net channel

`net.send-to-player` / `net.poll` for HUD sync payloads without inventing a
parallel protocol. Vanilla clients ignore unknown channels.

## Status

- Design / WIT sketch only in this PR (no host implementation yet).
- Omaha ships soft-pen + mod-side ray DDA until G4 lands.
