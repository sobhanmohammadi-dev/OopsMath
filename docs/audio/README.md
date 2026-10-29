# Sound effects (SFX)

One-shot sound effects, event-driven, loaded from OGG files in `assets/audio/sfx/`.
No music, no settings screen (both are future milestones; the volume numbers they
need are already centralized, see "Volume").

## Files

| File (`assets/audio/sfx/`) | Played when |
|---|---|
| `block_place.ogg` | a block was placed |
| `block_remove.ogg` | a block was removed with the hammer |
| `shop_purchase.ogg` | a purchase succeeded |
| `action_denied.ogg` | a purchase was refused |
| `math_correct.ogg` | a math answer was correct |
| `math_incorrect.ogg` | a math answer was wrong |
| `structure_collapse.ogg` | a structure collapsed |
| `stage_complete.ogg` | the stage was completed |

Format: **OGG Vorbis**, lower-case snake_case names. Bevy's default `audio` feature already
includes the Vorbis decoder; nothing was added to `Cargo.toml`. Do not rename files: the
names are the stable convention. The folder is separate from textures, models and stages,
and from the `assets/audio/music/` folder a later milestone can add.

**Missing files are safe.** The game logs one warning listing them (with what each is for),
skips those sounds, and carries on. Nothing is substituted. To see which files are still
missing: `cargo test -p oopsmath-engine --test audio_contract -- --nocapture`.

## Architecture (`crates/oopsmath-engine/src/audio/`)

```
committed game state ──► edges::SfxEdges ──► PlaySfx message ──► play_sfx ──► Bevy audio
```

* `catalog.rs` - `Sfx` (typed ids), file names/paths, group, level trim, cooldown, purpose;
  `SfxVolumes` (volume configuration). The only place that mentions file names.
* `edges.rs` - pure, tested logic that turns *changes of the authoritative state* into
  sounds. It needs no change to gameplay code and cannot be triggered by previews, redraws
  or conditions that stay true.
* `plugin.rs` - `SfxPlugin`: loads every file once at startup (handles are reused), reports
  once which loaded and which are missing, and plays `PlaySfx` requests.
* Any code can request a sound: `requests.write(PlaySfx(Sfx::ActionDenied))` with a
  `MessageWriter<PlaySfx>`.

## Event and repetition policy

All sounds are one-shot (`PlaybackMode::Despawn`: the entity removes itself when the sound
ends). Playing a sound never stops another. Each distinct sound plays at most once per frame.
The first observation of a stage run (entering, restarting, loading) only records a baseline,
so state reconstruction is silent.

| Sound | Edge that triggers it | Never triggered by |
|---|---|---|
| block placed | placed-block count +1 | cursor moves, preview, rejected placement |
| block removed | placed-block count -1 (collapse only moves blocks) | hammer on empty space, failed removal |
| purchase | wallet balance decreased (only the shop spends money) | opening the shop |
| purchase refused | shop result message set, nothing else about the shop changed, no money spent | opening/closing the popup, changing the quantity, redraws |
| math correct | solved-challenge count +1 | the result staying on screen |
| math wrong | challenge feedback set to the "not correct" text, nothing else changed | typing, opening, redraws, other results (empty, malformed, already solved) |
| collapse | falling-block count increases (a new collapse started) | every block of the same collapse, per-frame stability checks |
| stage complete | objectives complete: false -> true (latched) | repeated completion checks |

Rules: rapid placements/removals are **not** rate limited (no global cooldown). Only
`action_denied` (150 ms) and `structure_collapse` (400 ms) have a per-sound guard. When the
answer that finishes a stage is submitted, `stage_complete` replaces `math_correct` so the two
never stack. A collapse that makes many blocks fall at once is one sound.

## Volume

`SfxVolumes` (master, world, ui, major) times the per-sound `Sfx::trim`, clamped to 0..=1 (a
sound is never amplified above its file). Defaults are conservative: master 0.8, world 0.7,
ui 0.8, major 0.9; trims 0.7-1.0. The files are not assumed to be equally loud: adjust
`Sfx::trim` per sound. A later settings screen only has to change the `SfxVolumes` resource.

## Adding another sound

1. Put `my_sound.ogg` in `assets/audio/sfx/`.
2. Add a variant to `Sfx` in `catalog.rs`: add it to `ALL`, `file_stem`, `group`, `trim`,
   `min_interval`, `purpose` (the exhaustive test fails until you do).
3. Trigger it: write `PlaySfx(Sfx::MySound)` from the system that commits the action, or
   add an edge in `edges.rs` (with a test) when the game exposes the result as state.
4. Document it in the tables above and in `assets/audio/sfx/README.md`.

## Limits and what is not verified

* No sound was heard while developing this (no audio hardware or game session): loading,
  levels and timing must be checked by ear once the files exist.
* The "wrong answer" and "refused purchase" edges compare the resource with its previous
  value. A key press that changes nothing but still writes to the resource (typing at the
  maximum answer length right after a wrong answer) could replay the wrong-answer sound.
  `tests/audio_contract.rs` pins the feedback text the sound depends on.
* The edge for "purchase refused" fires for every refused attempt, not only insufficient
  funds (the game reports all refusals as one message).
* Playback needs an output device; without one Bevy logs it and the game continues silently.
