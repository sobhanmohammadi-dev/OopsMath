use bevy::prelude::*;
use serde_json::json;

use super::definition::*;
use super::error::*;
use super::grid::*;
use super::models::{register_builtin_blocks, wood_log_id};
use super::placement::{PlacedBlock, validate_placement};
use super::registry::BlockRegistry;
use super::stage_rules::{StageBlockRule, parse_allowed_blocks};

fn sample(id: &str) -> BlockDefinition {
    BlockDefinition::new(
        BlockId::new(id),
        "Sample",
        BlockModel::glb("sample.glb"),
        BlockDimensions::new(UVec3::ONE, Vec3::ONE),
        BlockEconomyConfig::with_default_price(5),
    )
}

fn builtin() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    register_builtin_blocks(&mut r).unwrap();
    r
}

// ------------------------------------------------------------ registry

#[test]
fn valid_definition_registers() {
    let mut r = BlockRegistry::new();
    r.register(sample("block.stone")).unwrap();
    assert!(r.contains(&BlockId::new("block.stone")));
}

#[test]
fn malformed_ids_are_rejected() {
    for bad in [
        "",
        "wood_log",
        "Block.Log",
        "block..log",
        "block.1log",
        "block.lo g",
        ".block",
    ] {
        let mut r = BlockRegistry::new();
        assert!(
            matches!(r.register(sample(bad)), Err(BlockError::InvalidId { .. })),
            "'{bad}' should be rejected"
        );
    }
}

#[test]
fn duplicate_ids_are_rejected() {
    let mut r = BlockRegistry::new();
    r.register(sample("block.stone")).unwrap();
    assert_eq!(
        r.register(sample("block.stone")),
        Err(BlockError::DuplicateId(BlockId::new("block.stone")))
    );
    assert_eq!(r.len(), 1);
}

#[test]
fn invalid_dimensions_are_rejected() {
    let mut zero = sample("block.a");
    zero.dimensions.footprint_cells = UVec3::new(1, 0, 1);
    let mut huge = sample("block.b");
    huge.dimensions.footprint_cells = UVec3::splat(MAX_FOOTPRINT_CELLS + 1);
    let mut nan = sample("block.c");
    nan.dimensions.world_size = Vec3::new(f32::NAN, 1.0, 1.0);
    let mut neg = sample("block.d");
    neg.dimensions.world_size = Vec3::new(-1.0, 1.0, 1.0);
    let mut too_big = sample("block.e");
    too_big.dimensions.world_size = Vec3::new(2.0, 1.0, 1.0);
    for def in [zero, huge, nan, neg, too_big] {
        let id = def.id.clone();
        assert!(
            matches!(def.validate(), Err(BlockError::InvalidDefinition { id: i, .. }) if i == id),
            "{id} should be invalid"
        );
    }
}

#[test]
fn invalid_costs_are_rejected() {
    let def = sample("block.pricey")
        .with_economy(BlockEconomyConfig::with_default_price(MAX_UNIT_PRICE + 1));
    assert!(matches!(
        def.validate(),
        Err(BlockError::InvalidDefinition { .. })
    ));
    // A free block is legal.
    let free = sample("block.free").with_economy(BlockEconomyConfig::with_default_price(0));
    assert!(free.validate().is_ok());
}

#[test]
fn bad_model_paths_are_rejected() {
    for path in [
        "",
        "/abs/log.glb",
        "../log.glb",
        "a\\b.glb",
        "log.gltf",
        "C:/log.glb",
    ] {
        let mut def = sample("block.x");
        def.model = BlockModel::glb(path);
        assert!(def.validate().is_err(), "'{path}' should be rejected");
    }
}

#[test]
fn lookup_of_missing_block_is_an_error() {
    let r = builtin();
    let missing = BlockId::new("block.missing");
    assert!(r.get(&missing).is_none());
    assert_eq!(r.require(&missing), Err(BlockError::NotRegistered(missing)));
}

#[test]
fn variants_do_not_copy_by_hand() {
    let base = sample("block.stone");
    let cheap = base
        .variant(BlockId::new("block.stone_cheap"), "Cheap Stone")
        .with_economy(BlockEconomyConfig::with_default_price(1));
    assert_eq!(cheap.model, base.model);
    assert_eq!(cheap.economy.default_unit_price, 1);
    assert!(cheap.validate().is_ok());
}

#[test]
fn registry_iteration_is_deterministic() {
    let mut r = BlockRegistry::new();
    for id in ["block.c", "block.a", "block.b"] {
        r.register(sample(id)).unwrap();
    }
    let order: Vec<_> = r.iter().map(|d| d.id.as_str().to_owned()).collect();
    assert_eq!(order, ["block.c", "block.a", "block.b"]);
}

// ------------------------------------------------------------ wood log

#[test]
fn wood_log_is_registered_with_declared_values() {
    let r = builtin();
    let log = r.require(&wood_log_id()).unwrap();
    assert_eq!(log.id.as_str(), "block.wood_log");
    assert_eq!(log.model.path, "log.glb");
    assert_eq!(log.economy.default_unit_price, 3);
    assert_eq!(log.dimensions.footprint_cells, UVec3::new(1, 1, 2));
    assert_eq!(log.grid, GridConfig::default_block());
    assert_eq!(log.grid.placement_mode, PlacementMode::Grid);
    assert_eq!(
        log.visuals.material_policy,
        MaterialPolicy::PreserveOriginal
    );
}

#[test]
fn log_glb_matches_declared_dimensions() {
    // Read the real file: GLB header, then the JSON chunk with accessor bounds.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/log.glb");
    let bytes = std::fs::read(path).expect("assets/log.glb must exist");
    assert_eq!(&bytes[0..4], b"glTF");
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let doc: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();

    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for acc in doc["accessors"].as_array().unwrap() {
        if acc["type"] == "VEC3" && acc.get("min").is_some() {
            let v = |k: &str| {
                let a = acc[k].as_array().unwrap();
                Vec3::new(
                    a[0].as_f64().unwrap() as f32,
                    a[1].as_f64().unwrap() as f32,
                    a[2].as_f64().unwrap() as f32,
                )
            };
            min = min.min(v("min"));
            max = max.max(v("max"));
        }
    }
    let log = builtin().require(&wood_log_id()).unwrap().clone();
    let visual_size = (max - min) * log.visuals.scale;
    assert!(
        (visual_size - log.dimensions.world_size)
            .abs()
            .max_element()
            < 0.06, // the model overhangs its 1x1x2 box by up to ~2.5%,
        "scaled model {visual_size} vs declared {}",
        log.dimensions.world_size
    );
}

