# Math challenges (v1)

Code: `challenges/` (`answer.rs` rules, `mod.rs` model/state/panel),
`ui/hud/challenge.rs` (button, popup, input).

## Stage data (additive in Compiler 1.1; DAT unchanged)

A challenge is an existing `objectives.tasks` entry of `type: math` with a
`math.answer` (already validated by the compiler). The task id is the challenge id.
The prompt is a localization key: `math.prompt` if present, otherwise the stage's
`learning.question` (stage 001 works without any new field: `t_math`, exact answer
12). `math.prompt` was rejected by Compiler 1.0 (the `math` object allowed no extra
keys) and is accepted since 1.1.

```yaml
learning: {question: stage.001.question}
objectives:
  tasks:
    - id: t_math
      type: math
      math: {type: multiplication, answer: {mode: exact, value: 12}}
```

Math tasks whose mode is not supported (`expression`, `multiple_choice`, custom,
generators) are skipped with a log warning. Malformed definitions (duplicate id,
bad value or tolerance, no prompt) disable the stage's challenges and are logged.
No math tasks = no challenges, and the button is hidden.

## Answer rules (`challenges/answer.rs`)

* **Integer**: `mode: exact` with an integer `value` (fits `i64`). Input: optional
  sign + digits; `12.0`, `1e3`, `1 2` are malformed, too-large numbers are out of
  range. Compared as integers.
* **Decimal**: `mode: exact` with a non-integer value, or `mode: tolerance`
  (`value`, `tolerance` finite and >= 0). Input: optional sign, digits, optional
  `.digits` (no `.5`, `5.`, exponents, `inf`, `nan`). Without tolerance, answers
  are equal when both parse to the same `f64` (so `2.5` = `2.50`); there is no
  epsilon. With tolerance: `|input - value| <= tolerance`.
* Input is trimmed; empty, malformed or out-of-range input is an error, never
  "solved" and never "wrong".

## Runtime

`StageChallenges` (definitions) and `ChallengeTracker` (solved flags) are rebuilt
when a stage becomes active and cleared when it exits; ordinary gameplay never
resets them. `challenges::submit` is the only transition to solved and happens at
most once (`AlreadySolved` afterwards). Solving changes no money, inventory,
blocks or selection.

## UI

A **Math Challenge** button (bottom-left, above Shop; hidden without challenges)
opens a popup on the first unsolved challenge: prompt, `Answer:` field, Submit,
Prev/Next when there are several, feedback, Close. There is no text-input widget
in the project, so keyboard messages feed the field while the popup is open:
digits, `.`, `+`, `-`, Backspace, **Enter** submits, **Escape** closes. The
overlay blocks the HUD and world; placement, rotation and the hammer do not run
while it is open. The popup is rebuilt on each change (including keystrokes), not
every frame. It closes when the stage exits; a stale popup cannot submit for another
stage because the target is resolved by index against the current stage.

## Objective prerequisite

An objective may add `requires_challenge: <task id>` to its `data`:

```yaml
data: {kind: place_block, block: block.wood_log, count: 12, requires_challenge: t_math}
```

It completes only when the construction count **and** the challenge are both
satisfied (neither completes the other). While built but unsolved the HUD shows
"solve the math challenge to finish". Completion stays latched as before.
A reference to a missing challenge disables the objective list with a diagnostic
(runtime) and is a compile error (Python compiler: must name a `type: math` task).
Challenges never depend on objectives, so there are no cycles.
Not implemented: rewards, unlocks, fractions, expressions, multiple choice.
