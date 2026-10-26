# OopsMath - core game strings (English, the source language).
#
# Ids are lower-case words separated by '-': area-name. Variables look like
# { $name }. Add a '# comment' line above a message to tell translators where
# it appears; the comment is copied into the .pot file.

## Main menu

menu-title = OOPSMATH
menu-play = PLAY
# The button that cycles through the installed languages. $name is the native
# name of the current language.
menu-language = Language: { $name }

## Stage browser

browser-title = SELECT A STAGE
browser-scanning = Scanning...
browser-select-hint = Select a stage to see its details.

## In-game HUD

hud-blocks = BLOCKS
hud-inventory = INVENTORY
hud-objectives = OBJECTIVES
hud-objectives-complete = All objectives complete!
hud-shop = Shop
hud-math = Math Challenge
hud-plan = Plan
hud-menu = Menu (Esc)

## Shop, math challenge and plan windows

popup-shop-title = SHOP
popup-math-title = MATH CHALLENGE
popup-plan-title = CONSTRUCTION PLAN
popup-close = Close
popup-submit = Submit
popup-solved = Solved!
popup-quantity = Quantity:
popup-no-blocks = No blocks are for sale in this stage.
popup-not-enough-money = Not enough money

## In-game menu and stage completion

session-menu-title = MENU
session-resume = Resume
session-controls = Controls
session-restart = Restart Stage
session-back-to-stages = Back to Stages
session-discard-warning = Restart and Back to Stages discard your progress in this stage.
session-controls-title = CONTROLS
session-back = Back
session-complete-title = STAGE COMPLETE!
session-complete-body = You finished every objective of this stage.
session-replay = Replay Stage
session-keep-building = Keep Building

## Controls help (a name and a description per control)

help-select-name = Choose what to use
help-select-text = Click a block in BLOCKS (top right) or an item in INVENTORY (bottom right).
help-place-name = Place a block
help-place-text = Left click on the ground or on the face of a placed block. A ghost shows where it will go.
help-rotate-name = Rotate a block
help-rotate-text = R turns the selected block a quarter turn (only blocks that can rotate).
help-remove-name = Remove a block
help-remove-text = Select the Hammer in INVENTORY, then left click a placed block. You get the block back.
help-camera-name = Camera
help-camera-text = Hold the right mouse button and drag to orbit, the middle button to pan, the wheel to zoom.
help-shop-name = Shop
help-shop-text = Shop button (bottom left): buy blocks with your coins.
help-math-name = Math Challenge
help-math-text = Math Challenge button (bottom left, if the stage has one): type the answer, Enter or Submit.
help-plan-name = Plan
help-plan-text = Plan button (bottom left, if the stage has one): shows what a blueprint needs. Nothing is built for you.
help-objectives-name = Objectives
help-objectives-text = The stage panel (top left) lists what you must do. Finish them all to complete the stage.
help-stability-name = Stability
help-stability-text = Blocks must touch the ground or another block. If their support is removed they fall.
help-menu-name = Menu and windows
help-menu-text = Esc closes an open window; otherwise it opens this menu. Back to Stages leaves the stage.

## Names of blocks and tools. Add one `block-<id>` / `tool-<id>` message per block or tool: the id is the block id with '.' turned into '-' (block.wood_log -> block-wood_log).

block-wood_log = Wood Log
tool-hammer = Hammer
# What kind of item a hotbar row is.
kind-block = Block
kind-tool = Tool

## HUD lines built in code. IMPORTANT: for the messages below the English text is
## matched against the text the game produces (see docs/i18n/README.md, "Text built in code"):
## do not change the English wording or the variable names without changing the code.

hud-no-stage = No stage is loaded
hud-rotation = Rotation: { $degrees } deg (R)
hud-stage-data-unavailable = Stage data is unavailable.
hud-starting-invalid = Starting resources are invalid: { $error }
hud-rules-invalid = Block rules are invalid: { $error }
hud-inventory-empty = Inventory is empty.
hud-none-in-inventory = You have no { $name } in your inventory.
hud-selected = Selected: { $name }
hud-selected-none = Selected: none
hud-selected-item = Selected: { $name } ({ $kind })
hud-no-selectable-blocks = This stage has no selectable blocks.
hud-none-registered = None of this stage's blocks are registered, so none can be selected.
hud-block-list-invalid = The stage's block list is invalid: { $error }
hud-tool-list-invalid = The stage's tool list is invalid: { $error }
# A block in the BLOCKS list.
hud-block-row = { $name } - { $price } coins
hud-block-row-max = { $name } - { $price } coins (max { $max })
hud-block-row-min = { $name } - { $price } coins (min { $min })
hud-block-row-minmax = { $name } - { $price } coins (min { $min }, max { $max })
# A row of the INVENTORY list.
hotbar-row = [{ $kind }] { $name } x{ $quantity }
hotbar-unavailable = [{ $kind }] { $id } x{ $quantity } - unavailable: { $reason }
hotbar-not-allowed = not allowed in this stage
hotbar-not-registered-block = not a registered block
hotbar-not-registered-tool = not a registered tool
hud-in-inventory = In inventory: { $owned }
hud-placed = Placed: { $placed }
hud-placed-max = Placed: { $placed } / { $max }

