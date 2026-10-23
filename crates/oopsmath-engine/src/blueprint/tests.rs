use serde_json::json;

use super::*;
use crate::blocks::definition::{
    AllowedRotations, BlockDimensions, BlockEconomyConfig, BlockModel, GridConfig,
};
use crate::blocks::grid::GridBounds;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockDefinition, PlacedBlock};
use crate::items::ItemRef;
use crate::items::economy::Wallet;
use crate::items::inventory::Inventory;
use crate::objectives::{ObjectiveTracker, StageObjectives, evaluate};

fn def(id: &str, footprint: UVec3, price: u32) -> BlockDefinition {
    BlockDefinition::new(
        BlockId::new(id),
        id,
        BlockModel::glb("x.glb"),
        BlockDimensions::new(footprint, footprint.as_vec3()),
        BlockEconomyConfig::with_default_price(price),
    )
}

fn registry() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    r.register(def("block.cube", UVec3::ONE, 3)).unwrap();
    r.register(def("block.slab", UVec3::new(2, 1, 1), 5))
        .unwrap();
    r.register(def("block.free", UVec3::ONE, 0)).unwrap();
    r.register(def("block.gold", UVec3::ONE, 1_000_000))
        .unwrap();
    r.register(
        def("block.fixed", UVec3::ONE, 1)
            .with_grid(GridConfig::default_block().with_rotations(AllowedRotations::None)),
    )
    .unwrap();
    r
}

fn grid(size: u32) -> GridSpec {
    GridSpec::new(
        Vec3::ZERO,
        Vec3::ONE,
        Some(GridBounds {
            min: IVec3::ZERO,
            size: UVec3::new(size, size, size),
        }),
    )
    .unwrap()
}

fn rules() -> StagePlacementRules {
    StagePlacementRules::from_construction(&json!({"allowed_blocks": [
        {"id": "block.cube", "max_count": 6}, {"id": "block.slab"}, {"id": "block.free"},
        {"id": "block.gold"}, {"id": "block.fixed"}
    ]}))
}

fn no_titles(_: &str) -> Option<String> {
    None
}

fn parse(blueprints: serde_json::Value) -> Result<Vec<Blueprint>, BlueprintError> {
    parse_blueprints(
        &json!({ "blueprints": blueprints }),
        &registry(),
        &grid(16),
        &no_titles,
    )
}

fn one(blocks: serde_json::Value) -> serde_json::Value {
    json!([{"id": "bp", "blocks": blocks}])
}

fn planned(block: &str, cell: (i32, i32, i32), rotation: GridRotation) -> PlannedBlock {
    PlannedBlock {
        block: BlockId::new(block),
        cell: GridCoord::new(cell.0, cell.1, cell.2),
        rotation,
    }
}

fn blueprint(blocks: Vec<PlannedBlock>) -> Blueprint {
    Blueprint {
        id: "bp".into(),
        title: "Plan".into(),
        blocks,
    }
}

fn compute(bp: &Blueprint, inventory: &Inventory) -> Plan {
    Plan::compute(bp, &registry(), &rules(), inventory)
}

// ----------------------------------------------------------- definitions

#[test]
fn a_valid_blueprint_parses_with_defaults_and_titles() {
    let doc = json!([{"id": "bp", "title": "k.title", "blocks": [
        {"block": "block.cube", "position": [0, 0, 0]},
        {"block": "block.slab", "position": [3, 0, 1], "rotation": 90},
    ]}]);
    let translate = |k: &str| (k == "k.title").then(|| "My plan".to_string());
    let list = parse_blueprints(
        &json!({"blueprints": doc}),
        &registry(),
        &grid(16),
        &translate,
    )
    .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "My plan");
    assert_eq!(
        list[0].blocks[0],
        planned("block.cube", (0, 0, 0), GridRotation::Deg0)
    );
    assert_eq!(list[0].blocks[1].rotation, GridRotation::Deg90);
    // Without a title key the id is used; an untranslated key is shown as-is.
    assert_eq!(
        parse(one(json!([{"block": "block.cube", "position": [0, 0, 0]}]))).unwrap()[0].title,
        "bp"
    );
}

