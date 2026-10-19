use serde_json::json;

use super::*;
use crate::blocks::definition::{BlockDimensions, BlockEconomyConfig, BlockModel};
use crate::blocks::grid::GridBounds;
use crate::blocks::{BlockDefinition, BlockId, BlockRegistry, PlacedBlock};
use crate::items::ItemRef;
use crate::items::economy::Wallet;
use crate::items::inventory::Inventory;
use crate::objectives::{ObjectiveTracker, StageObjectives, evaluate};

fn def(id: &str, footprint: UVec3) -> BlockDefinition {
    BlockDefinition::new(
        BlockId::new(id),
        id,
        BlockModel::glb("x.glb"),
        BlockDimensions::new(footprint, footprint.as_vec3()),
        BlockEconomyConfig::with_default_price(1),
    )
}

fn registry() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    r.register(def("block.cube", UVec3::ONE)).unwrap();
    r.register(def("block.slab", UVec3::new(2, 1, 1))).unwrap();
    r.register(def("block.tower", UVec3::new(1, 2, 1))).unwrap();
    r
}

fn grid() -> GridSpec {
    GridSpec::new(
        Vec3::ZERO,
        Vec3::ONE,
        Some(GridBounds {
            min: IVec3::ZERO,
            size: UVec3::new(16, 8, 16),
        }),
    )
    .unwrap()
}

struct Site {
    world: PlacedWorld,
    ecs: World,
    registry: BlockRegistry,
}

impl Site {
    fn new() -> Self {
        Self {
            world: PlacedWorld::default(),
            ecs: World::new(),
            registry: registry(),
        }
    }

    fn put(&mut self, id: &str, cell: (i32, i32, i32), rot: GridRotation) -> Entity {
        let e = self.ecs.spawn_empty().id();
        let block = BlockId::new(id);
        let placed = PlacedBlock::new(
            &self.registry,
            &grid(),
            block.clone(),
            GridCoord::new(cell.0, cell.1, cell.2),
            rot,
        )
        .unwrap();
        self.world
            .insert(&placed, self.registry.require(&block).unwrap(), e)
            .unwrap();
        e
    }

    fn cube(&mut self, x: i32, y: i32, z: i32) -> Entity {
        self.put("block.cube", (x, y, z), GridRotation::Deg0)
    }

    fn unsupported(&self) -> Vec<Entity> {
        unsupported_blocks(&self.world, 0)
    }

    fn at(&self, x: i32, y: i32, z: i32) -> Option<Entity> {
        self.world.occupant(GridCoord::new(x, y, z))
    }

    fn can_place(&self, cells: &[(i32, i32, i32)]) -> bool {
        let cells: Vec<_> = cells
            .iter()
            .map(|c| GridCoord::new(c.0, c.1, c.2))
            .collect();
        placement_supported(&self.world, 0, &cells)
    }
}

fn sorted(mut v: Vec<Entity>) -> Vec<Entity> {
    v.sort();
    v
}

// ------------------------------------------------------------ support rule

#[test]
fn an_empty_grid_has_nothing_unsupported() {
    assert!(Site::new().unsupported().is_empty());
    assert!(settle(&mut PlacedWorld::default(), 0).is_empty());
}

#[test]
fn ground_blocks_and_stacks_are_supported_floating_ones_are_not() {
    let mut s = Site::new();
    for y in 0..5 {
        s.cube(2, y, 2);
    }
    assert!(s.unsupported().is_empty());
    let floating = s.cube(9, 3, 9);
    assert_eq!(s.unsupported(), vec![floating]);
}

#[test]
fn disconnected_structures_and_diagonal_contact_are_unsupported() {
    let mut s = Site::new();
    s.cube(0, 0, 0);
    let a = s.cube(8, 4, 8);
    let b = s.cube(9, 4, 8);
    let corner = s.cube(1, 1, 1);
    let edge = s.cube(1, 1, 0);
    assert_eq!(s.unsupported(), sorted(vec![a, b, corner, edge]));
}

#[test]
fn support_spreads_sideways_and_multiple_foundations_work() {
    let mut s = Site::new();
    s.cube(0, 0, 0);
    s.cube(0, 1, 0);
    s.cube(1, 1, 0);
    s.cube(10, 0, 10);
    s.cube(10, 1, 10);
    assert!(s.unsupported().is_empty());
}