// ---------------------------------------------------------------- grid

#[test]
fn grid_coordinates_convert_to_world() {
    let g = GridSpec::default();
    assert_eq!(g.cell_center(GridCoord::new(0, 0, 0)), Vec3::splat(0.5));
    assert_eq!(
        g.cell_center(GridCoord::new(2, 1, -3)),
        Vec3::new(2.5, 1.5, -2.5)
    );
    let g2 = GridSpec::new(Vec3::new(10.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 2.0), None).unwrap();
    assert_eq!(
        g2.cell_min_corner(GridCoord::new(1, 0, 1)),
        Vec3::new(12.0, 0.0, 2.0)
    );
    assert_eq!(
        g2.cell_center(GridCoord::new(1, 0, 1)),
        Vec3::new(13.0, 0.5, 3.0)
    );
}

#[test]
fn snapping_is_deterministic_and_floors() {
    let g = GridSpec::default();
    assert_eq!(
        g.world_to_cell(Vec3::new(0.99, 0.0, -0.01)),
        GridCoord::new(0, 0, -1)
    );
    assert_eq!(
        g.world_to_cell(Vec3::new(1.0, 0.0, 0.0)),
        GridCoord::new(1, 0, 0)
    );
    let p = Vec3::new(3.7, 0.2, 1.1);
    assert_eq!(g.snap_to_cell_center(p), Vec3::new(3.5, 0.5, 1.5));
    assert_eq!(
        g.snap_to_cell_center(g.snap_to_cell_center(p)),
        g.snap_to_cell_center(p)
    );
}

#[test]
fn rotation_rules() {
    assert_eq!(GridRotation::Deg0.next(), GridRotation::Deg90);
    assert_eq!(GridRotation::Deg270.next(), GridRotation::Deg0);
    let f = UVec3::new(2, 1, 3);
    assert_eq!(GridRotation::Deg0.rotate_footprint(f), f);
    assert_eq!(GridRotation::Deg90.rotate_footprint(f), UVec3::new(3, 1, 2));
    assert_eq!(GridRotation::Deg180.rotate_footprint(f), f);
    assert_eq!(
        GridRotation::Deg270.rotate_footprint(f),
        UVec3::new(3, 1, 2)
    );
    assert!(AllowedRotations::None.allows(GridRotation::Deg0));
    assert!(!AllowedRotations::None.allows(GridRotation::Deg90));
}

#[test]
fn invalid_grid_specs_are_rejected() {
    assert!(GridSpec::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 1.0), None).is_err());
    assert!(GridSpec::new(Vec3::NAN, Vec3::ONE, None).is_err());
    let empty = GridBounds {
        min: IVec3::ZERO,
        size: UVec3::new(0, 1, 1),
    };
    assert!(GridSpec::new(Vec3::ZERO, Vec3::ONE, Some(empty)).is_err());
}

#[test]
fn occupied_cells_cover_the_rotated_footprint() {
    let cells: Vec<_> = occupied_cells(GridCoord::new(1, 0, 1), UVec3::new(2, 1, 1)).collect();
    assert_eq!(cells, [GridCoord::new(1, 0, 1), GridCoord::new(2, 0, 1)]);
}

// ----------------------------------------------------------- placement

fn bounded_grid() -> GridSpec {
    GridSpec::new(
        Vec3::ZERO,
        Vec3::ONE,
        Some(GridBounds {
            min: IVec3::ZERO,
            size: UVec3::new(4, 2, 4),
        }),
    )
    .unwrap()
}

#[test]
fn placement_produces_a_valid_instance_and_transform() {
    let r = builtin();
    let g = bounded_grid();
    let placed = PlacedBlock::new(
        &r,
        &g,
        wood_log_id(),
        GridCoord::new(1, 0, 2),
        GridRotation::Deg90,
    )
    .unwrap();
    let def = r.require(&placed.block).unwrap();
    let t = placed.transform(def, &g);
    // The 1x1x2 log turned 90 degrees about anchor (1,2) covers (1,2) and (2,2).
    assert_eq!(t.translation, Vec3::new(2.0, 0.5, 2.5));
    assert_eq!(t.scale, def.visuals.scale);
    assert_eq!(placed.occupied_cells(def).count(), 2);
}

#[test]
fn placement_failures_are_descriptive() {
    let r = builtin();
    let g = bounded_grid();
    let id = wood_log_id();
    assert!(matches!(
        validate_placement(
            &r,
            &g,
            &BlockId::new("block.nope"),
            GridCoord::new(0, 0, 0),
            GridRotation::Deg0
        ),
        Err(BlockError::NotRegistered(_))
    ));
    assert!(matches!(
        validate_placement(&r, &g, &id, GridCoord::new(4, 0, 0), GridRotation::Deg0),
        Err(BlockError::OutOfBounds { .. })
    ));
    assert!(matches!(
        validate_placement(&r, &g, &id, GridCoord::new(0, -1, 0), GridRotation::Deg0),
        Err(BlockError::OutOfBounds { .. })
    ));
    let mut fixed = BlockRegistry::new();
    fixed
        .register(
            sample("block.fixed")
                .with_grid(GridConfig::default_block().with_rotations(AllowedRotations::None)),
        )
        .unwrap();
    assert!(matches!(
        validate_placement(
            &fixed,
            &g,
            &BlockId::new("block.fixed"),
            GridCoord::new(0, 0, 0),
            GridRotation::Deg90
        ),
        Err(BlockError::RotationNotAllowed { .. })
    ));
}

// --------------------------------------------------------- stage rules

#[test]
fn stage_cost_overrides_the_default_price() {
    let r = builtin();
    let log = r.require(&wood_log_id()).unwrap();
    let mut rule = StageBlockRule::new(wood_log_id());
    assert_eq!(rule.effective_price(log), 3);
    rule.cost = Some(7);
    assert_eq!(rule.effective_price(log), 7);
}