## Objectives

# $mark is the next message when there is something to add.
objective-line = { $title }: { $progress } / { $required }{ $mark }
objective-done = (done)
objective-awaiting = - solve the math challenge to finish
objectives-invalid = Objectives are invalid: { $error }

## Math challenge

challenge-counter = Challenge { $index } / { $count }
# The underscore is the text cursor; keep it at the end.
challenge-answer = Answer: { $input }_
challenge-already-solved = Already solved.
challenge-incorrect = Not correct. Try again.
challenge-unavailable = This challenge is no longer available.

## Shop

shop-money = Money: { $amount } coins
shop-product = { $name } - { $price } coins each (you own { $owned })
shop-buy = Buy { $quantity } for { $total } coins
shop-too-expensive = Too expensive
shop-bought = Bought { $quantity } x { $name } for { $total } coins.
err-quantity = Quantity must be at least 1.
err-not-registered = '{ $id }' is not a registered block.
err-not-sold = '{ $id }' is not sold in this stage.
err-no-price = '{ $id }' has no valid price and cannot be bought.
err-total-too-large = The total price is too large.
err-not-enough-money = Not enough money: you need { $needed } coins but have { $balance }.
err-inventory-full = Your inventory cannot hold that many.

## Placing and removing blocks

err-no-selection = No block is selected.
err-not-allowed = '{ $id }' is not allowed in this stage.
err-block-unregistered = Block '{ $id }' is not registered.
err-outside = That position is outside the build area.
err-occupied = That space is already occupied.
err-no-support = No support: build on the ground or against another block.
err-none-left = You have no '{ $id }' left in your inventory.
err-limit = Placement limit reached: this stage allows at most { $max } of '{ $block }'.
err-invalid-placement = Invalid placement: { $reason }
place-could-not = Could not place { $name }: { $error }
place-success = Placed { $name } at ({ $x }, { $y }, { $z })
remove-none = There is no placed block to remove.
remove-cannot-return = The block could not be returned to your inventory.
remove-success = Removed { $name }; 1 returned to your inventory.
collapse = The structure collapsed: { $count } block(s) fell.

## Construction plan (blueprints)

plan-button = Plan: { $built } / { $total } built
plan-counter = Plan { $index } / { $count }
plan-none = This stage has no blueprints.
plan-invalid = This stage's blueprints are invalid: { $error }
plan-size = Size: { $width } wide x { $height } high x { $depth } deep (cells)    Blocks: { $blocks }    Built: { $built } / { $total }
plan-materials = Materials
plan-line = { $name }: need { $required }, have { $available }{ $short } ({ $price })
plan-more-needed = , { $count } more needed
plan-price-each = { $unit } coins each = { $cost } coins
plan-price-each-large = { $unit } coins each (total too large)
plan-no-price = no price
plan-estimated = Estimated cost for everything: { $total } coins
plan-estimated-unavailable = Estimated cost: unavailable
plan-still-to-buy = Still to buy (what you do not own yet): { $missing } coins
plan-cannot-build = Cannot build: { $issue }
plan-note = Note: { $issue }
plan-issue-limit = The plan needs { $required } of '{ $block }' but this stage allows at most { $max }.
plan-issue-no-price = '{ $id }' has no valid price, so the cost cannot be estimated.
plan-issue-too-large = The estimated cost is too large to calculate.
plan-footer = Nothing is charged or placed by this plan; you build it block by block.
plan-stop-guide = Stop guide
plan-start-guide = Start building (show guide)
plan-cannot-be-built = This plan cannot be built in this stage.

## Stage browser

browser-back = BACK
browser-rescan = RESCAN
browser-play-stage = PLAY STAGE
browser-pick-first = Select a stage before pressing Play Stage.
browser-no-stage = No stage is selected.
browser-load-failed = Failed to load '{ $path }': { $error }
browser-count = { $count } stage(s) found in { $dir }
browser-level = Level { $level }
browser-grade = Grade { $min }
browser-grades = Grades { $min }-{ $max }
browser-grade-plus = Grade { $min }+
browser-up-to-grade = Up to grade { $max }
detail-overview = OVERVIEW
detail-learning = LEARNING
detail-environment = ENVIRONMENT
detail-rewards = REWARDS
# The labels of the details panel ('Label: value').
label-id = ID
label-level = Level
label-difficulty = Difficulty
label-grades = Grades
label-topics = Topics
label-locales = Locales
label-question = Question
label-preset = Preset
label-world-size = World size
label-world-origin = World origin
label-tasks = Tasks
label-money = Money
label-xp = XP

## Math challenge answers

challenge-correct = Correct! Challenge solved.
answer-empty = Type an answer first.
answer-malformed = Enter a number, for example 12 or 3.5.
answer-out-of-range = That number is out of range.