#[test]
fn multi_cell_and_rotated_blocks_are_one_object_using_their_real_cells() {
    let mut s = Site::new();
    let slab = s.put("block.slab", (5, 4, 5), GridRotation::Deg0);
    assert_eq!(s.world.cells().count(), 2);
    assert_eq!(
        s.unsupported(),
        vec![slab],
        "reported once, not once per cell"
    );

    let mut s = Site::new();
    s.put("block.slab", (6, 0, 6), GridRotation::Deg90); // (6,0,6) and (6,0,5)
    s.cube(6, 1, 5);
    assert!(s.unsupported().is_empty());
    let wrong = s.cube(7, 1, 6); // above where the unrotated slab would be
    assert_eq!(s.unsupported(), vec![wrong]);
}

// ------------------------------------------------------------- placement

#[test]
fn placement_needs_the_ground_or_a_face_contact() {
    let mut s = Site::new();
    s.cube(3, 0, 3);
    assert!(s.can_place(&[(0, 0, 0)]), "ground");
    assert!(s.can_place(&[(3, 1, 3)]), "on a block");
    assert!(
        s.can_place(&[(4, 1, 3), (4, 0, 3)]),
        "one cell on the ground"
    );
    assert!(!s.can_place(&[(6, 3, 6)]), "floating");
    assert!(!s.can_place(&[(4, 1, 4)]), "diagonal contact only");
    assert_eq!(s.world.len(), 1, "asking never changes the world");
}

#[test]
fn the_preview_check_uses_the_rotated_footprint_and_changes_nothing() {
    let mut s = Site::new();
    s.cube(3, 0, 3);
    let slab = s.registry.require(&BlockId::new("block.slab")).unwrap();
    let at = GridCoord::new(4, 1, 3);
    // Deg0 covers (4,1,3),(5,1,3): nothing touches. Deg180 covers (4,1,3),(3,1,3): above the cube.
    assert!(check_placement_support(&s.world, &grid(), slab, at, GridRotation::Deg0).is_err());
    assert!(check_placement_support(&s.world, &grid(), slab, at, GridRotation::Deg180).is_ok());
    assert_eq!(s.world.len(), 1);
}

// ------------------------------------------------------- collapse = landing

#[test]
fn removing_the_bottom_block_lands_the_top_block_on_the_ground() {
    // The scenario from the bug report.
    let mut s = Site::new();
    let bottom = s.cube(4, 0, 4);
    let top = s.cube(4, 1, 4);
    s.world.remove(bottom); // the hammer removed the support
    let landings = settle(&mut s.world, 0);
    assert_eq!(
        landings,
        vec![Landing {
            blocks: vec![top],
            drop: 1
        }]
    );
    // The block is still there, now on the ground.
    assert_eq!(s.at(4, 0, 4), Some(top));
    assert_eq!(s.at(4, 1, 4), None);
    assert_eq!(s.world.len(), 1);
    assert_eq!(s.world.record(top).unwrap().cell, GridCoord::new(4, 0, 4));
    assert!(s.unsupported().is_empty());
}

#[test]
fn a_column_falls_together_and_keeps_its_shape() {
    let mut s = Site::new();
    let base = s.cube(3, 0, 3);
    let mid = s.cube(3, 1, 3);
    let top = s.cube(3, 2, 3);
    s.world.remove(base);
    let landings = settle(&mut s.world, 0);
    assert_eq!(landings.len(), 1);
    assert_eq!(landings[0].blocks, sorted(vec![mid, top]));
    assert_eq!(
        (s.at(3, 0, 3), s.at(3, 1, 3), s.at(3, 2, 3)),
        (Some(mid), Some(top), None)
    );
}

#[test]
fn a_floating_block_lands_on_the_block_below_it() {
    let mut s = Site::new();
    s.cube(5, 0, 5);
    s.cube(5, 1, 5);
    let floating = s.cube(5, 5, 5);
    let landings = settle(&mut s.world, 0);
    assert_eq!(
        landings,
        vec![Landing {
            blocks: vec![floating],
            drop: 3
        }]
    );
    assert_eq!(s.at(5, 2, 5), Some(floating), "rests on the block at y = 1");
    assert!(s.unsupported().is_empty());
}