#[test]
fn stage_rules_parse_and_validate() {
    let c = json!({"allowed_blocks": [
        {"id": "block.wood_log", "cost": 3, "min_count": 0, "max_count": 20},
        {"id": "brick", "max_count": 20, "min_count": 0}
    ]});
    let rules = parse_allowed_blocks(&c).unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].cost, Some(3));
    assert_eq!(rules[1].cost, None);
    assert!(rules[0].allows_count(20) && !rules[0].allows_count(21));
    // The existing stage id 'brick' is not registered: resolution reports it.
    assert!(matches!(
        rules[1].definition(&builtin()),
        Err(BlockError::NotRegistered(_))
    ));
    assert!(parse_allowed_blocks(&json!({})).unwrap().is_empty());
    for bad in [
        json!({"allowed_blocks": [{"cost": 1}]}),
        json!({"allowed_blocks": [{"id": "block.x", "cost": -1}]}),
        json!({"allowed_blocks": [{"id": "block.x", "min_count": 5, "max_count": 2}]}),
        json!({"allowed_blocks": "nope"}),
    ] {
        assert!(matches!(
            parse_allowed_blocks(&bad),
            Err(BlockError::InvalidStageRule(_))
        ));
    }
}

// ------------------------------------------------- targeting & occupancy

use super::occupancy::{PlacedWorld, PlacementRejection, evaluate_placement};
use super::targeting::ray_cell_target;

fn stage_grid() -> GridSpec {
    GridSpec::new(
        Vec3::new(10.0, 0.0, 20.0),
        Vec3::ONE,
        Some(GridBounds {
            min: IVec3::ZERO,
            size: UVec3::new(16, 8, 16),
        }),
    )
    .unwrap()
}

fn log_only() -> impl Fn(&BlockId) -> bool {
    |id| id == &wood_log_id()
}

fn place(
    world: &mut PlacedWorld,
    registry: &BlockRegistry,
    grid: &GridSpec,
    id: &BlockId,
    cell: GridCoord,
    rotation: GridRotation,
    entity: Entity,
) -> Result<(), PlacementRejection> {
    let def = evaluate_placement(registry, grid, world, |_| true, Some(id), cell, rotation)?;
    let placed = PlacedBlock::new(registry, grid, id.clone(), cell, rotation).unwrap();
    world.insert(&placed, def, entity)
}

#[test]
fn ray_down_hits_the_floor_cell_using_the_grid_origin() {
    let g = stage_grid();
    // Straight down onto world (12.5, 0, 23.5) -> cell (2, 0, 3).
    let cell = ray_cell_target(
        Vec3::new(12.5, 5.0, 23.5),
        Vec3::NEG_Y,
        &g,
        std::iter::empty(),
    );
    assert_eq!(cell, Some(GridCoord::new(2, 0, 3)));
    // Oblique ray lands where the plane intersection says (floor, not rounding).
    let cell = ray_cell_target(
        Vec3::new(10.5, 2.0, 20.5),
        Vec3::new(1.0, -1.0, 0.0).normalize(),
        &g,
        std::iter::empty(),
    );
    assert_eq!(cell, Some(GridCoord::new(2, 0, 0)));
}

#[test]
fn rays_that_miss_the_floor_return_none() {
    let g = stage_grid();
    assert_eq!(
        ray_cell_target(Vec3::new(12.5, 5.0, 23.5), Vec3::Y, &g, std::iter::empty()),
        None
    );
    assert_eq!(
        ray_cell_target(Vec3::new(12.5, 5.0, 23.5), Vec3::X, &g, std::iter::empty()),
        None
    );
}

#[test]
fn rays_target_the_face_of_an_occupied_cell() {
    let g = stage_grid();
    let occupied = [GridCoord::new(2, 0, 3)];
    // From above: stack on top.
    let top = ray_cell_target(
        Vec3::new(12.5, 5.0, 23.5),
        Vec3::NEG_Y,
        &g,
        occupied.into_iter(),
    );
    assert_eq!(top, Some(GridCoord::new(2, 1, 3)));
    // From +X at mid height: the neighbour on the +X side.
    let side = ray_cell_target(
        Vec3::new(20.0, 0.5, 23.5),
        Vec3::NEG_X,
        &g,
        occupied.into_iter(),
    );
    assert_eq!(side, Some(GridCoord::new(3, 0, 3)));
}

#[test]
fn valid_single_cell_placement_is_recorded() {
    let r = builtin();
    let g = stage_grid();
    let mut world = PlacedWorld::default();
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    place(
        &mut world,
        &r,
        &g,
        &wood_log_id(),
        GridCoord::new(1, 0, 1),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    assert_eq!(world.len(), 1);
    assert!(world.is_occupied(GridCoord::new(1, 0, 1)));
    assert_eq!(world.occupant(GridCoord::new(1, 0, 1)), Some(e));
    let rec = world.record(e).unwrap();
    assert_eq!(
        (&rec.block, rec.cell),
        (&wood_log_id(), GridCoord::new(1, 0, 1))
    );
}

#[test]
fn occupied_target_is_rejected_without_mutation() {
    let r = builtin();
    let g = stage_grid();
    let mut world = PlacedWorld::default();
    let mut ecs = World::new();
    let (a, b) = (ecs.spawn_empty().id(), ecs.spawn_empty().id());
    let log = wood_log_id();
    place(
        &mut world,
        &r,
        &g,
        &log,
        GridCoord::new(1, 0, 1),
        GridRotation::Deg0,
        a,
    )
    .unwrap();
    let err = place(
        &mut world,
        &r,
        &g,
        &log,
        GridCoord::new(1, 0, 1),
        GridRotation::Deg0,
        b,
    )
    .unwrap_err();
    assert!(matches!(err, PlacementRejection::Occupied { .. }));
    assert_eq!(world.len(), 1);
    assert_eq!(world.occupant(GridCoord::new(1, 0, 1)), Some(a));
    assert!(world.record(b).is_none());
}

#[test]
fn placement_rules_reject_with_clear_reasons() {
    let r = builtin();
    let g = stage_grid();
    let world = PlacedWorld::default();
    let log = wood_log_id();
    let cell = GridCoord::new(0, 0, 0);
    let eval = |allowed: bool, sel: Option<&BlockId>, c: GridCoord| {
        evaluate_placement(&r, &g, &world, move |_| allowed, sel, c, GridRotation::Deg0).map(|_| ())
    };
    assert_eq!(
        eval(true, None, cell),
        Err(PlacementRejection::NothingSelected)
    );
    assert_eq!(
        eval(false, Some(&log), cell),
        Err(PlacementRejection::NotAllowed(log.clone()))
    );
    let ghost = BlockId::new("block.ghost");
    assert_eq!(
        eval(true, Some(&ghost), cell),
        Err(PlacementRejection::NotRegistered(ghost.clone()))
    );
    for out in [
        GridCoord::new(16, 0, 0),
        GridCoord::new(0, -1, 0),
        GridCoord::new(0, 8, 0),
    ] {
        assert!(matches!(
            eval(true, Some(&log), out),
            Err(PlacementRejection::OutOfBounds { .. })
        ));
    }
    assert_eq!(eval(true, Some(&log), GridCoord::new(15, 7, 14)), Ok(()));
    // The allow-list closure is honoured against the actual id.
    assert!(
        evaluate_placement(
            &r,
            &g,
            &world,
            log_only(),
            Some(&log),
            cell,
            GridRotation::Deg0
        )
        .is_ok()
    );
}

fn slab_registry() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    let mut slab = sample("block.slab");
    slab.dimensions = BlockDimensions::new(UVec3::new(2, 1, 1), Vec3::new(2.0, 1.0, 1.0));
    r.register(slab).unwrap();
    r
}

