//! Block material and placement-ghost look.
//!
//! `assets/log.glb` carries no base colour, texture or vertex colour (its two
//! materials only set roughness), so under the glTF defaults it renders plain
//! white. Rather than edit the binary asset, three shared
//! [`StandardMaterial`]s are created once ([`BlockLook`]) and assigned to the
//! mesh primitives of spawned block scenes:
//!
//! * placed blocks (root has [`PlacedBlock`]) -> one shared matte wood material;
//! * the placement preview (root has [`GhostVisual`]) -> a translucent green or
//!   red ghost material, chosen from the current [`PlacementTarget`] verdict,
//!   and the ghost does not cast shadows.
//!
//! Only material *handles* on mesh entities are swapped. No material asset is
//! ever mutated, so recolouring the ghost cannot touch a placed block, and the
//! loaded glTF materials stay untouched (other users of the model, such as the
//! development preview scene, keep the original look).
//!
//! Nothing here reads or writes the authoritative grid, inventory, wallet,
//! objectives or stability state: the systems only see mesh entities, their
//! parents, the shared materials and the (read-only) placement verdict.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::interaction::PlacementTarget;
use super::placement::PlacedBlock;

/// Marks the root of the placement preview.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct GhostVisual;

/// Marks a mesh primitive that belongs to the ghost (so it can be re-tinted).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct GhostPart;

/// Shared materials for block visuals.
#[derive(Resource, Clone, Debug)]
pub struct BlockLook {
    pub wood: Handle<StandardMaterial>,
    pub ghost_valid: Handle<StandardMaterial>,
    pub ghost_invalid: Handle<StandardMaterial>,
}

impl BlockLook {
    /// Creates the three materials in `materials`.
    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let wood = materials.add(StandardMaterial {
            base_color: Color::srgb(0.60, 0.40, 0.22),
            perceptual_roughness: 0.85,
            metallic: 0.0,
            reflectance: 0.15,
            // The imported glTF materials were double sided; keep that.
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let ghost_valid = materials.add(ghost_material(Color::srgba(0.30, 0.95, 0.40, 0.45)));
        let ghost_invalid = materials.add(ghost_material(Color::srgba(1.00, 0.28, 0.28, 0.45)));
        Self {
            wood,
            ghost_valid,
            ghost_invalid,
        }
    }

    /// The ghost material for a valid or an invalid target.
    pub fn ghost(&self, valid: bool) -> &Handle<StandardMaterial> {
        if valid {
            &self.ghost_valid
        } else {
            &self.ghost_invalid
        }
    }
}

fn ghost_material(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    }
}

/// Whether the current placement target is valid. No target, or a rejected
/// one, counts as invalid (the ghost is hidden while there is no target).
pub fn target_is_valid(target: &PlacementTarget) -> bool {
    matches!(target.verdict, Some(Ok(_)))
}

/// Creates [`BlockLook`] when materials exist (a headless app has none).
fn setup_block_look(mut commands: Commands, materials: Option<ResMut<Assets<StandardMaterial>>>) {
    if let Some(mut materials) = materials {
        commands.insert_resource(BlockLook::new(&mut materials));
    }
}

/// The role of the block scene a mesh primitive belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Placed,
    Ghost,
}

fn role_of(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    placed: &Query<(), With<PlacedBlock>>,
    ghosts: &Query<(), With<GhostVisual>>,
) -> Option<Role> {
    // Scene hierarchies are shallow; the bound only protects against cycles.
    for _ in 0..32 {
        let parent = parents.get(entity).ok()?.parent();
        if ghosts.contains(parent) {
            return Some(Role::Ghost);
        }
        if placed.contains(parent) {
            return Some(Role::Placed);
        }
        entity = parent;
    }
    None
}