#[test]
fn groups_below_land_first_so_upper_groups_land_on_them() {
    let mut s = Site::new();
    let lower = s.cube(8, 2, 8);
    let upper = s.cube(8, 5, 8); // same column, not touching
    let landings = settle(&mut s.world, 0);
    assert_eq!(landings.len(), 2);
    assert_eq!((s.at(8, 0, 8), s.at(8, 1, 8)), (Some(lower), Some(upper)));
    assert!(s.unsupported().is_empty());
    assert!(
        settle(&mut s.world, 0).is_empty(),
        "settling again changes nothing"
    );
}

#[test]
fn separate_groups_fall_by_their_own_distance() {
    let mut s = Site::new();
    let lone = s.cube(8, 5, 8);
    let low = s.cube(2, 1, 2);
    let high = s.cube(2, 2, 2);
    s.cube(2, 0, 9); // an unrelated stable block
    let landings = settle(&mut s.world, 0);
    assert_eq!(landings.len(), 2);
    assert_eq!(s.at(8, 0, 8), Some(lone));
    assert_eq!((s.at(2, 0, 2), s.at(2, 1, 2)), (Some(low), Some(high)));
    assert_eq!(s.at(2, 0, 9).is_some(), true);
}

#[test]
fn multi_cell_blocks_land_as_one_object() {
    let mut s = Site::new();
    s.cube(6, 2, 5); // a stable-looking block that is itself supported below
    s.cube(6, 1, 5);
    s.cube(6, 0, 5);
    let slab = s.put("block.slab", (5, 5, 5), GridRotation::Deg0); // (5,5,5),(6,5,5)
    let landings = settle(&mut s.world, 0);
    assert_eq!(
        landings,
        vec![Landing {
            blocks: vec![slab],
            drop: 2
        }]
    );
    // The cell above the tower stops it; the free end hangs over open air but stays attached.
    assert_eq!(s.at(6, 3, 5), Some(slab));
    assert_eq!(s.at(5, 3, 5), Some(slab));
    assert_eq!(s.world.record(slab).unwrap().cell, GridCoord::new(5, 3, 5));
}

#[test]
fn a_rotated_block_keeps_its_orientation_when_it_lands() {
    let mut s = Site::new();
    let slab = s.put("block.slab", (6, 3, 6), GridRotation::Deg90); // (6,3,6),(6,3,5)
    settle(&mut s.world, 0);
    let record = s.world.record(slab).unwrap();
    assert_eq!(record.rotation, GridRotation::Deg90);
    assert_eq!((s.at(6, 0, 6), s.at(6, 0, 5)), (Some(slab), Some(slab)));
}

#[test]
fn a_tall_block_lands_without_colliding_with_itself() {
    let mut s = Site::new();
    let tower = s.put("block.tower", (10, 4, 10), GridRotation::Deg0); // (10,4,10),(10,5,10)
    let landings = settle(&mut s.world, 0);
    assert_eq!(
        landings,
        vec![Landing {
            blocks: vec![tower],
            drop: 4
        }]
    );
    assert_eq!(
        (s.at(10, 0, 10), s.at(10, 1, 10)),
        (Some(tower), Some(tower))
    );
}

#[test]
fn shifting_is_atomic_and_refuses_to_overlap() {
    let mut s = Site::new();
    let a = s.cube(1, 2, 1);
    let b = s.cube(1, 1, 1);
    let blocker = s.cube(1, 0, 1);
    let before = s.world.cells().count();
    // The group {a} cannot drop through b.
    assert!(!s.world.shift_down(&[a], 1));
    assert!(!s.world.shift_down(&[a], 0));
    assert_eq!(s.world.cells().count(), before);
    assert_eq!(
        (s.at(1, 2, 1), s.at(1, 1, 1), s.at(1, 0, 1)),
        (Some(a), Some(b), Some(blocker))
    );
    assert_eq!(s.world.record(a).unwrap().cell, GridCoord::new(1, 2, 1));
}