#[test]
fn multi_cell_blocks_occupy_and_collide_by_footprint() {
    let r = slab_registry();
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let mut world = PlacedWorld::default();
    let mut ecs = World::new();
    let (a, b, c) = (
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
    );
    place(
        &mut world,
        &r,
        &g,
        &slab,
        GridCoord::new(2, 0, 2),
        GridRotation::Deg0,
        a,
    )
    .unwrap();
    assert!(
        world.is_occupied(GridCoord::new(2, 0, 2)) && world.is_occupied(GridCoord::new(3, 0, 2))
    );
    assert!(!world.is_occupied(GridCoord::new(4, 0, 2)));
    // Partially overlapping second slab is rejected and changes nothing.
    assert!(
        place(
            &mut world,
            &r,
            &g,
            &slab,
            GridCoord::new(3, 0, 2),
            GridRotation::Deg0,
            b
        )
        .is_err()
    );
    assert_eq!(world.len(), 1);
    assert!(!world.is_occupied(GridCoord::new(4, 0, 2)));
    // Rotated 90 degrees it extends along Z instead.
    place(
        &mut world,
        &r,
        &g,
        &slab,
        GridCoord::new(6, 0, 6),
        GridRotation::Deg90,
        c,
    )
    .unwrap();
    // Turned about the anchor cell: from (6,6) it extends towards -Z.
    assert!(
        world.is_occupied(GridCoord::new(6, 0, 5)) && !world.is_occupied(GridCoord::new(7, 0, 6))
    );
    // Footprint must fit the bounds: x = 15 + 2 cells leaves the 16-wide grid.
    assert!(matches!(
        evaluate_placement(
            &r,
            &g,
            &world,
            |_| true,
            Some(&slab),
            GridCoord::new(15, 0, 0),
            GridRotation::Deg0
        ),
        Err(PlacementRejection::OutOfBounds { .. })
    ));
}

#[test]
fn evaluating_a_preview_never_touches_occupancy() {
    let r = builtin();
    let g = stage_grid();
    let world = PlacedWorld::default();
    let log = wood_log_id();
    for x in 0..4 {
        let _ = evaluate_placement(
            &r,
            &g,
            &world,
            log_only(),
            Some(&log),
            GridCoord::new(x, 0, 0),
            GridRotation::Deg0,
        );
    }
    assert!(world.is_empty());
    assert_eq!(world.occupied_cells().count(), 0);
}

