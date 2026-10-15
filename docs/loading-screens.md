# Loading screens

The game menu appears fully on its first rendered frame, while the title
backdrop loads in the background. There is no wordmark hold, opening reveal,
or initial-page fade. The first click or key acts on the menu immediately.
The plain menu background is replaced by the landscape when the backdrop is
ready, without its former 1.6-second fade. Later navigation, hover feedback,
and the live backdrop camera retain their existing behavior.

The launcher shows a static wordmark while its release request is pending.
There is no minimum hold, reveal animation, background drift, or timed fallback.
Completion or failure exposes the interface immediately, including the existing
error and retry UI. Clicking or pressing a key dismisses its loading screen while
the request continues. The persisted opening preference controls this screen.

This removes presentation delays and the game's dependency on backdrop
readiness; it does not speed up networking, resource loading, world generation,
or rendering. The game still loads its core resources and renderer before it
can draw the first frame. Classic menus and direct world/server launches retain
their existing behavior.

## Validation

Run `python3 tools/check_loading_screens.py` from the repository root for the
production launcher transitions and immediate backdrop visibility. Run `cargo test -p horizon-client --lib front::tests` for
actual game menu drawing, initial-page/navigation timing, and first-click behavior
before and after backdrop readiness. Full GUI builds cover the surrounding drawing and input-handler types.
