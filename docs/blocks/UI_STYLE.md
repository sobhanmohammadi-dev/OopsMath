# UI style layer

Code: `ui/theme.rs` (`UiTheme`), `ui/style.rs` (`StylePlugin`), and the shared
use in `ui/hud/session.rs`. Panels keep their layouts; only colours and a few
shared helpers were unified.

## Palette (`UiTheme`)

| Role | Fields |
|---|---|
| Panels | `panel` (94 % opaque so the scene shows through a little), `panel_hover`, `selected`, `background` (menus) |
| Secondary buttons | `button`, `button_hover`, `button_pressed` |
| Primary buttons | `primary`, `primary_hover`, `primary_pressed` |
| Text | `text`, `secondary_text`, `disabled_text`, `accent` |
| Status | `success`, `warning`, `error` |
| Modals | `overlay` (dimming layer) |

Unit tests check that text, secondary text, accent and the status colours keep a
contrast ratio of at least 4.5 against the panel, and that primary and secondary
buttons differ in every state.

## Shared helpers (`ui/style.rs`)

* `text::{TITLE, HEADING, BODY, BUTTON, HELPER}` and `space::*` - one size and
  spacing scale. `font(size)`.
* `modal_overlay(theme)` / `modal_panel(theme, width)` - full-screen dimming
  layer (with an `Interaction`, so clicks do not reach the HUD) and its panel.
* `dialog_button(parent, theme, label, ButtonKind, marker)`.
* `PrimaryButton` - the generic hover system recolours every `Button` with the
  secondary colours; `primary_button_interaction` runs after it and restores the
  primary colours, and `paint_new_primary_buttons` colours a fresh primary button
  before it is first drawn.

Used so far: the menu, controls help and stage-complete overlays
(`ui/hud/session.rs`), the Submit button of the Math Challenge and the Buy button
of the Shop (primary), and the overlay colour of the Shop, Math Challenge and
Plan popups. Other panels still use their own spacing and sizes.

## Feedback line

`PlacementFeedback` (placement, hammer and collapse messages) is cleared
`FEEDBACK_SECONDS` (6 s) after it last changed. The timer restarts on every
change, so a newer message is never cleared by an older message's deadline
(`FeedbackClock`, with unit tests). Messages have no severity, so they share one
colour.

## Fonts and languages

* The UI uses Bevy's built-in default font. The repository bundles no font
  (`assets/` holds only `log.glb`), and none was added: no font with a verified
  licence is available in the project.
* That default font has no Persian glyphs. A stage localized to `fa` (the
  example ships `fa.ftl`; the locale is chosen with the environment variable
  defined in `stage/localization.rs`) would show missing glyphs. Right-to-left
  layout and Arabic-script shaping were not inspected or tested, and are not
  claimed to work.
* To add a font later: place it in `assets/fonts/`, load it once, and set it in
  `ui::style::font` (every new UI text goes through it). Verify the licence and
  glyph coverage first.

## Window

`crates/oopsmath-game/src/main.rs` sets the window title to "OopsMath". Size,
mode and icon are unchanged (no icon asset exists).

## Not done

No icons, borders, animation, disabled-button styling (no panel has a disabled
button state today), layout rework or responsive layout. HUD panels are still
fixed width (top left up to 420 px, right side 320 px).