#[test]
fn clearing_the_world_forgets_everything() {
    let r = builtin();
    let g = stage_grid();
    let mut world = PlacedWorld::default();
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    place(
        &mut world,
        &r,
        &g,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    world.clear();
    assert!(world.is_empty() && !world.is_occupied(GridCoord::new(0, 0, 0)));
    assert!(world.record(e).is_none());
}

// ------------------------------------- stage rules, limits and inventory

use super::occupancy::{commit_placement, evaluate_commit};
use super::stage_rules::StagePlacementRules;
use crate::items::ItemRef;
use crate::items::inventory::Inventory;

fn rules(allowed: serde_json::Value) -> StagePlacementRules {
    StagePlacementRules::from_construction(&json!({ "allowed_blocks": allowed }))
}

fn inventory_with(id: &BlockId, quantity: u32) -> Inventory {
    let mut inv = Inventory::default();
    if quantity > 0 {
        inv.add(ItemRef::Block(id.clone()), quantity).unwrap();
    }
    inv
}

/// Full attempt as the placement system does it: decide, then commit.
fn attempt(
    registry: &BlockRegistry,
    grid: &GridSpec,
    world: &mut PlacedWorld,
    rules: &StagePlacementRules,
    inventory: &mut Inventory,
    id: &BlockId,
    cell: GridCoord,
    entity: Entity,
) -> Result<(), PlacementRejection> {
    let def = evaluate_commit(
        registry,
        grid,
        world,
        rules,
        inventory,
        Some(id),
        cell,
        GridRotation::Deg0,
    )?;
    let placed = PlacedBlock::new(registry, grid, id.clone(), cell, GridRotation::Deg0).unwrap();
    commit_placement(world, inventory, &placed, def, entity)
}

#[test]
fn placement_rules_parse_allowlist_and_limits() {
    let r = rules(json!([{"id": "block.wood_log", "max_count": 15}, {"id": "block.stone"}]));
    assert!(r.is_allowed(&wood_log_id()) && r.is_allowed(&BlockId::new("block.stone")));
    assert_eq!(r.max_placed(&wood_log_id()), Some(15));
    assert_eq!(
        r.max_placed(&BlockId::new("block.stone")),
        None,
        "absent = no instance cap"
    );
    assert!(!r.is_allowed(&BlockId::new("block.other")));
    assert_eq!(r.error, None);
}

#[test]
fn legacy_or_malformed_rules_never_grant_unrestricted_placement() {
    let legacy = StagePlacementRules::from_construction(&json!({}));
    assert!(legacy.is_empty() && !legacy.is_allowed(&wood_log_id()));
    let bad = rules(json!([{"id": "block.wood_log", "max_count": -3}]));
    assert!(bad.error.is_some() && bad.is_empty() && !bad.is_allowed(&wood_log_id()));
}

#[test]
fn stage_change_replaces_the_rules() {
    let a = rules(json!([{"id": "block.wood_log", "max_count": 2}]));
    let b = rules(json!([{"id": "block.stone"}]));
    assert!(a.is_allowed(&wood_log_id()) && !b.is_allowed(&wood_log_id()));
    assert_eq!(b.max_placed(&wood_log_id()), None);
}

#[test]
fn commit_rejects_disallowed_unknown_and_unselected() {
    let r = builtin();
    let g = stage_grid();
    let world = PlacedWorld::default();
    let log = wood_log_id();
    let inv = inventory_with(&log, 5);
    let cell = GridCoord::new(0, 0, 0);
    let go = |rules: &StagePlacementRules, sel: Option<&BlockId>| {
        evaluate_commit(&r, &g, &world, rules, &inv, sel, cell, GridRotation::Deg0).map(|_| ())
    };
    let ok = rules(json!([{"id": "block.wood_log"}]));
    assert_eq!(go(&ok, Some(&log)), Ok(()));
    assert_eq!(
        go(&ok, None),
        Err(PlacementRejection::NothingSelected),
        "a selected tool derives no block"
    );
    assert_eq!(
        go(&rules(json!([])), Some(&log)),
        Err(PlacementRejection::NotAllowed(log.clone()))
    );
    let ghost = BlockId::new("block.ghost");
    let allows_ghost = rules(json!([{"id": "block.ghost"}]));
    assert_eq!(
        go(&allows_ghost, Some(&ghost)),
        Err(PlacementRejection::NotRegistered(ghost))
    );
}

#[test]
fn placement_consumes_exactly_one_item() {
    let r = builtin();
    let g = stage_grid();
    let log = wood_log_id();
    let rules = rules(json!([{"id": "block.wood_log"}]));
    let mut world = PlacedWorld::default();
    let mut inv = inventory_with(&log, 3);
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    attempt(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &log,
        GridCoord::new(1, 0, 1),
        e,
    )
    .unwrap();
    assert_eq!(inv.quantity(&ItemRef::Block(log.clone())), 2);
    assert_eq!(world.len(), 1);
    assert_eq!(world.count_of(&log), 1);
}

#[test]
fn rejected_attempts_consume_nothing_and_change_nothing() {
    let r = builtin();
    let g = stage_grid();
    let log = wood_log_id();
    let rules = rules(json!([{"id": "block.wood_log", "max_count": 1}]));
    let mut ecs = World::new();
    let (a, b, c, d) = (
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
    );

    // Zero inventory.
    let mut world = PlacedWorld::default();
    let mut none = inventory_with(&log, 0);
    let err = attempt(
        &r,
        &g,
        &mut world,
        &rules,
        &mut none,
        &log,
        GridCoord::new(0, 0, 0),
        a,
    )
    .unwrap_err();
    assert_eq!(err, PlacementRejection::NoInventory(log.clone()));
    assert!(world.is_empty() && none.is_empty());

    // Out of bounds / invalid target.
    let mut inv = inventory_with(&log, 5);
    let err = attempt(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &log,
        GridCoord::new(99, 0, 0),
        b,
    )
    .unwrap_err();
    assert!(matches!(err, PlacementRejection::OutOfBounds { .. }));
    assert_eq!(inv.quantity(&ItemRef::Block(log.clone())), 5);
    assert!(world.is_empty());

    // Occupied target (limit raised so occupancy is what rejects it).
    let loose = rules_loose();
    attempt(
        &r,
        &g,
        &mut world,
        &loose,
        &mut inv,
        &log,
        GridCoord::new(2, 0, 2),
        c,
    )
    .unwrap();
    assert_eq!(inv.quantity(&ItemRef::Block(log.clone())), 4);
    let err = attempt(
        &r,
        &g,
        &mut world,
        &loose,
        &mut inv,
        &log,
        GridCoord::new(2, 0, 2),
        d,
    )
    .unwrap_err();
    assert!(matches!(err, PlacementRejection::Occupied { .. }));
    assert_eq!(inv.quantity(&ItemRef::Block(log.clone())), 4);
    assert_eq!(world.len(), 1);
    assert!(world.record(d).is_none());

    // Limit of 1 reached.
    let err = attempt(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &log,
        GridCoord::new(5, 0, 5),
        d,
    )
    .unwrap_err();
    assert_eq!(
        err,
        PlacementRejection::LimitReached {
            block: log.clone(),
            max: 1
        }
    );
    assert_eq!(inv.quantity(&ItemRef::Block(log)), 4);
    assert_eq!(world.len(), 1);
}

pub(super) fn rules_loose() -> StagePlacementRules {
    rules(json!([{"id": "block.wood_log"}]))
}

#[test]
fn limits_count_instances_not_cells_or_previews() {
    let r = slab_registry(); // 2-cell blocks
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let rules = rules(json!([{"id": "block.slab", "max_count": 2}]));
    let mut inv = inventory_with(&slab, 10);
    let mut world = PlacedWorld::default();
    let mut ecs = World::new();
    let e: Vec<_> = (0..3).map(|_| ecs.spawn_empty().id()).collect();
    // Previewing (evaluating) repeatedly never counts.
    for _ in 0..5 {
        let _ = evaluate_commit(
            &r,
            &g,
            &world,
            &rules,
            &inv,
            Some(&slab),
            GridCoord::new(0, 0, 0),
            GridRotation::Deg0,
        );
    }
    assert_eq!(world.count_of(&slab), 0);
    attempt(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(0, 0, 0),
        e[0],
    )
    .unwrap();
    assert_eq!(world.count_of(&slab), 1, "one instance, two cells");
    assert_eq!(world.occupied_cells().count(), 2);
    attempt(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(4, 0, 0),
        e[1],
    )
    .unwrap();
    assert_eq!(
        attempt(
            &r,
            &g,
            &mut world,
            &rules,
            &mut inv,
            &slab,
            GridCoord::new(8, 0, 0),
            e[2]
        ),
        Err(PlacementRejection::LimitReached {
            block: slab.clone(),
            max: 2
        })
    );
    assert_eq!(
        inv.quantity(&ItemRef::Block(slab.clone())),
        8,
        "exactly one unit per instance"
    );
}

#[test]
fn removing_a_record_frees_cells_and_the_count() {
    let r = slab_registry();
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let mut world = PlacedWorld::default();
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    place(
        &mut world,
        &r,
        &g,
        &slab,
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    assert_eq!(world.count_of(&slab), 1);
    assert!(world.remove(e).is_some());
    assert_eq!(world.count_of(&slab), 0);
    assert_eq!(world.occupied_cells().count(), 0);
    assert!(world.remove(e).is_none());
}

#[test]
fn commit_rolls_back_the_world_if_the_inventory_cannot_pay() {
    let r = builtin();
    let g = stage_grid();
    let log = wood_log_id();
    let def = r.require(&log).unwrap();
    let placed = PlacedBlock::new(
        &r,
        &g,
        log.clone(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    )
    .unwrap();
    let mut world = PlacedWorld::default();
    let mut empty = Inventory::default();
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    assert_eq!(
        commit_placement(&mut world, &mut empty, &placed, def, e),
        Err(PlacementRejection::NoInventory(log))
    );
    assert!(world.is_empty() && !world.is_occupied(GridCoord::new(0, 0, 0)));
}

// ---------------------------------------------------------- rotation

fn attempt_rot(
    registry: &BlockRegistry,
    grid: &GridSpec,
    world: &mut PlacedWorld,
    rules: &StagePlacementRules,
    inventory: &mut Inventory,
    id: &BlockId,
    cell: GridCoord,
    rotation: GridRotation,
    entity: Entity,
) -> Result<(), PlacementRejection> {
    let def = evaluate_commit(
        registry,
        grid,
        world,
        rules,
        inventory,
        Some(id),
        cell,
        rotation,
    )?;
    let placed = PlacedBlock::new(registry, grid, id.clone(), cell, rotation).unwrap();
    commit_placement(world, inventory, &placed, def, entity)
}

#[test]
fn quarter_turn_stepping_wraps_after_four() {
    let mut r = GridRotation::Deg0;
    let mut seen = Vec::new();
    for _ in 0..4 {
        r = AllowedRotations::QuarterTurns.step(r);
        seen.push(r.quarter_turns());
    }
    assert_eq!(seen, [1, 2, 3, 0]);
    assert_eq!(r, GridRotation::Deg0);
}

#[test]
fn non_rotatable_blocks_never_change_orientation() {
    assert!(!AllowedRotations::None.is_rotatable());
    assert_eq!(
        AllowedRotations::None.step(GridRotation::Deg0),
        GridRotation::Deg0
    );
    assert_eq!(
        AllowedRotations::None.effective(GridRotation::Deg90),
        GridRotation::Deg0
    );
    assert_eq!(
        AllowedRotations::QuarterTurns.effective(GridRotation::Deg270),
        GridRotation::Deg270
    );
    // Direct placement with a disallowed rotation is refused, not silently turned.
    let mut r = BlockRegistry::new();
    r.register(
        sample("block.fixed")
            .with_grid(GridConfig::default_block().with_rotations(AllowedRotations::None)),
    )
    .unwrap();
    let g = stage_grid();
    let id = BlockId::new("block.fixed");
    assert!(matches!(
        evaluate_placement(
            &r,
            &g,
            &PlacedWorld::default(),
            |_| true,
            Some(&id),
            GridCoord::new(0, 0, 0),
            GridRotation::Deg90
        ),
        Err(PlacementRejection::Invalid(
            BlockError::RotationNotAllowed { .. }
        ))
    ));
}

#[test]
fn footprint_follows_orientation_and_cubes_are_unchanged() {
    let slab = UVec3::new(2, 1, 1);
    let turned: Vec<_> = GridRotation::ALL
        .iter()
        .map(|r| r.rotate_footprint(slab))
        .collect();
    assert_eq!(
        turned,
        [slab, UVec3::new(1, 1, 2), slab, UVec3::new(1, 1, 2)]
    );
    for r in GridRotation::ALL {
        assert_eq!(r.rotate_footprint(UVec3::ONE), UVec3::ONE);
        assert_eq!(
            r.rotate_footprint(UVec3::new(3, 2, 3)),
            UVec3::new(3, 2, 3),
            "square plan, height kept"
        );
        assert_eq!(
            r.rotate_footprint(slab).y,
            1,
            "vertical extent never changes"
        );
    }
}

#[test]
fn rotated_footprint_detects_collisions_the_unrotated_one_misses() {
    let r = slab_registry();
    let mut reg = r;
    reg.register(sample("block.cube")).unwrap();
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let cube = BlockId::new("block.cube");
    let rules = rules(json!([{"id": "block.slab"}, {"id": "block.cube"}]));
    let mut world = PlacedWorld::default();
    let mut inv = Inventory::default();
    inv.add(ItemRef::Block(slab.clone()), 5).unwrap();
    inv.add(ItemRef::Block(cube.clone()), 5).unwrap();
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    // A cube sits at (3, 0, 4).
    attempt_rot(
        &reg,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &cube,
        GridCoord::new(3, 0, 4),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    let probe = |rot| {
        evaluate_commit(
            &reg,
            &g,
            &world,
            &rules,
            &inv,
            Some(&slab),
            GridCoord::new(3, 0, 3),
            rot,
        )
        .map(|_| ())
    };
    // From anchor (3,3) the slab extends +X, -Z, -X, +Z.
    assert_eq!(probe(GridRotation::Deg0), Ok(()));
    assert_eq!(probe(GridRotation::Deg90), Ok(()));
    assert_eq!(probe(GridRotation::Deg180), Ok(()));
    assert!(
        matches!(
            probe(GridRotation::Deg270),
            Err(PlacementRejection::Occupied { .. })
        ),
        "covers (3,3) and (3,4)"
    );
}

#[test]
fn rotated_footprint_is_bounds_checked() {
    let r = slab_registry();
    let g = stage_grid(); // 16 x 8 x 16
    let slab = BlockId::new("block.slab");
    let world = PlacedWorld::default();
    let at = GridCoord::new(0, 0, 15);
    let eval = |rot| evaluate_placement(&r, &g, &world, |_| true, Some(&slab), at, rot).map(|_| ());
    assert_eq!(eval(GridRotation::Deg0), Ok(()));
    assert_eq!(eval(GridRotation::Deg90), Ok(()), "extends to z = 14");
    assert!(
        matches!(
            eval(GridRotation::Deg180),
            Err(PlacementRejection::OutOfBounds { .. })
        ),
        "extends to x = -1"
    );
    assert!(
        matches!(
            eval(GridRotation::Deg270),
            Err(PlacementRejection::OutOfBounds { .. })
        ),
        "extends to z = 16"
    );
}

#[test]
fn preview_rotation_changes_do_not_touch_occupancy_or_inventory() {
    let r = slab_registry();
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let rules = rules(json!([{"id": "block.slab", "max_count": 3}]));
    let world = PlacedWorld::default();
    let inv = inventory_with(&slab, 4);
    let inv_before = inv.clone();
    for rot in GridRotation::ALL {
        let _ = evaluate_commit(
            &r,
            &g,
            &world,
            &rules,
            &inv,
            Some(&slab),
            GridCoord::new(1, 0, 1),
            rot,
        );
    }
    assert!(world.is_empty());
    assert_eq!(inv, inv_before);
}

#[test]
fn placement_stores_the_previewed_orientation_and_consumes_one_item() {
    let r = slab_registry();
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let rules = rules(json!([{"id": "block.slab"}]));
    let mut world = PlacedWorld::default();
    let mut inv = inventory_with(&slab, 2);
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(2, 0, 2),
        GridRotation::Deg90,
        e,
    )
    .unwrap();
    assert_eq!(world.record(e).unwrap().rotation, GridRotation::Deg90);
    assert_eq!(inv.quantity(&ItemRef::Block(slab.clone())), 1);
    // Cells are the rotated ones: along Z, not X.
    assert!(
        world.is_occupied(GridCoord::new(2, 0, 1)) && !world.is_occupied(GridCoord::new(3, 0, 2))
    );
    // The visual transform uses the same orientation and footprint centre.
    let placed = PlacedBlock::new(
        &r,
        &g,
        slab.clone(),
        GridCoord::new(2, 0, 2),
        GridRotation::Deg90,
    )
    .unwrap();
    let t = placed.transform(r.require(&slab).unwrap(), &g);
    assert_eq!(t.rotation, GridRotation::Deg90.to_quat());
    // stage_grid() starts at world (10, 0, 20): cells (2,2) and (2,1) -> centre (12.5, 0.5, 22.0).
    assert_eq!(t.translation, Vec3::new(12.5, 0.5, 22.0));
}

#[test]
fn failed_rotated_placement_changes_nothing() {
    let r = slab_registry();
    let g = stage_grid();
    let slab = BlockId::new("block.slab");
    let rules = rules(json!([{"id": "block.slab"}]));
    let mut world = PlacedWorld::default();
    let mut inv = inventory_with(&slab, 2);
    let mut ecs = World::new();
    let e = ecs.spawn_empty().id();
    let err = attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(0, 0, 15),
        GridRotation::Deg270,
        e,
    )
    .unwrap_err();
    assert!(matches!(err, PlacementRejection::OutOfBounds { .. }));
    assert!(world.is_empty());
    assert_eq!(inv.quantity(&ItemRef::Block(slab)), 2);
}

#[test]
fn a_two_cell_block_points_in_four_directions_from_the_anchor() {
    let anchor = GridCoord::new(5, 0, 5);
    let size = UVec3::new(2, 1, 1);
    let second: Vec<_> = GridRotation::ALL
        .iter()
        .map(|r| {
            rotated_cells(anchor, size, *r)
                .find(|c| *c != anchor)
                .unwrap()
        })
        .collect();
    assert_eq!(
        second,
        [
            GridCoord::new(6, 0, 5),
            GridCoord::new(5, 0, 4),
            GridCoord::new(4, 0, 5),
            GridCoord::new(5, 0, 6)
        ]
    );
    for r in GridRotation::ALL {
        assert!(
            rotated_cells(anchor, size, r).any(|c| c == anchor),
            "the aimed cell is always occupied"
        );
    }
}

#[test]
fn cells_and_visual_centre_agree_for_every_rotation() {
    let g = GridSpec::default();
    let anchor = GridCoord::new(5, 1, 5);
    for size in [
        UVec3::new(2, 1, 1),
        UVec3::new(3, 2, 2),
        UVec3::new(1, 1, 3),
        UVec3::ONE,
    ] {
        for r in GridRotation::ALL {
            let cells: Vec<_> = rotated_cells(anchor, size, r).collect();
            assert_eq!(cells.len() as u32, size.x * size.y * size.z);
            let min = cells.iter().fold(IVec3::MAX, |m, c| m.min(c.0));
            let max = cells.iter().fold(IVec3::MIN, |m, c| m.max(c.0));
            let bounds_center = (min.as_vec3() + max.as_vec3() + Vec3::ONE) * 0.5;
            assert_eq!(
                g.footprint_center(anchor, size, r),
                bounds_center,
                "{size} {r:?}"
            );
            let (bmin, bsize) = footprint_box(anchor, size, r);
            assert_eq!((bmin.0, bsize), (min, r.rotate_footprint(size)));
        }
    }
}

// ------------------------------------------------------------ hammer

use super::interaction::{ClickAction, click_action};
use super::occupancy::{RemovalError, remove_block};
use super::targeting::ray_pick_occupied;
use crate::items::tools::{ToolRegistry, hammer_id, register_builtin_tools};

fn placed_slab_world() -> (
    BlockRegistry,
    GridSpec,
    PlacedWorld,
    Inventory,
    StagePlacementRules,
    World,
) {
    let mut r = slab_registry();
    r.register(sample("block.cube")).unwrap();
    let rules = rules(json!([{"id": "block.slab"}, {"id": "block.cube"}]));
    let mut inv = Inventory::default();
    inv.add(ItemRef::Block(BlockId::new("block.slab")), 5)
        .unwrap();
    inv.add(ItemRef::Block(BlockId::new("block.cube")), 5)
        .unwrap();
    (
        r,
        stage_grid(),
        PlacedWorld::default(),
        inv,
        rules,
        World::new(),
    )
}

#[test]
fn removing_an_unrotated_block_frees_its_cell_and_refunds_one() {
    let (r, g, mut world, mut inv, rules, mut ecs) = placed_slab_world();
    let cube = BlockId::new("block.cube");
    let e = ecs.spawn_empty().id();
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &cube,
        GridCoord::new(1, 0, 1),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    assert_eq!(inv.quantity(&ItemRef::Block(cube.clone())), 4);
    let record = remove_block(&mut world, &mut inv, e).unwrap();
    assert_eq!(
        (record.block, record.cell),
        (cube.clone(), GridCoord::new(1, 0, 1))
    );
    assert!(world.is_empty() && !world.is_occupied(GridCoord::new(1, 0, 1)));
    assert_eq!(world.count_of(&cube), 0);
    assert_eq!(inv.quantity(&ItemRef::Block(cube)), 5);
}

#[test]
fn removing_a_rotated_block_releases_exactly_its_rotated_cells() {
    let (r, g, mut world, mut inv, rules, mut ecs) = placed_slab_world();
    let (slab, cube) = (BlockId::new("block.slab"), BlockId::new("block.cube"));
    let (a, b, c) = (
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
        ecs.spawn_empty().id(),
    );
    // Slab turned 90 degrees at anchor (6,6): cells (6,6) and (6,5).
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(6, 0, 6),
        GridRotation::Deg90,
        a,
    )
    .unwrap();
    // Neighbours: the cell the *unrotated* slab would have covered, and one beside the rotated one.
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &cube,
        GridCoord::new(7, 0, 6),
        GridRotation::Deg0,
        b,
    )
    .unwrap();
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &cube,
        GridCoord::new(5, 0, 5),
        GridRotation::Deg0,
        c,
    )
    .unwrap();
    remove_block(&mut world, &mut inv, a).unwrap();
    assert!(
        !world.is_occupied(GridCoord::new(6, 0, 6)) && !world.is_occupied(GridCoord::new(6, 0, 5))
    );
    assert_eq!(
        world.occupant(GridCoord::new(7, 0, 6)),
        Some(b),
        "neighbour untouched"
    );
    assert_eq!(
        world.occupant(GridCoord::new(5, 0, 5)),
        Some(c),
        "neighbour untouched"
    );
    assert_eq!(world.len(), 2);
}

