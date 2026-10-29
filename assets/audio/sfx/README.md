# Sound effects

OGG Vorbis files only (`.ogg`), lower-case snake_case names, in this folder. The game
looks for exactly these files; a missing file is skipped (one warning in the log, no
crash, no replacement sound).

| File | Played when | Group |
|---|---|---|
| `block_place.ogg` | a block was placed | world |
| `block_remove.ogg` | a block was removed with the hammer | world |
| `shop_purchase.ogg` | a purchase succeeded | ui |
| `action_denied.ogg` | a purchase was refused | ui |
| `math_correct.ogg` | a math answer was correct | ui |
| `math_incorrect.ogg` | a math answer was wrong | ui |
| `structure_collapse.ogg` | a structure collapsed | world |
| `stage_complete.ogg` | the stage was completed | major |

Tips: short clips (under about 1.5 s; completion may be longer), no leading silence,
trim loud files in the catalog (`Sfx::trim`) rather than re-encoding. Only add sounds you
have the right to distribute, and keep their licence with the repository.
To add another sound see `docs/audio/README.md`.