#[test]
fn stages_without_blueprints_load_unchanged() {
    for construction in [json!({}), json!({"blueprints": []})] {
        let list = parse_blueprints(&construction, &registry(), &grid(16), &no_titles).unwrap();
        assert!(list.is_empty());
    }
    let state = StageBlueprints::from_stage(&json!({}), &registry(), &grid(16), &no_titles);
    assert!(!state.is_available() && state.error.is_none());
}

#[test]
fn duplicate_ids_and_unknown_blocks_are_rejected() {
    let a = json!({"id": "bp", "blocks": [{"block": "block.cube", "position": [0, 0, 0]}]});
    assert_eq!(
        parse(json!([a.clone(), a])),
        Err(BlueprintError::DuplicateId("bp".into()))
    );
    assert!(matches!(
        parse(one(
            json!([{"block": "block.ghost", "position": [0, 0, 0]}])
        )),
        Err(BlueprintError::UnknownBlock { index: 0, .. })
    ));
}

#[test]
fn malformed_fields_are_rejected() {
    for bad in [
        json!([{"block": "block.cube", "position": [0, 0]}]),
        json!([{"block": "block.cube", "position": [0, 0, "x"]}]),
        json!([{"block": "block.cube", "position": [0.5, 0, 0]}]),
        json!([{"block": "block.cube", "position": [0, 0, 0], "rotation": 45}]),
        json!([{"block": "block.cube", "position": [0, 0, 0], "rotation": "left"}]),
        json!([{"position": [0, 0, 0]}]),
        json!([]),
    ] {
        assert!(
            matches!(parse(one(bad.clone())), Err(BlueprintError::Invalid { .. })),
            "{bad}"
        );
    }
    assert!(matches!(
        parse(json!([{"blocks": []}])),
        Err(BlueprintError::Invalid { .. })
    ));
    assert!(matches!(
        parse_blueprints(
            &json!({"blueprints": "nope"}),
            &registry(),
            &grid(16),
            &no_titles
        ),
        Err(BlueprintError::Invalid { .. })
    ));
}

#[test]
fn existing_placement_rules_apply_to_planned_blocks() {
    // Out of bounds (the 2-cell slab turned 180 degrees extends to x = -1).
    assert!(matches!(
        parse(one(
            json!([{"block": "block.slab", "position": [0, 0, 0], "rotation": 180}])
        )),
        Err(BlueprintError::Placement {
            cause: BlockError::OutOfBounds { .. },
            ..
        })
    ));
    assert!(matches!(
        parse(one(
            json!([{"block": "block.cube", "position": [16, 0, 0]}])
        )),
        Err(BlueprintError::Placement { .. })
    ));
    // A block that cannot rotate cannot be planned rotated.
    assert!(matches!(
        parse(one(
            json!([{"block": "block.fixed", "position": [0, 0, 0], "rotation": 90}])
        )),
        Err(BlueprintError::Placement {
            cause: BlockError::RotationNotAllowed { .. },
            ..
        })
    ));
}

#[test]
fn overlapping_planned_blocks_are_rejected_using_real_footprints() {
    // Same cell twice (a duplicate placement).
    assert!(matches!(
        parse(one(json!([
            {"block": "block.cube", "position": [2, 0, 2]},
            {"block": "block.cube", "position": [2, 0, 2]},
        ]))),
        Err(BlueprintError::Overlap {
            first: 0,
            second: 1,
            ..
        })
    ));
    // Multi-cell: the slab covers (0,0,0) and (1,0,0).
    assert!(matches!(
        parse(one(json!([
            {"block": "block.slab", "position": [0, 0, 0]},
            {"block": "block.cube", "position": [1, 0, 0]},
        ]))),
        Err(BlueprintError::Overlap { .. })
    ));
    // Rotated 90 degrees the same slab covers (0,0,0) and (0,0,-1): no overlap with x = 1,
    // but an overlap with a cube at z = -1... which is out of bounds, so use z = 3 anchor instead.
    let ok = one(json!([
        {"block": "block.slab", "position": [0, 0, 1], "rotation": 90},
        {"block": "block.cube", "position": [1, 0, 1]},
        {"block": "block.cube", "position": [0, 0, 2]},
    ]));
    assert!(
        parse(ok).is_ok(),
        "the rotated slab covers (0,0,1) and (0,0,0)"
    );
    assert!(matches!(
        parse(one(json!([
            {"block": "block.slab", "position": [0, 0, 1], "rotation": 90},
            {"block": "block.cube", "position": [0, 0, 0]},
        ]))),
        Err(BlueprintError::Overlap { .. })
    ));
}

