//! In-game placement: aim, preview, confirm.
//!
//! Flow each frame while in game: [`update_target`] converts the cursor into a
//! target cell and a verdict (from the pure `evaluate_placement`);
//! [`sync_preview`] keeps one ghost entity in step; [`draw_target_outline`]
//! shows a green/red cell box; [`place_on_click`] re-validates and places on
//! left click. Placement never runs while the cursor is over HUD nodes.
//! Nothing here handles inventory, prices or removal. The allow-list comes
//! from the stage-aware HUD model (the stage's allowed blocks that are also
//! registered).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::world_serialization::WorldAssetRoot;

use super::assets::BlockAssets;
use super::camera::OrbitCamera;
use super::debug_scene::draw_box;
use super::definition::BlockId;
use super::grid::GridCoord;
use super::grid::{GridRotation, GridSpec, footprint_box};
use super::occupancy::{
    PlacedWorld, PlacementRejection, commit_placement, evaluate_commit, remove_block,
};
use super::placement::PlacedBlock;
use super::registry::BlockRegistry;
use super::rendering::spawn_block;
use super::selection::SelectedBlock;
use super::stage_rules::StagePlacementRules;
use super::targeting::ray_pick_occupied;
use crate::app::session::SessionUi;
use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blueprint::BlueprintMode;
use crate::challenges::ChallengePanel;
use crate::items::inventory::Inventory;
use crate::items::shop::ShopState;
use crate::items::tools::{ToolKind, ToolRegistry};
use crate::items::{ItemRef, SelectedItem};
use crate::scene::StageScene;
use crate::structure::check_placement_support;

/// Result of aiming this frame. `cell` is `None` when the ray misses, the
/// cursor is outside the window, or the cursor is over the HUD.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct PlacementTarget {
    pub cell: Option<GridCoord>,
    /// Orientation used for the verdict, preview and placement (always one the
    /// selected block allows).
    pub rotation: GridRotation,
    pub verdict: Option<Result<(), PlacementRejection>>,
}

/// The orientation the player has dialled in with R. Per-session UI state:
/// it is only a request; [`update_target`] resolves it against the selected
/// block's allowed rotations, and it never touches occupancy or inventory.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementRotation(pub GridRotation);

/// The placed block under the cursor while the hammer is selected. Holds only
/// an entity that is validated against [`PlacedWorld`] before use.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HammerTarget(pub Option<Entity>);

/// What a left click means for the current selection (never both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    /// Block selected (or nothing: the placement path reports that).
    PlaceBlock,
    /// The hammer is selected.
    UseHammer,
    /// Some other tool: no click behaviour yet.
    Nothing,
}

pub fn click_action(selected: Option<&ItemRef>, tools: &ToolRegistry) -> ClickAction {
    match selected {
        Some(ItemRef::Tool(id)) => match tools.get(id).map(|def| def.kind) {
            Some(ToolKind::Hammer) => ClickAction::UseHammer,
            None => ClickAction::Nothing,
        },
        _ => ClickAction::PlaceBlock,
    }
}

/// Short message for the player (last placement result).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct PlacementFeedback(pub Option<String>);

/// The ghost model. Not a `PlacedBlock` and never in `PlacedWorld`.
#[derive(Component)]
struct PlacementPreview {
    block: BlockId,
}

pub struct BlockPlacementPlugin;

impl Plugin for BlockPlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacedWorld>()
            .init_resource::<PlacementTarget>()
            .init_resource::<PlacementFeedback>()
            .init_resource::<PlacementRotation>()
            .init_resource::<HammerTarget>()
            .add_systems(OnEnter(AppState::InGame), reset_rotation)
            .add_systems(OnExit(AppState::InGame), reset_rotation)
            .init_resource::<StagePlacementRules>()
            .add_systems(OnEnter(AppState::InGame), init_placement_rules)
            .add_systems(OnExit(AppState::InGame), clear_placement_rules)
            .add_systems(OnEnter(AppState::InGame), reset_placement)
            .add_systems(OnExit(AppState::InGame), reset_placement)
            .add_systems(
                Update,
                (
                    rotate_placement,
                    update_target,
                    update_hammer_target,
                    sync_preview,
                    draw_target_outline,
                    draw_hammer_outline,
                    place_on_click,
                    use_hammer,
                )
                    .chain()
                    .run_if(
                        in_state(AppState::InGame)
                            .and_then(resource_exists::<StageScene>)
                            .and_then(|shop: Res<ShopState>| !shop.open)
                            .and_then(|panel: Res<ChallengePanel>| !panel.open)
                            .and_then(|plan: Res<BlueprintMode>| !plan.panel_open)
                            .and_then(|session: Res<SessionUi>| !session.blocks_world()),
                    ),
            );
    }
}

