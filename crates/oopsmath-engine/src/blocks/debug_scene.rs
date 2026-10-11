//! Opt-in development scene that proves the Blocks module end to end.
//!
//! Activation: launch with the environment variable `OOPSMATH_BLOCK_PREVIEW=1`.
//! On entering the main menu the app then switches to
//! [`AppState::BlockPreview`]. Without the variable nothing here runs, and
//! nothing is spawned outside that state (all entities despawn on exit).
//!
//! The scene adds its own 3D camera (render order 1, above the existing
//! `Camera2d`), lights and a ground plane; it never touches the gameplay HUD.

use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use super::assets::{BlockAssetState, BlockAssets};
use super::camera::{OrbitCamera, orbit_camera_system};
use super::definition::BlockId;
use super::grid::{GridBounds, GridCoord, GridRotation, GridSpec, occupied_cells};
use super::models::wood_log_id;
use super::placement::{PlacedBlock, validate_placement};
use super::registry::BlockRegistry;
use super::rendering::spawn_block;
use crate::app::states::AppState;
use crate::ui::theme::UiTheme;

pub const PREVIEW_ENV: &str = "OOPSMATH_BLOCK_PREVIEW";

/// Preview grid: 16 x 8 x 16 cells, matching the world bounds of stage 001.
const GRID_SIZE: UVec3 = UVec3::new(16, 8, 16);

/// The grid used by the preview (kept apart from any future stage grid).
#[derive(Resource, Clone, Copy, Debug)]
pub struct PreviewGrid(pub GridSpec);

impl Default for PreviewGrid {
    fn default() -> Self {
        Self(GridSpec {
            bounds: Some(GridBounds {
                min: IVec3::ZERO,
                size: GRID_SIZE,
            }),
            ..GridSpec::default()
        })
    }
}

/// Which registered block is previewed, and where.
#[derive(Resource, Clone, Debug)]
pub struct BlockPreview {
    pub block: BlockId,
    pub cell: GridCoord,
    pub rotation: GridRotation,
    /// Spawn failure shown in the status line (spawning is not retried).
    pub spawn_error: Option<String>,
}

impl Default for BlockPreview {
    fn default() -> Self {
        Self {
            block: wood_log_id(),
            cell: GridCoord::new(8, 0, 8),
            rotation: GridRotation::Deg0,
            spawn_error: None,
        }
    }
}

#[derive(Component)]
struct PreviewBlock;

#[derive(Component)]
struct PreviewStatusText;

pub struct BlockPreviewPlugin;

impl Plugin for BlockPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewGrid>()
            .init_resource::<BlockPreview>()
            .add_systems(OnEnter(AppState::MainMenu), request_preview_from_env)
            .add_systems(OnEnter(AppState::BlockPreview), setup_preview)
            .add_systems(OnExit(AppState::BlockPreview), reset_preview)
            .add_systems(
                Update,
                (
                    preview_input,
                    spawn_preview_block,
                    orbit_camera_system,
                    draw_preview_grid,
                    update_status_text,
                )
                    .chain()
                    .run_if(in_state(AppState::BlockPreview)),
            );
    }
}

fn request_preview_from_env(mut next: ResMut<NextState<AppState>>) {
    if std::env::var_os(PREVIEW_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        info!("{PREVIEW_ENV} is set: entering the block preview scene");
        next.set(AppState::BlockPreview);
    }
}

fn reset_preview(mut preview: ResMut<BlockPreview>) {
    preview.spawn_error = None;
}