/// Assigns the shared materials to newly spawned block mesh primitives.
///
/// Runs in `PostUpdate`, after the scene spawner (`SpawnScene`) and before
/// rendering extraction, so a block is never drawn with the white default.
#[allow(clippy::type_complexity)]
pub fn apply_block_look(
    look: Option<Res<BlockLook>>,
    target: Res<PlacementTarget>,
    mut fresh: Query<
        (Entity, &mut MeshMaterial3d<StandardMaterial>),
        Added<MeshMaterial3d<StandardMaterial>>,
    >,
    parents: Query<&ChildOf>,
    placed: Query<(), With<PlacedBlock>>,
    ghosts: Query<(), With<GhostVisual>>,
    mut commands: Commands,
) {
    let Some(look) = look else {
        return;
    };
    let valid = target_is_valid(&target);
    for (entity, mut material) in &mut fresh {
        match role_of(entity, &parents, &placed, &ghosts) {
            Some(Role::Placed) => material.0 = look.wood.clone(),
            Some(Role::Ghost) => {
                material.0 = look.ghost(valid).clone();
                commands.entity(entity).insert((GhostPart, NotShadowCaster));
            }
            None => {}
        }
    }
}

/// Re-tints the existing ghost when the placement verdict changes (no respawn).
pub fn retint_ghost(
    look: Option<Res<BlockLook>>,
    target: Res<PlacementTarget>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>, With<GhostPart>>,
) {
    let Some(look) = look else {
        return;
    };
    if !target.is_changed() && !look.is_changed() {
        return;
    }
    let wanted = look.ghost(target_is_valid(&target));
    for mut material in &mut parts {
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

pub struct BlockLookPlugin;

impl Plugin for BlockLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_block_look)
            .add_systems(PostUpdate, (apply_block_look, retint_ghost).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::definition::BlockId;
    use crate::blocks::grid::{GridCoord, GridRotation};
    use bevy::ecs::system::RunSystemOnce;

    fn look() -> (World, BlockLook) {
        let mut world = World::new();
        let mut assets = Assets::<StandardMaterial>::default();
        let look = BlockLook::new(&mut assets);
        world.insert_resource(assets);
        world.insert_resource(look.clone());
        world.init_resource::<PlacementTarget>();
        (world, look)
    }

    fn placed_root(world: &mut World) -> Entity {
        world
            .spawn(PlacedBlock {
                block: BlockId::new("block.wood_log"),
                cell: GridCoord::new(0, 0, 0),
                rotation: GridRotation::Deg0,
            })
            .id()
    }

    /// A mesh primitive nested like a glTF scene: root -> node -> primitive.
    fn primitive(world: &mut World, root: Entity) -> Entity {
        let node = world.spawn(ChildOf(root)).id();
        world
            .spawn((ChildOf(node), MeshMaterial3d::<StandardMaterial>(default())))
            .id()
    }

    fn handle(world: &World, e: Entity) -> Handle<StandardMaterial> {
        world
            .get::<MeshMaterial3d<StandardMaterial>>(e)
            .unwrap()
            .0
            .clone()
    }

    #[test]
    fn the_three_materials_are_distinct_and_configured() {
        let mut assets = Assets::<StandardMaterial>::default();
        let look = BlockLook::new(&mut assets);
        assert_ne!(look.wood, look.ghost_valid);
        assert_ne!(look.wood, look.ghost_invalid);
        assert_ne!(look.ghost_valid, look.ghost_invalid);
        let wood = assets.get(&look.wood).unwrap();
        assert_eq!(wood.alpha_mode, AlphaMode::Opaque);
        assert_ne!(wood.base_color, Color::WHITE, "a deliberate colour");
        assert!(wood.perceptual_roughness >= 0.7, "matte, not plastic");
        for h in [&look.ghost_valid, &look.ghost_invalid] {
            let m = assets.get(h).unwrap();
            assert_eq!(m.alpha_mode, AlphaMode::Blend);
            assert!(m.base_color.alpha() < 1.0);
        }
        // Green vs red.
        let g = assets.get(&look.ghost_valid).unwrap().base_color.to_srgba();
        let r = assets
            .get(&look.ghost_invalid)
            .unwrap()
            .base_color
            .to_srgba();
        assert!(g.green > g.red && r.red > r.green);
    }

    #[test]
    fn every_primitive_of_a_placed_block_gets_the_shared_wood() {
        let (mut world, look) = look();
        let root = placed_root(&mut world);
        let a = primitive(&mut world, root);
        let b = primitive(&mut world, root);
        world.run_system_once(apply_block_look).unwrap();
        assert_eq!(handle(&world, a), look.wood);
        assert_eq!(
            handle(&world, b),
            look.wood,
            "all primitives, one shared handle"
        );
        assert!(world.get::<GhostPart>(a).is_none());
        assert!(
            world.get::<NotShadowCaster>(a).is_none(),
            "placed blocks cast shadows"
        );
    }

    #[test]
    fn the_ghost_gets_its_own_translucent_material_without_shadows() {
        let (mut world, look) = look();
        let ghost = world.spawn(GhostVisual).id();
        let part = primitive(&mut world, ghost);
        world.run_system_once(apply_block_look).unwrap();
        // No verdict yet counts as invalid.
        assert_eq!(handle(&world, part), look.ghost_invalid);
        assert!(world.get::<GhostPart>(part).is_some());
        assert!(world.get::<NotShadowCaster>(part).is_some());
        assert_ne!(handle(&world, part), look.wood);
    }

    #[test]
    fn a_ghost_spawned_while_valid_starts_green() {
        let (mut world, look) = look();
        world.resource_mut::<PlacementTarget>().verdict = Some(Ok(()));
        let ghost = world.spawn(GhostVisual).id();
        let part = primitive(&mut world, ghost);
        world.run_system_once(apply_block_look).unwrap();
        assert_eq!(handle(&world, part), look.ghost_valid);
    }

    #[test]
    fn retinting_the_ghost_never_touches_placed_blocks() {
        let (mut world, look) = look();
        let placed = placed_root(&mut world);
        let wood_part = primitive(&mut world, placed);
        let ghost = world.spawn(GhostVisual).id();
        let ghost_part = primitive(&mut world, ghost);
        world.run_system_once(apply_block_look).unwrap();
        assert_eq!(handle(&world, ghost_part), look.ghost_invalid);

        world.resource_mut::<PlacementTarget>().verdict = Some(Ok(()));
        world.run_system_once(retint_ghost).unwrap();
        assert_eq!(handle(&world, ghost_part), look.ghost_valid);
        assert_eq!(
            handle(&world, wood_part),
            look.wood,
            "placed block unchanged"
        );

        world.resource_mut::<PlacementTarget>().verdict = None;
        world.run_system_once(retint_ghost).unwrap();
        assert_eq!(handle(&world, ghost_part), look.ghost_invalid);
        assert_eq!(handle(&world, wood_part), look.wood);
    }

    #[test]
    fn the_shared_materials_are_never_mutated_by_tinting() {
        let (mut world, look) = look();
        let before = world
            .resource::<Assets<StandardMaterial>>()
            .get(&look.wood)
            .unwrap()
            .base_color;
        let ghost = world.spawn(GhostVisual).id();
        primitive(&mut world, ghost);
        world.run_system_once(apply_block_look).unwrap();
        world.resource_mut::<PlacementTarget>().verdict = Some(Ok(()));
        world.run_system_once(retint_ghost).unwrap();
        let after = world
            .resource::<Assets<StandardMaterial>>()
            .get(&look.wood)
            .unwrap()
            .base_color;
        assert_eq!(before, after);
    }

    #[test]
    fn unrelated_meshes_keep_their_materials() {
        let (mut world, look) = look();
        let stray = world
            .spawn(MeshMaterial3d::<StandardMaterial>(default()))
            .id();
        let other_root = world.spawn_empty().id();
        let child = primitive(&mut world, other_root);
        world.run_system_once(apply_block_look).unwrap();
        for e in [stray, child] {
            let h = handle(&world, e);
            assert!(h != look.wood && h != look.ghost_valid && h != look.ghost_invalid);
        }
    }

    #[test]
    fn despawning_the_ghost_removes_its_parts() {
        let (mut world, _look) = look();
        let ghost = world.spawn(GhostVisual).id();
        let part = primitive(&mut world, ghost);
        world.run_system_once(apply_block_look).unwrap();
        assert!(world.get_entity(part).is_ok());
        world.entity_mut(ghost).despawn();
        assert!(world.get_entity(part).is_err(), "no orphaned ghost meshes");
    }

    #[test]
    fn without_materials_the_systems_do_nothing() {
        let mut world = World::new();
        world.init_resource::<PlacementTarget>();
        let root = placed_root(&mut world);
        let part = primitive(&mut world, root);
        let original = handle(&world, part);
        world.run_system_once(apply_block_look).unwrap();
        world.run_system_once(retint_ghost).unwrap();
        assert_eq!(handle(&world, part), original);
    }
}
