# Loading screens

The launcher and game show a static wordmark only while their startup work is
pending. There is no minimum hold, reveal animation, background drift, or timed
fallback. The interface appears fully on the first frame that observes readiness.

- The launcher waits for the release request to complete or fail. A failure shows
  the existing error and retry UI. Clicking or pressing a key immediately opens
  the launcher while the request continues; its existing loading message remains.
  The persisted opening preference now controls the loading screen.
- The game waits for the backdrop readiness flag, rather than its former
  1.6-second fade. The backdrop and interface appear immediately when ready.
  Clicking or pressing a key dismisses the loading screen immediately while the
  backdrop continues loading. Classic style, frame harnesses, and direct world
  or server launches retain their opening bypasses.
- The first page in both applications has no fade or slide. Animations for later
  navigation, hover feedback, and the live backdrop camera remain unchanged.

Loading duration depends on the release request or backdrop generation. There
is no automatic timeout that dismisses the loading screen before work finishes;
input skip remains available. This change removes presentation delays; it does
not speed up networking, resource loading, world generation, or rendering.

## Validation

Run `python3 tools/check_loading_screens.py` from the repository root. It compiles
actual production transition methods with minimal surrounding containers and
checks immediate readiness, pending-to-ready transitions, launcher failure,
input skip, disabled/bypassed loading screens, refresh after completion, the
initial page's lack of a fade, later navigation, and immediate backdrop visibility.
Full GUI build checks cover the surrounding drawing and event-handler types.