fn setup_preview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    grid: Res<PreviewGrid>,
    preview: Res<BlockPreview>,
    theme: Res<UiTheme>,
) {
    let scope = DespawnOnExit(AppState::BlockPreview);
    let target = grid.0.cell_center(preview.cell);

    let orbit = OrbitCamera::new(target, 5.0, 0.6, 0.5);
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.10, 0.11, 0.13)),
            ..default()
        },
        // Scoped to this camera so the rest of the game keeps its lighting.
        AmbientLight {
            brightness: 250.0,
            ..default()
        },
        IsDefaultUiCamera,
        orbit.transform(),
        orbit,
        DespawnOnExit(AppState::BlockPreview),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(target, Vec3::Y),
        DespawnOnExit(AppState::BlockPreview),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 2_500.0,
            ..default()
        },
        Transform::from_xyz(-5.0, 3.0, -4.0).looking_at(target, Vec3::Y),
        DespawnOnExit(AppState::BlockPreview),
    ));

    // Ground plane just below the grid so the log casts a visible shadow.
    let extent = GRID_SIZE.as_vec3() * grid.0.cell_size;
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(extent.x, extent.z))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.20, 0.22),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_translation(grid.0.origin + Vec3::new(extent.x * 0.5, -0.003, extent.z * 0.5)),
        DespawnOnExit(AppState::BlockPreview),
    ));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(16),
                right: px(16),
                padding: UiRect::all(px(10)),
                ..default()
            },
            BackgroundColor(theme.panel),
            scope,
        ))
        .with_child((
            Text::new(
                "RMB drag: orbit   MMB drag: pan   Wheel: zoom\nArrows: move cell   R: rotate",
            ),
            TextFont {
                font_size: FontSize::Px(16.0),
                ..default()
            },
            TextColor(theme.secondary_text),
        ));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(16),
                left: px(16),
                padding: UiRect::all(px(10)),
                ..default()
            },
            BackgroundColor(theme.panel),
            DespawnOnExit(AppState::BlockPreview),
        ))
        .with_child((
            PreviewStatusText,
            Text::new("Block preview"),
            TextFont {
                font_size: FontSize::Px(18.0),
                ..default()
            },
            TextColor(theme.text),
        ));
}

/// Moves / rotates the preview block on the grid. Invalid targets (outside the
/// bounds, disallowed rotation) are ignored.
fn preview_input(
    keys: Res<ButtonInput<KeyCode>>,
    registry: Res<BlockRegistry>,
    grid: Res<PreviewGrid>,
    mut preview: ResMut<BlockPreview>,
    mut blocks: Query<(&mut PlacedBlock, &mut Transform), With<PreviewBlock>>,
) {
    let mut cell = preview.cell;
    let mut rotation = preview.rotation;
    if keys.just_pressed(KeyCode::ArrowLeft) {
        cell.0.x -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        cell.0.x += 1;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        cell.0.z -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        cell.0.z += 1;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        rotation = rotation.next();
    }
    if cell == preview.cell && rotation == preview.rotation {
        return;
    }

    let def = match validate_placement(&registry, &grid.0, &preview.block, cell, rotation) {
        Ok(def) => def,
        Err(err) => {
            debug!("Preview move ignored: {err}");
            return;
        }
    };
    preview.cell = cell;
    preview.rotation = rotation;
    for (mut placed, mut transform) in &mut blocks {
        placed.cell = cell;
        placed.rotation = rotation;
        *transform = placed.transform(def, &grid.0);
    }
}

/// Spawns the preview block once its model is ready; failures are logged and
/// surfaced in the status line, never panicked on.
fn spawn_preview_block(
    mut commands: Commands,
    registry: Res<BlockRegistry>,
    assets: Res<BlockAssets>,
    grid: Res<PreviewGrid>,
    mut preview: ResMut<BlockPreview>,
    existing: Query<(), With<PreviewBlock>>,
) {
    if !existing.is_empty() || preview.spawn_error.is_some() {
        return;
    }
    if assets.state(&preview.block) != Some(&BlockAssetState::Ready) {
        return; // still loading, or failed (reported by the status line)
    }
    match spawn_block(
        &mut commands,
        &registry,
        &assets,
        &grid.0,
        preview.block.clone(),
        preview.cell,
        preview.rotation,
    ) {
        Ok(entity) => {
            commands
                .entity(entity)
                .insert((PreviewBlock, DespawnOnExit(AppState::BlockPreview)));
        }
        Err(err) => {
            error!("Could not spawn preview block: {err}");
            preview.spawn_error = Some(err.to_string());
        }
    }
}

/// Grid lines on the floor plane: one gizmo line per grid line, no per-cell
/// entities. Every 4th line is brighter.
pub fn draw_grid(gizmos: &mut Gizmos, origin: Vec3, size: UVec3, cell: Vec3) {
    let extent = size.as_vec3() * cell;
    let minor = Color::srgba(1.0, 1.0, 1.0, 0.10);
    let major = Color::srgba(1.0, 1.0, 1.0, 0.28);
    for x in 0..=size.x {
        let color = if x % 4 == 0 { major } else { minor };
        let px = origin.x + x as f32 * cell.x;
        gizmos.line(
            Vec3::new(px, origin.y, origin.z),
            Vec3::new(px, origin.y, origin.z + extent.z),
            color,
        );
    }
    for z in 0..=size.z {
        let color = if z % 4 == 0 { major } else { minor };
        let pz = origin.z + z as f32 * cell.z;
        gizmos.line(
            Vec3::new(origin.x, origin.y, pz),
            Vec3::new(origin.x + extent.x, origin.y, pz),
            color,
        );
    }
}