/// Clears placement state so nothing from a previous stage can leak into the
/// next one. (Placed and preview entities despawn with `DespawnOnExit`.)
pub fn reset_placement(
    mut world: ResMut<PlacedWorld>,
    mut target: ResMut<PlacementTarget>,
    mut feedback: ResMut<PlacementFeedback>,
) {
    world.clear();
    *target = PlacementTarget::default();
    *feedback = PlacementFeedback::default();
}

/// Reads the active stage's placement permissions and limits. Replaces any
/// rules left over from a previous stage.
fn init_placement_rules(loaded: Res<LoadedStage>, mut rules: ResMut<StagePlacementRules>) {
    *rules = match loaded.0.as_ref() {
        Some(package) => {
            let parsed = StagePlacementRules::from_construction(&package.stage.construction);
            if let Some(err) = &parsed.error {
                error!(
                    "Invalid block rules in stage '{}': {err}",
                    package.meta.stage_id
                );
            }
            parsed
        }
        None => StagePlacementRules::default(),
    };
}

fn reset_rotation(mut rotation: ResMut<PlacementRotation>) {
    *rotation = PlacementRotation::default();
}

/// R: one quarter turn for the selected block, if it is rotatable. Changes the
/// requested orientation only; nothing is placed, consumed or reserved.
fn rotate_placement(
    keys: Res<ButtonInput<KeyCode>>,
    selected: Res<SelectedBlock>,
    registry: Res<BlockRegistry>,
    mut rotation: ResMut<PlacementRotation>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    // No block selected (nothing, or a tool): R does nothing.
    let Some(def) = selected.0.as_ref().and_then(|id| registry.get(id)) else {
        return;
    };
    let current = def.grid.rotations.effective(rotation.0);
    let next = def.grid.rotations.step(current);
    if next != rotation.0 {
        rotation.0 = next;
    }
}

fn clear_placement_rules(mut rules: ResMut<StagePlacementRules>) {
    *rules = StagePlacementRules::default();
}

#[allow(clippy::too_many_arguments)]
fn update_target(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    ui: Query<&Interaction>,
    selected: Res<SelectedBlock>,
    registry: Res<BlockRegistry>,
    rules: Res<StagePlacementRules>,
    inventory: Res<Inventory>,
    scene: Res<StageScene>,
    world: Res<PlacedWorld>,
    requested: Res<PlacementRotation>,
    mut target: ResMut<PlacementTarget>,
) {
    let grid = scene.grid_spec();
    let rotation = selected
        .0
        .as_ref()
        .and_then(|id| registry.get(id))
        .map_or(GridRotation::Deg0, |def| {
            def.grid.rotations.effective(requested.0)
        });
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let cell = if over_ui {
        None
    } else {
        window
            .cursor_position()
            .and_then(|pos| camera.0.viewport_to_world(camera.1, pos).ok())
            .and_then(|ray| {
                super::targeting::ray_cell_target(
                    ray.origin,
                    *ray.direction,
                    &grid,
                    world.occupied_cells(),
                )
            })
    };
    let verdict = cell.map(|cell| {
        evaluate_commit(
            &registry,
            &grid,
            &world,
            &rules,
            &inventory,
            selected.0.as_ref(),
            cell,
            rotation,
        )
        .and_then(|def| check_placement_support(&world, &grid, def, cell, rotation))
    });
    let new = PlacementTarget {
        cell,
        rotation,
        verdict,
    };
    if *target != new {
        *target = new;
    }
}

/// Transform and visibility of the ghost for the current target.
fn preview_pose(
    block: &BlockId,
    target: &PlacementTarget,
    registry: &BlockRegistry,
    grid: &GridSpec,
) -> (Transform, Visibility) {
    let valid = matches!(target.verdict, Some(Ok(())));
    match (target.cell, registry.get(block)) {
        (Some(cell), Some(def)) if valid => {
            let placed = PlacedBlock {
                block: block.clone(),
                cell,
                rotation: target.rotation,
            };
            (placed.transform(def, grid), Visibility::Inherited)
        }
        _ => (Transform::default(), Visibility::Hidden),
    }
}

