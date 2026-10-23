# Stage session: menu, completion, replay, controls help

Code: `crates/oopsmath-engine/src/app/session.rs` (rules, unit tests) and
`crates/oopsmath-engine/src/ui/hud/session.rs` (button, overlays, input).

## Flow

```
StageBrowser -> LoadingStage -> InGame
InGame --Restart Stage / Replay Stage--> LoadingStage -> InGame   (same selected stage)
InGame --Back to Stages-------------------> StageBrowser
```

Nothing is reset by hand. Leaving `InGame` runs the existing `OnExit(InGame)`
cleanup of every stage-local plugin; `LoadingStage` reloads the selected `.dat`
(`StageSelection`, or `OOPSMATH_STAGE_DAT`); `OnEnter(InGame)` rebuilds
everything. A load failure returns to the stage browser with the error shown.

## Overlays (`SessionOverlay`)

One at a time:

* **Menu** - Esc or the "Menu (Esc)" button (bottom left, above Plan):
  Resume, Controls, Restart Stage, Back to Stages.
* **Controls** - from the menu; Back (or Esc) returns to the menu.
* **Stage complete** - shown once when `ObjectiveTracker::all_complete()` becomes
  true (at least one objective, all complete; the existing latched semantics).
  Buttons: Replay Stage, Back to Stages, Keep Building (Esc does the same).
  If the Shop, Math Challenge or Plan popup is open at that moment (the last challenge is solved inside one), it is closed and the panel is shown. The panel waits only for the menu.

## Input rules

* Esc closes the Shop / Math Challenge / Plan popup if one is open; otherwise it
  opens or closes the menu. `handle_session_input` is ordered before those three
  handlers so one key press never does both.
* While an overlay is open (or a transition has been requested) placement,
  rotation and the hammer do not run (`SessionUi::blocks_world`, added to the
  placement run condition in `blocks/interaction.rs`). The overlay is a
  full-screen node with an `Interaction`, so clicks do not reach the HUD.
* After Restart / Back to Stages is requested, `leaving` is set: further
  requests in the same or following frame are ignored, so the transition is
  deterministic.
* Camera orbit/pan/zoom is not gated (same as the other popups).

## Not included

No score, stars, rewards, unlocking, saving, or "next stage" button.