// ------------------------------------------------------------------ plan

#[test]
fn dimensions_use_the_real_rotated_footprints() {
    // Unrotated slab (2 x 1 x 1) at the origin and a cube above its far end.
    let bp = blueprint(vec![
        planned("block.slab", (0, 0, 0), GridRotation::Deg0),
        planned("block.cube", (1, 1, 0), GridRotation::Deg0),
    ]);
    let plan = compute(&bp, &Inventory::default());
    assert_eq!((plan.min, plan.size), (IVec3::ZERO, UVec3::new(2, 2, 1)));
    // Turned 90 degrees the slab runs along z instead: anchor (0,0,1) covers z = 0..1.
    let turned = blueprint(vec![planned("block.slab", (0, 0, 1), GridRotation::Deg90)]);
    let plan = compute(&turned, &Inventory::default());
    assert_eq!((plan.min, plan.size), (IVec3::ZERO, UVec3::new(1, 1, 2)));
    // A plan that does not start at the origin reports its own bounds.
    let shifted = blueprint(vec![
        planned("block.cube", (4, 2, 5), GridRotation::Deg0),
        planned("block.cube", (6, 2, 5), GridRotation::Deg0),
    ]);
    let plan = compute(&shifted, &Inventory::default());
    assert_eq!(
        (plan.min, plan.size),
        (IVec3::new(4, 2, 5), UVec3::new(3, 1, 1))
    );
}

#[test]
fn materials_are_counted_by_block_id_with_registry_names() {
    let bp = blueprint(vec![
        planned("block.slab", (0, 0, 0), GridRotation::Deg0),
        planned("block.cube", (4, 0, 0), GridRotation::Deg0),
        planned("block.cube", (5, 0, 0), GridRotation::Deg0),
    ]);
    let plan = compute(&bp, &Inventory::default());
    assert_eq!(plan.total_blocks, 3);
    let lines: Vec<_> = plan
        .lines
        .iter()
        .map(|l| (l.block.as_str(), l.required))
        .collect();
    assert_eq!(
        lines,
        [("block.cube", 2), ("block.slab", 1)],
        "sorted by id; a multi-cell block counts once"
    );
    assert_eq!(plan.lines[0].name, "block.cube");
}

#[test]
fn cost_comes_from_the_economy_config_with_checked_arithmetic() {
    let bp = blueprint(vec![
        planned("block.cube", (0, 0, 0), GridRotation::Deg0),
        planned("block.cube", (1, 0, 0), GridRotation::Deg0),
        planned("block.slab", (3, 0, 0), GridRotation::Deg0),
    ]);
    let mut inventory = Inventory::default();
    inventory
        .add(ItemRef::Block(BlockId::new("block.cube")), 1)
        .unwrap();
    let plan = compute(&bp, &inventory);
    assert_eq!(plan.total_cost, Some(2 * 3 + 5));
    assert_eq!(plan.missing_cost, Some(3 + 5), "one cube is already owned");
    assert!(plan.issues.is_empty());
    assert!(plan.has_shortage() && plan.is_buildable());
    assert_eq!((plan.lines[0].available, plan.lines[0].missing()), (1, 1));
    // Enough in stock: no shortage, nothing left to buy.
    inventory
        .add(ItemRef::Block(BlockId::new("block.cube")), 5)
        .unwrap();
    inventory
        .add(ItemRef::Block(BlockId::new("block.slab")), 1)
        .unwrap();
    let covered = compute(&bp, &inventory);
    assert!(!covered.has_shortage());
    assert_eq!(covered.missing_cost, Some(0));
    assert_eq!(
        covered.total_cost,
        Some(11),
        "the full cost does not depend on what you own"
    );
}

