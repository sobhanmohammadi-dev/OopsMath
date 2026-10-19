//! Builds the 3D environment of the stage that was selected and loaded.
//!
//! On entering [`AppState::InGame`] this reads the [`LoadedStage`] and spawns a
//! 3D camera (orbit/pan/zoom, same controls as the block preview), lights, a
//! ground plane and sky colour from `world.environment.preset`, a floor grid
//! over `world.bounds`, and an outline for every `construction.targets` region.
//!
//! Everything is despawned on exit. The gameplay HUD is untouched and keeps
//! rendering on top via the new camera. Voxel terrain (`WRLD`) and block
//! placement by the player are not rendered yet.

use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use serde_json::Value;

use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blocks::camera::{OrbitCamera, orbit_camera_system};
use crate::blocks::debug_scene::{draw_box, draw_grid};
use crate::blocks::grid::{GridBounds, GridSpec};
use crate::stage::package::StageDefinition;

/// Used when the stage declares no bounds (matches the stage template).
const DEFAULT_SIZE: UVec3 = UVec3::new(16, 8, 16);

/// Geometry of the stage world, kept while in game for gizmo drawing.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct StageScene {
    pub origin: Vec3,
    pub size: UVec3,
    /// Target regions as world-space boxes (`min..max`, max cell inclusive).
    pub targets: Vec<(Vec3, Vec3)>,
}

impl StageScene {
    /// The placement grid of this stage: 1 m cells from `origin`, bounded by
    /// the stage's world size. Cell (0,0,0) is the minimum corner; Y is up.
    pub fn grid_spec(&self) -> GridSpec {
        GridSpec {
            origin: self.origin,
            cell_size: Vec3::ONE,
            bounds: Some(GridBounds {
                min: IVec3::ZERO,
                size: self.size,
            }),
        }
    }
}

pub struct StageScenePlugin;

impl Plugin for StageScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), build_stage_scene)
            .add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
                commands.remove_resource::<StageScene>();
            })
            .add_systems(
                Update,
                (orbit_camera_system, draw_stage_gizmos)
                    .run_if(in_state(AppState::InGame).and_then(resource_exists::<StageScene>)),
            );
    }
}

fn vec3_of(value: Option<&Value>) -> Option<[f64; 3]> {
    let a = value?.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
    ])
}

/// Extracts the world geometry from a stage definition; tolerant of missing
/// optional data.
pub fn stage_scene_from(def: &StageDefinition) -> StageScene {
    let bounds = def.world.get("bounds");
    let origin = vec3_of(bounds.and_then(|b| b.get("origin")))
        .map(|o| Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32))
        .unwrap_or(Vec3::ZERO);
    let size = vec3_of(bounds.and_then(|b| b.get("size")))
        .filter(|s| s.iter().all(|v| *v >= 1.0 && *v <= 256.0))
        .map(|s| UVec3::new(s[0] as u32, s[1] as u32, s[2] as u32))
        .unwrap_or(DEFAULT_SIZE);

    let targets = def
        .construction
        .get("targets")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    let region = t.get("region")?;
                    let min = vec3_of(region.get("min"))?;
                    let max = vec3_of(region.get("max"))?;
                    let lo = origin + Vec3::new(min[0] as f32, min[1] as f32, min[2] as f32);
                    let hi = origin
                        + Vec3::new(
                            max[0] as f32 + 1.0,
                            max[1] as f32 + 1.0,
                            max[2] as f32 + 1.0,
                        );
                    Some((lo, hi))
                })
                .collect()
        })
        .unwrap_or_default();

    StageScene {
        origin,
        size,
        targets,
    }
}

/// (sky colour, ground colour) for an environment preset.
fn palette(preset: &str) -> (Color, Color) {
    match preset {
        "meadow" => (Color::srgb(0.53, 0.74, 0.92), Color::srgb(0.30, 0.52, 0.25)),
        _ => (Color::srgb(0.10, 0.11, 0.13), Color::srgb(0.18, 0.20, 0.22)),
    }
}

fn build_stage_scene(
    mut commands: Commands,
    loaded: Res<LoadedStage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(package) = loaded.0.as_ref() else {
        warn!("Entered the game without a loaded stage; no environment built");
        return;
    };
    let scene = stage_scene_from(&package.stage);
    let preset = package
        .stage
        .world
        .get("environment")
        .and_then(|e| e.get("preset"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let (sky, ground) = palette(preset);
    info!(
        "Building stage environment '{}' ({}x{}x{} cells, preset '{preset}')",
        package.meta.stage_id, scene.size.x, scene.size.y, scene.size.z
    );

    let extent = scene.size.as_vec3();
    let center = scene.origin + Vec3::new(extent.x * 0.5, 0.0, extent.z * 0.5);

    let mut orbit = OrbitCamera::new(center + Vec3::Y, extent.x.max(extent.z) * 1.1, 0.6, 0.6);
    orbit.min_distance = 2.0;
    orbit.max_distance = extent.max_element() * 6.0;
    orbit.clamp();
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(sky),
            ..default()
        },
        AmbientLight {
            brightness: 400.0,
            ..default()
        },
        IsDefaultUiCamera,
        orbit.transform(),
        orbit,
        DespawnOnExit(AppState::InGame),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(center + Vec3::new(10.0, 20.0, 12.0))
            .looking_at(center, Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(extent.x, extent.z))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: ground,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_translation(center - Vec3::Y * 0.003),
        DespawnOnExit(AppState::InGame),
    ));

    commands.insert_resource(scene);
}

fn draw_stage_gizmos(mut gizmos: Gizmos, scene: Res<StageScene>) {
    draw_grid(&mut gizmos, scene.origin, scene.size, Vec3::ONE);
    for (lo, hi) in &scene.targets {
        draw_box(&mut gizmos, *lo, *hi, Color::srgb(0.2, 0.9, 0.9));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def(world: Value, construction: Value) -> StageDefinition {
        let mut d: StageDefinition = serde_json::from_value(json!({
            "schema_version": 1, "stage": {}, "learning": {}, "world": {},
            "construction": {}, "objectives": {}, "rewards": {}
        }))
        .unwrap();
        d.world = world;
        d.construction = construction;
        d
    }

    #[test]
    fn reads_bounds_and_target_regions() {
        let s = stage_scene_from(&def(
            json!({"bounds": {"origin": [0,0,0], "size": [16,8,16]}}),
            json!({"targets": [{"id": "wall_a", "region": {"min": [0,0,0], "max": [3,2,0]}}]}),
        ));
        assert_eq!(s.size, UVec3::new(16, 8, 16));
        assert_eq!(s.targets, vec![(Vec3::ZERO, Vec3::new(4.0, 3.0, 1.0))]);
    }

    #[test]
    fn missing_or_invalid_bounds_fall_back() {
        let s = stage_scene_from(&def(json!({}), json!({})));
        assert_eq!(s.size, DEFAULT_SIZE);
        assert!(s.targets.is_empty());
        let s = stage_scene_from(&def(json!({"bounds": {"size": [0,8,16]}}), json!({})));
        assert_eq!(s.size, DEFAULT_SIZE);
    }
}