#[test]
fn removal_only_clears_cells_owned_by_the_removed_entity() {
    let (r, g, mut world, mut inv, rules, mut ecs) = placed_slab_world();
    let slab = BlockId::new("block.slab");
    let (a, b) = (ecs.spawn_empty().id(), ecs.spawn_empty().id());
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(2, 0, 2),
        GridRotation::Deg0,
        a,
    )
    .unwrap();
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &slab,
        GridCoord::new(4, 0, 2),
        GridRotation::Deg0,
        b,
    )
    .unwrap();
    remove_block(&mut world, &mut inv, a).unwrap();
    assert!(
        world.is_occupied(GridCoord::new(4, 0, 2)) && world.is_occupied(GridCoord::new(5, 0, 2))
    );
    assert!(
        !world.is_occupied(GridCoord::new(2, 0, 2)) && !world.is_occupied(GridCoord::new(3, 0, 2))
    );
}

#[test]
fn removal_frees_a_placement_slot() {
    let (r, g, mut world, mut inv, _, mut ecs) = placed_slab_world();
    let cube = BlockId::new("block.cube");
    let limited = rules(json!([{"id": "block.cube", "max_count": 1}]));
    let (a, b) = (ecs.spawn_empty().id(), ecs.spawn_empty().id());
    attempt_rot(
        &r,
        &g,
        &mut world,
        &limited,
        &mut inv,
        &cube,
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
        a,
    )
    .unwrap();
    assert!(matches!(
        attempt_rot(
            &r,
            &g,
            &mut world,
            &limited,
            &mut inv,
            &cube,
            GridCoord::new(3, 0, 3),
            GridRotation::Deg0,
            b
        ),
        Err(PlacementRejection::LimitReached { .. })
    ));
    remove_block(&mut world, &mut inv, a).unwrap();
    attempt_rot(
        &r,
        &g,
        &mut world,
        &limited,
        &mut inv,
        &cube,
        GridCoord::new(3, 0, 3),
        GridRotation::Deg0,
        b,
    )
    .unwrap();
    assert_eq!(world.count_of(&cube), 1);
}

