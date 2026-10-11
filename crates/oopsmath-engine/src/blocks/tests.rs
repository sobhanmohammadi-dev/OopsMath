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
    assert_eq!(log.dimensions.footprint_cells, UVec3::ONE);
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
            < 0.02,
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
    assert_eq!(t.translation, Vec3::new(1.5, 0.5, 2.5));
    assert_eq!(t.scale, Vec3::splat(0.5));
    assert_eq!(placed.occupied_cells(def).count(), 1);
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