#[test]
fn blocks_without_a_price_are_reported_not_invented() {
    let bp = blueprint(vec![
        planned("block.cube", (0, 0, 0), GridRotation::Deg0),
        planned("block.free", (1, 0, 0), GridRotation::Deg0),
    ]);
    let plan = compute(&bp, &Inventory::default());
    assert_eq!(plan.total_cost, None);
    assert!(
        plan.issues
            .contains(&PlanIssue::NoPrice(BlockId::new("block.free")))
    );
    assert!(plan.is_buildable(), "a missing price is informational");
    assert_eq!(plan.lines[1].unit_price, None);
    assert_eq!(plan.lines[0].line_cost, Some(3));
}

#[test]
fn cost_overflow_is_reported_explicitly() {
    // 4300 gold blocks at 1,000,000 each = 4.3e9 > u32::MAX.
    let blocks: Vec<_> = (0..4300)
        .map(|i| {
            planned(
                "block.gold",
                (i % 40, (i / 40) % 40, i / 1600),
                GridRotation::Deg0,
            )
        })
        .collect();
    let plan = compute(&blueprint(blocks), &Inventory::default());
    assert_eq!(plan.total_cost, None);
    assert_eq!(plan.lines[0].line_cost, None);
    assert!(plan.issues.contains(&PlanIssue::CostOverflow));
    // Each line fits but the total does not: 2 lines of 2.5e9 would overflow the sum.
    let mut r = registry();
    r.register(def("block.platinum", UVec3::ONE, 1_000_000))
        .unwrap();
    let rules = StagePlacementRules::from_construction(
        &json!({"allowed_blocks": [{"id": "block.gold"}, {"id": "block.platinum"}]}),
    );
    let mut mixed: Vec<_> = (0..2500)
        .map(|i| planned("block.gold", (i % 40, i / 40, 0), GridRotation::Deg0))
        .collect();
    mixed.extend(
        (0..2500).map(|i| planned("block.platinum", (i % 40, i / 40, 1), GridRotation::Deg0)),
    );
    let plan = Plan::compute(&blueprint(mixed), &r, &rules, &Inventory::default());
    assert!(
        plan.lines
            .iter()
            .all(|l| l.line_cost == Some(2_500_000_000))
    );
    assert_eq!(plan.total_cost, None);
    assert!(plan.issues.contains(&PlanIssue::CostOverflow));
}

#[test]
fn stage_restrictions_are_reported() {
    // block.cube is limited to 6 in rules(); block.ghost is unknown to the stage and registry.
    let seven: Vec<_> = (0..7)
        .map(|i| planned("block.cube", (i, 0, 0), GridRotation::Deg0))
        .collect();
    let plan = compute(&blueprint(seven), &Inventory::default());
    assert!(plan.issues.contains(&PlanIssue::ExceedsStageLimit {
        block: BlockId::new("block.cube"),
        required: 7,
        max: 6
    }));
    assert!(!plan.is_buildable());

    let strict =
        StagePlacementRules::from_construction(&json!({"allowed_blocks": [{"id": "block.slab"}]}));
    let bp = blueprint(vec![planned("block.cube", (0, 0, 0), GridRotation::Deg0)]);
    let plan = Plan::compute(&bp, &registry(), &strict, &Inventory::default());
    assert!(
        plan.issues
            .contains(&PlanIssue::NotAllowed(BlockId::new("block.cube")))
    );
    assert!(!plan.is_buildable());
    // A legacy stage with no allowed blocks permits nothing.
    let legacy = StagePlacementRules::from_construction(&json!({}));
    assert!(!Plan::compute(&bp, &registry(), &legacy, &Inventory::default()).is_buildable());
    // An unregistered block is blocking and does not pretend to have a cost.
    let ghost = blueprint(vec![planned("block.ghost", (0, 0, 0), GridRotation::Deg0)]);
    let plan = compute(&ghost, &Inventory::default());
    assert!(
        plan.issues
            .contains(&PlanIssue::Unregistered(BlockId::new("block.ghost")))
    );
    assert_eq!(plan.total_cost, None);
}