#[test]
fn refund_happens_exactly_once_and_non_blocks_cannot_be_removed() {
    let (r, g, mut world, mut inv, rules, mut ecs) = placed_slab_world();
    let cube = BlockId::new("block.cube");
    let (e, preview) = (ecs.spawn_empty().id(), ecs.spawn_empty().id());
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &cube,
        GridCoord::new(1, 0, 1),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    remove_block(&mut world, &mut inv, e).unwrap();
    let after = inv.clone();
    // Repeating, or targeting an entity that is not a placed block (preview, ground...).
    assert_eq!(
        remove_block(&mut world, &mut inv, e),
        Err(RemovalError::NotPlaced)
    );
    assert_eq!(
        remove_block(&mut world, &mut inv, preview),
        Err(RemovalError::NotPlaced)
    );
    assert_eq!(inv, after);
    assert!(world.is_empty());
}

#[test]
fn a_refund_that_cannot_be_stored_removes_nothing() {
    let (r, g, mut world, _, rules, mut ecs) = placed_slab_world();
    let cube = BlockId::new("block.cube");
    let mut inv = Inventory::default();
    inv.add(ItemRef::Block(cube.clone()), u32::MAX).unwrap();
    let e = ecs.spawn_empty().id();
    attempt_rot(
        &r,
        &g,
        &mut world,
        &rules,
        &mut inv,
        &cube,
        GridCoord::new(1, 0, 1),
        GridRotation::Deg0,
        e,
    )
    .unwrap();
    assert_eq!(inv.quantity(&ItemRef::Block(cube.clone())), u32::MAX - 1);
    inv.add(ItemRef::Block(cube.clone()), 1).unwrap(); // now full again
    assert_eq!(
        remove_block(&mut world, &mut inv, e),
        Err(RemovalError::RefundFailed)
    );
    assert!(world.is_occupied(GridCoord::new(1, 0, 1)));
    assert_eq!(world.len(), 1);
}