/// Keeps exactly one ghost for the selected block: respawned only when the
/// selection changes, otherwise just moved and shown/hidden.
fn sync_preview(
    mut commands: Commands,
    selected: Res<SelectedBlock>,
    target: Res<PlacementTarget>,
    assets: Res<BlockAssets>,
    registry: Res<BlockRegistry>,
    scene: Res<StageScene>,
    mut previews: Query<(Entity, &PlacementPreview, &mut Transform, &mut Visibility)>,
) {
    if !(selected.is_changed() || target.is_changed() || assets.is_changed()) {
        return;
    }
    let grid = scene.grid_spec();
    let wanted = selected
        .0
        .as_ref()
        .filter(|id| assets.ready_handle(id).is_ok());

    let mut have_current = false;
    for (entity, preview, mut transform, mut visibility) in &mut previews {
        if Some(&preview.block) == wanted {
            let (t, v) = preview_pose(&preview.block, &target, &registry, &grid);
            if *transform != t {
                *transform = t;
            }
            if *visibility != v {
                *visibility = v;
            }
            have_current = true;
        } else {
            commands.entity(entity).despawn();
        }
    }

    if have_current {
        return;
    }
    if let Some(id) = wanted
        && let Ok(handle) = assets.ready_handle(id)
    {
        let (transform, visibility) = preview_pose(id, &target, &registry, &grid);
        commands.spawn((
            Name::new("Placement Preview"),
            PlacementPreview { block: id.clone() },
            WorldAssetRoot(handle.clone()),
            transform,
            visibility,
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn draw_target_outline(
    mut gizmos: Gizmos,
    selected: Res<SelectedBlock>,
    target: Res<PlacementTarget>,
    registry: Res<BlockRegistry>,
    scene: Res<StageScene>,
) {
    let (Some(id), Some(cell)) = (selected.0.as_ref(), target.cell) else {
        return;
    };
    let grid = scene.grid_spec();
    let footprint = registry
        .get(id)
        .map_or(UVec3::ONE, |d| d.dimensions.footprint_cells);
    let (min, size) = footprint_box(cell, footprint, target.rotation);
    let lo = grid.cell_min_corner(min);
    let hi = lo + size.as_vec3() * grid.cell_size;
    let color = if matches!(target.verdict, Some(Ok(()))) {
        Color::srgb(0.3, 1.0, 0.4)
    } else {
        Color::srgb(1.0, 0.3, 0.3)
    };
    draw_box(&mut gizmos, lo, hi, color);
}

/// Left click: re-validate (stage rules, limit, inventory, geometry), spawn,
/// then record the placement and consume one item together. The only fallible step after
/// validation is the spawn, which happens before the record is written, and
/// the record cannot fail after the validation just performed, so the logical
/// grid and the rendered entity are never left out of step.
#[allow(clippy::too_many_arguments)]
fn place_on_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    target: Res<PlacementTarget>,
    selected: Res<SelectedBlock>,
    item: Res<SelectedItem>,
    tools: Res<ToolRegistry>,
    registry: Res<BlockRegistry>,
    assets: Res<BlockAssets>,
    rules: Res<StagePlacementRules>,
    scene: Res<StageScene>,
    mut inventory: ResMut<Inventory>,
    mut world: ResMut<PlacedWorld>,
    mut feedback: ResMut<PlacementFeedback>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    // Tools never place blocks; the hammer has its own system.
    if click_action(item.0.as_ref(), &tools) != ClickAction::PlaceBlock {
        return;
    }
    // Missed the build surface, or the click belongs to the HUD.
    let Some(cell) = target.cell else {
        return;
    };
    let grid = scene.grid_spec();
    let rotation = target.rotation;

    let def = match evaluate_commit(
        &registry,
        &grid,
        &world,
        &rules,
        &inventory,
        selected.0.as_ref(),
        cell,
        rotation,
    ) {
        Ok(def) => def,
        Err(err) => {
            feedback.0 = Some(err.to_string());
            return;
        }
    };
    if let Err(err) = check_placement_support(&world, &grid, def, cell, rotation) {
        feedback.0 = Some(err.to_string());
        return;
    }
    let Some(id) = selected.0.clone() else {
        return;
    };
    let entity = match spawn_block(
        &mut commands,
        &registry,
        &assets,
        &grid,
        id.clone(),
        cell,
        rotation,
    ) {
        Ok(entity) => entity,
        Err(err) => {
            warn!("Placement failed: {err}");
            feedback.0 = Some(format!("Could not place {}: {err}", def.display_name));
            return;
        }
    };
    let placed = PlacedBlock {
        block: id,
        cell,
        rotation,
    };
    if let Err(err) = commit_placement(&mut world, &mut inventory, &placed, def, entity) {
        commands.entity(entity).despawn();
        feedback.0 = Some(err.to_string());
        return;
    }
    commands
        .entity(entity)
        .insert(DespawnOnExit(AppState::InGame));
    feedback.0 = Some(format!(
        "Placed {} at ({}, {}, {})",
        def.display_name, cell.0.x, cell.0.y, cell.0.z
    ));
}

fn is_hammer(item: &SelectedItem, tools: &ToolRegistry) -> bool {
    click_action(item.0.as_ref(), tools) == ClickAction::UseHammer
        && matches!(item.0, Some(ItemRef::Tool(_)))
}

/// Picks the nearest placed block under the cursor while the hammer is
/// selected; otherwise (or over the HUD, or on a miss) there is no target.
#[allow(clippy::too_many_arguments)]
fn update_hammer_target(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    ui: Query<&Interaction>,
    item: Res<SelectedItem>,
    tools: Res<ToolRegistry>,
    scene: Res<StageScene>,
    world: Res<PlacedWorld>,
    mut target: ResMut<HammerTarget>,
) {
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let entity = if !is_hammer(&item, &tools) || over_ui {
        None
    } else {
        let grid = scene.grid_spec();
        window
            .cursor_position()
            .and_then(|pos| camera.0.viewport_to_world(camera.1, pos).ok())
            .and_then(|ray| {
                ray_pick_occupied(ray.origin, *ray.direction, &grid, world.occupied_cells())
            })
            .and_then(|cell| world.occupant(cell))
    };
    if target.0 != entity {
        target.0 = entity;
    }
}

/// Outlines the block the hammer would remove, using its recorded cells.
fn draw_hammer_outline(
    mut gizmos: Gizmos,
    target: Res<HammerTarget>,
    world: Res<PlacedWorld>,
    registry: Res<BlockRegistry>,
    scene: Res<StageScene>,
) {
    let Some(record) = target.0.and_then(|e| world.record(e)) else {
        return;
    };
    let grid = scene.grid_spec();
    let footprint = registry
        .get(&record.block)
        .map_or(UVec3::ONE, |d| d.dimensions.footprint_cells);
    let (min, size) = footprint_box(record.cell, footprint, record.rotation);
    let lo = grid.cell_min_corner(min);
    let hi = lo + size.as_vec3() * grid.cell_size;
    draw_box(&mut gizmos, lo, hi, Color::srgb(1.0, 0.45, 0.1));
}

/// Left click with the hammer: remove the targeted placed block, release its
/// cells, refund one unit. Entity validity is checked against the logical
/// world, so stale targets, previews and other entities are ignored.
#[allow(clippy::too_many_arguments)]
fn use_hammer(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    item: Res<SelectedItem>,
    tools: Res<ToolRegistry>,
    target: Res<HammerTarget>,
    registry: Res<BlockRegistry>,
    mut world: ResMut<PlacedWorld>,
    mut inventory: ResMut<Inventory>,
    mut feedback: ResMut<PlacementFeedback>,
) {
    if !mouse.just_pressed(MouseButton::Left) || !is_hammer(&item, &tools) {
        return;
    }
    // A miss (or a click on the HUD) is a silent no-op.
    let Some(entity) = target.0 else {
        return;
    };
    match remove_block(&mut world, &mut inventory, entity) {
        Ok(record) => {
            commands.entity(entity).try_despawn();
            let name = registry
                .get(&record.block)
                .map_or(record.block.as_str(), |def| def.display_name.as_str());
            feedback.0 = Some(format!("Removed {name}; 1 returned to your inventory."));
        }
        Err(err) => feedback.0 = Some(err.to_string()),
    }
}