#[test]
fn calculating_a_plan_never_changes_inventory_or_money() {
    let bp = blueprint(vec![planned("block.cube", (0, 0, 0), GridRotation::Deg0)]);
    let wallet = Wallet::new(50);
    let mut inventory = Inventory::default();
    inventory
        .add(ItemRef::Block(BlockId::new("block.cube")), 2)
        .unwrap();
    let (w0, i0) = (wallet, inventory.clone());
    for _ in 0..3 {
        compute(&bp, &inventory);
    }
    assert_eq!((wallet, inventory), (w0, i0));
}

#[test]
fn the_same_input_always_gives_the_same_plan() {
    let bp = blueprint(vec![
        planned("block.slab", (0, 0, 0), GridRotation::Deg0),
        planned("block.cube", (4, 0, 0), GridRotation::Deg0),
    ]);
    assert_eq!(
        compute(&bp, &Inventory::default()),
        compute(&bp, &Inventory::default())
    );
}

// ---------------------------------------------------- progress / preview

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

    fn build(&mut self, block: &str, cell: (i32, i32, i32), rotation: GridRotation) -> Entity {
        let e = self.ecs.spawn_empty().id();
        let id = BlockId::new(block);
        let placed = PlacedBlock::new(
            &self.registry,
            &grid(16),
            id.clone(),
            GridCoord::new(cell.0, cell.1, cell.2),
            rotation,
        )
        .unwrap();
        self.world
            .insert(&placed, self.registry.require(&id).unwrap(), e)
            .unwrap();
        e
    }

    fn statuses(&self, bp: &Blueprint) -> Vec<PlanStatus> {
        plan_statuses(bp, &self.registry, &self.world)
    }
}

#[test]
fn progress_matches_id_anchor_and_rotation() {
    let bp = blueprint(vec![
        planned("block.cube", (0, 0, 0), GridRotation::Deg0),
        planned("block.slab", (4, 0, 1), GridRotation::Deg90),
        planned("block.cube", (8, 0, 0), GridRotation::Deg0),
    ]);
    let mut s = Site::new();
    assert_eq!(s.statuses(&bp), vec![PlanStatus::Pending; 3]);
    s.build("block.cube", (0, 0, 0), GridRotation::Deg0);
    s.build("block.slab", (4, 0, 1), GridRotation::Deg90);
    assert_eq!(
        s.statuses(&bp),
        vec![PlanStatus::Built, PlanStatus::Built, PlanStatus::Pending]
    );
    assert_eq!(built_count(&s.statuses(&bp)), 2);
}

#[test]
fn a_different_block_or_orientation_does_not_count_and_obstructs() {
    let bp = blueprint(vec![
        planned("block.cube", (0, 0, 0), GridRotation::Deg0),
        planned("block.slab", (4, 0, 1), GridRotation::Deg90),
    ]);
    let mut s = Site::new();
    s.build("block.slab", (0, 0, 0), GridRotation::Deg0); // other block on the planned cell
    s.build("block.slab", (4, 0, 0), GridRotation::Deg0); // right block, wrong place and orientation
    let statuses = s.statuses(&bp);
    assert_eq!(
        statuses,
        vec![PlanStatus::Obstructed, PlanStatus::Obstructed]
    );
    assert_eq!(built_count(&statuses), 0);
}

#[test]
fn removed_or_collapsed_blocks_stop_counting() {
    let bp = blueprint(vec![planned("block.cube", (3, 1, 3), GridRotation::Deg0)]);
    let mut s = Site::new();
    let base = s.build("block.cube", (3, 0, 3), GridRotation::Deg0);
    let top = s.build("block.cube", (3, 1, 3), GridRotation::Deg0);
    assert_eq!(s.statuses(&bp), vec![PlanStatus::Built]);
    // The support is hammered away and the top block falls one cell: it no longer matches.
    s.world.remove(base);
    assert!(s.world.shift_down(&[top], 1));
    assert_eq!(s.statuses(&bp), vec![PlanStatus::Pending]);
    // Removing a built block un-builds it.
    let mut t = Site::new();
    let e = t.build("block.cube", (3, 1, 3), GridRotation::Deg0);
    assert_eq!(t.statuses(&bp), vec![PlanStatus::Built]);
    t.world.remove(e);
    assert_eq!(t.statuses(&bp), vec![PlanStatus::Pending]);
}