/// Wireframe of the axis-aligned box `lo..hi`.
pub fn draw_box(gizmos: &mut Gizmos, lo: Vec3, hi: Vec3, color: Color) {
    let c = |x: bool, y: bool, z: bool| {
        Vec3::new(
            if x { hi.x } else { lo.x },
            if y { hi.y } else { lo.y },
            if z { hi.z } else { lo.z },
        )
    };
    for (a, b) in [
        ((false, false, false), (true, false, false)),
        ((false, true, false), (true, true, false)),
        ((false, false, true), (true, false, true)),
        ((false, true, true), (true, true, true)),
        ((false, false, false), (false, true, false)),
        ((true, false, false), (true, true, false)),
        ((false, false, true), (false, true, true)),
        ((true, false, true), (true, true, true)),
        ((false, false, false), (false, false, true)),
        ((true, false, false), (true, false, true)),
        ((false, true, false), (false, true, true)),
        ((true, true, false), (true, true, true)),
    ] {
        gizmos.line(c(a.0, a.1, a.2), c(b.0, b.1, b.2), color);
    }
}

/// Grid, origin axes (X red, Y green, Z blue) and the selected footprint.
fn draw_preview_grid(
    mut gizmos: Gizmos,
    grid: Res<PreviewGrid>,
    preview: Res<BlockPreview>,
    registry: Res<BlockRegistry>,
) {
    let spec = &grid.0;
    draw_grid(&mut gizmos, spec.origin, GRID_SIZE, spec.cell_size);

    let origin = spec.origin;
    gizmos.line(origin, origin + Vec3::X * 2.0, Color::srgb(0.95, 0.25, 0.25));
    gizmos.line(origin, origin + Vec3::Y * 2.0, Color::srgb(0.25, 0.95, 0.35));
    gizmos.line(origin, origin + Vec3::Z * 2.0, Color::srgb(0.3, 0.5, 1.0));

    let footprint = registry
        .get(&preview.block)
        .map(|d| preview.rotation.rotate_footprint(d.dimensions.footprint_cells))
        .unwrap_or(UVec3::ONE);
    let lo = spec.cell_min_corner(preview.cell);
    let hi = lo + footprint.as_vec3() * spec.cell_size;
    draw_box(&mut gizmos, lo, hi, Color::srgb(1.0, 0.85, 0.2));
}

fn update_status_text(
    registry: Res<BlockRegistry>,
    assets: Res<BlockAssets>,
    preview: Res<BlockPreview>,
    mut text: Single<&mut Text, With<PreviewStatusText>>,
) {
    if !(assets.is_changed() || preview.is_changed()) {
        return;
    }
    let model = match assets.state(&preview.block) {
        None => "not requested".to_string(),
        Some(BlockAssetState::Loading) => "loading...".to_string(),
        Some(BlockAssetState::Ready) => "ready".to_string(),
        Some(BlockAssetState::Failed(reason)) => format!("FAILED: {reason}"),
    };
    let path = assets.path(&preview.block).unwrap_or("-");
    let (name, price, cells) = match registry.get(&preview.block) {
        Some(def) => (
            def.display_name.as_str(),
            def.economy.default_unit_price.to_string(),
            occupied_cells(
                preview.cell,
                preview.rotation.rotate_footprint(def.dimensions.footprint_cells),
            )
            .count(),
        ),
        None => ("<unregistered>", "-".to_string(), 0),
    };
    let mut s = format!(
        "{name} ({})\nModel {path}: {model}\nCell {} {} {}  rotation {} deg  covers {cells} cell(s)\nDefault price {price} coins",
        preview.block,
        preview.cell.0.x,
        preview.cell.0.y,
        preview.cell.0.z,
        preview.rotation.degrees(),
    );
    if let Some(err) = &preview.spawn_error {
        s.push_str(&format!("\nSpawn error: {err}"));
    }
    text.0 = s;
}