// ------------------------------------------------- other systems stay intact

#[test]
fn landing_keeps_objective_progress_inventory_and_money() {
    let mut s = Site::new();
    let wallet = Wallet::new(25);
    let mut inventory = Inventory::default();
    inventory
        .add(ItemRef::Block(BlockId::new("block.cube")), 4)
        .unwrap();
    let (w0, i0) = (wallet, inventory.clone());
    let doc = json!({"tasks": [{"id": "t", "type": "construction",
        "data": {"kind": "place_block", "block": "block.cube", "count": 3}}]});
    let rules = crate::blocks::stage_rules::StagePlacementRules::from_construction(
        &json!({"allowed_blocks": [{"id": "block.cube"}]}),
    );
    let list = StageObjectives::parse(&doc, &s.registry, &rules, &|_| None).unwrap();
    let mut tracker = ObjectiveTracker::default();
    let base = s.cube(1, 0, 1);
    s.cube(1, 1, 1);
    evaluate(&list, &s.world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 2);
    s.world.remove(base);
    evaluate(&list, &s.world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 1);
    settle(&mut s.world, 0);
    evaluate(&list, &s.world, &mut tracker);
    assert_eq!(
        tracker.states()[0].progress,
        1,
        "the block fell; it was not removed"
    );
    assert_eq!(s.world.count_of(&BlockId::new("block.cube")), 1);
    assert_eq!((wallet, inventory), (w0, i0));
}

#[test]
fn placing_a_support_means_nothing_needs_to_fall() {
    let mut s = Site::new();
    s.cube(3, 1, 3);
    s.cube(3, 0, 3); // support placed in time
    assert!(settle(&mut s.world, 0).is_empty());
    assert_eq!(
        (s.at(3, 0, 3).is_some(), s.at(3, 1, 3).is_some()),
        (true, true)
    );
}

#[test]
fn the_floor_follows_the_grid_bounds() {
    assert_eq!(floor_y(&grid()), 0);
    assert_eq!(floor_y(&GridSpec::default()), 0);
    let raised = GridSpec::new(
        Vec3::ZERO,
        Vec3::ONE,
        Some(GridBounds {
            min: IVec3::new(0, 2, 0),
            size: UVec3::ONE,
        }),
    )
    .unwrap();
    assert_eq!(floor_y(&raised), 2);
    // Blocks land on the raised floor.
    let mut s = Site::new();
    let e = s.ecs.spawn_empty().id();
    let red = s
        .registry
        .require(&BlockId::new("block.cube"))
        .unwrap()
        .clone();
    let placed = PlacedBlock {
        block: red.id.clone(),
        cell: GridCoord::new(0, 5, 0),
        rotation: GridRotation::Deg0,
    };
    s.world.insert(&placed, &red, e).unwrap();
    settle(&mut s.world, 2);
    assert_eq!(s.world.occupant(GridCoord::new(0, 2, 0)), Some(e));
}

// ----------------------------------------------------------------- animation

#[test]
fn the_fall_animation_is_visual_only_and_bounded() {
    let e = World::new().spawn_empty().id();
    let mut anim = FallAnimations::default();
    anim.begin(e, 2.0);
    let first = *anim.get(e).unwrap();
    assert!(first.duration >= MIN_FALL_SECS && first.drop == 2.0);
    // Extending (a second fall) adds up instead of restarting.
    anim.begin(e, 1.0);
    assert_eq!(anim.get(e).unwrap().drop, 3.0);
    assert!(anim.get(e).unwrap().duration > first.duration);
    assert_eq!(anim.len(), 1);
    assert_eq!(fall_offset(100.0, 3.0), 3.0, "never overshoots");
    assert_eq!(fall_offset(0.0, 3.0), 0.0);
    assert!(fall_offset(0.2, 3.0) < fall_offset(0.4, 3.0));
    anim.advance(f32::NAN);
    anim.advance(-1.0);
    assert_eq!(anim.prune_finished(), 0);
    anim.advance(10.0);
    assert_eq!(anim.prune_finished(), 1);
    assert!(anim.is_empty());
    anim.begin(e, 1.0);
    anim.clear();
    assert!(anim.is_empty(), "stage changes clear the animations");
}