#[test]
fn the_preview_is_not_part_of_the_world() {
    // A guide only reads the world: drawing it needs no entities, so occupancy,
    // placement counts and objective progress cannot be affected.
    let bp = blueprint(vec![
        planned("block.cube", (0, 0, 0), GridRotation::Deg0),
        planned("block.cube", (1, 0, 0), GridRotation::Deg0),
    ]);
    let s = Site::new();
    for _ in 0..3 {
        s.statuses(&bp);
        compute(&bp, &Inventory::default());
    }
    assert!(s.world.is_empty());
    assert_eq!(s.world.count_of(&BlockId::new("block.cube")), 0);
    let doc = json!({"tasks": [{"id": "t", "type": "construction",
        "data": {"kind": "place_block", "block": "block.cube", "count": 1}}]});
    let rules =
        StagePlacementRules::from_construction(&json!({"allowed_blocks": [{"id": "block.cube"}]}));
    let list = StageObjectives::parse(&doc, &s.registry, &rules, &|_| None).unwrap();
    let mut tracker = ObjectiveTracker::default();
    evaluate(&list, &s.world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 0);
}

// ------------------------------------------------------- mode and lifecycle

#[test]
fn the_planning_mode_only_changes_its_own_state() {
    let mut mode = BlueprintMode::default();
    mode.open();
    assert!(mode.panel_open && !mode.guide);
    mode.step(1, 3);
    mode.step(1, 3);
    mode.step(1, 3);
    assert_eq!(mode.selected, 0, "wraps");
    mode.step(-1, 3);
    assert_eq!(mode.selected, 2);
    mode.step(1, 0);
    assert_eq!(mode.selected, 2, "no blueprints, no change");
    mode.start_building();
    assert!(
        mode.guide && !mode.panel_open,
        "starting the guide returns to the game"
    );
    mode.stop_guide();
    assert!(!mode.guide);
}

#[test]
fn switching_stages_replaces_blueprints_and_clears_the_preview() {
    let first = StageBlueprints::from_stage(
        &json!({"blueprints": [{"id": "a", "blocks": [{"block": "block.cube", "position": [0, 0, 0]}]}]}),
        &registry(),
        &grid(16),
        &no_titles,
    );
    assert_eq!(first.blueprints.len(), 1);
    let before = (
        BlueprintMode {
            panel_open: true,
            selected: 0,
            guide: true,
        },
        BlueprintProgress {
            statuses: vec![PlanStatus::Built],
        },
    );
    assert_ne!(
        before,
        (BlueprintMode::default(), BlueprintProgress::default())
    );
    // What entering or leaving a stage does: new definitions, default mode, empty progress.
    let second = StageBlueprints::from_stage(&json!({}), &registry(), &grid(16), &no_titles);
    assert!(second.blueprints.is_empty());
    assert_eq!(
        (BlueprintMode::default(), BlueprintProgress::default()),
        (
            BlueprintMode {
                panel_open: false,
                selected: 0,
                guide: false
            },
            BlueprintProgress { statuses: vec![] }
        )
    );
}

#[test]
fn invalid_blueprints_disable_the_list_with_a_diagnostic() {
    let state = StageBlueprints::from_stage(
        &json!({"blueprints": [{"id": "a", "blocks": [{"block": "block.ghost", "position": [0, 0, 0]}]}]}),
        &registry(),
        &grid(16),
        &no_titles,
    );
    assert!(state.blueprints.is_empty());
    assert!(state.error.as_deref().unwrap().contains("block.ghost"));
    assert!(
        state.is_available(),
        "the button stays so the problem can be seen"
    );
}