#[test]
fn hammer_picking_respects_occlusion_and_ignores_the_floor() {
    let g = stage_grid(); // origin (10, 0, 20)
    let near = GridCoord::new(2, 0, 3);
    let far = GridCoord::new(2, 0, 6);
    // Looking along +Z at mid height: both cells are in line, the nearer wins.
    let origin = Vec3::new(12.5, 0.5, 18.0);
    let hit = ray_pick_occupied(origin, Vec3::Z, &g, [far, near].into_iter());
    assert_eq!(hit, Some(near));
    // Straight down at an empty cell: the floor is not pickable.
    assert_eq!(
        ray_pick_occupied(
            Vec3::new(14.5, 5.0, 23.5),
            Vec3::NEG_Y,
            &g,
            [near].into_iter()
        ),
        None
    );
    // Nothing placed: nothing to pick.
    assert_eq!(
        ray_pick_occupied(origin, Vec3::Z, &g, std::iter::empty()),
        None
    );
}

#[test]
fn a_click_is_either_placement_or_hammer_never_both() {
    let mut tools = ToolRegistry::default();
    register_builtin_tools(&mut tools).unwrap();
    let block = ItemRef::Block(wood_log_id());
    let hammer = ItemRef::Tool(hammer_id());
    let unknown = ItemRef::Tool(crate::items::tools::ToolId::new("laser"));
    assert_eq!(click_action(Some(&block), &tools), ClickAction::PlaceBlock);
    assert_eq!(
        click_action(None, &tools),
        ClickAction::PlaceBlock,
        "placement reports 'nothing selected'"
    );
    assert_eq!(click_action(Some(&hammer), &tools), ClickAction::UseHammer);
    assert_eq!(click_action(Some(&unknown), &tools), ClickAction::Nothing);
}
