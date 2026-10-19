use serde_json::json;

use super::*;
use crate::blocks::definition::{BlockDimensions, BlockEconomyConfig, BlockModel};
use crate::blocks::grid::{GridBounds, GridCoord, GridRotation, GridSpec};
use crate::blocks::models::{register_builtin_blocks, wood_log_id};
use crate::blocks::{BlockDefinition, PlacedBlock};
use crate::items::ItemRef;
use crate::items::inventory::Inventory;

fn registry() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    register_builtin_blocks(&mut r).unwrap();
    for (id, footprint) in [
        ("block.stone", UVec3::ONE),
        ("block.slab", UVec3::new(2, 1, 1)),
    ] {
        r.register(BlockDefinition::new(
            BlockId::new(id),
            id,
            BlockModel::glb("x.glb"),
            BlockDimensions::new(footprint, footprint.as_vec3()),
            BlockEconomyConfig::with_default_price(1),
        ))
        .unwrap();
    }
    r
}

fn rules() -> StagePlacementRules {
    StagePlacementRules::from_construction(&json!({"allowed_blocks": [
        {"id": "block.wood_log"}, {"id": "block.stone"}, {"id": "block.slab"}
    ]}))
}

fn no_titles(_: &str) -> Option<String> {
    None
}

fn parse(objectives: serde_json::Value) -> Result<Vec<Objective>, ObjectiveError> {
    StageObjectives::parse(&objectives, &registry(), &rules(), &no_titles)
}

fn logs(count: u64) -> serde_json::Value {
    json!({"tasks": [{"id": "t_logs", "type": "construction",
        "data": {"kind": "place_block", "block": "block.wood_log", "count": count}}]})
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

fn place(
    world: &mut PlacedWorld,
    ecs: &mut World,
    id: &BlockId,
    cell: GridCoord,
    rot: GridRotation,
) -> Entity {
    let r = registry();
    let g = grid();
    let e = ecs.spawn_empty().id();
    let placed = PlacedBlock::new(&r, &g, id.clone(), cell, rot).unwrap();
    world.insert(&placed, r.require(id).unwrap(), e).unwrap();
    e
}

// ------------------------------------------------------------ parsing

#[test]
fn a_valid_block_count_objective_parses() {
    let list = parse(logs(12)).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "t_logs");
    assert_eq!(
        list[0].title, "Wood Log",
        "defaults to the block's display name"
    );
    assert_eq!(
        list[0].kind,
        ObjectiveKind::PlaceBlock {
            block: wood_log_id(),
            count: 12
        }
    );
}

#[test]
fn titles_use_the_localization_key_when_translated() {
    let doc = json!({"tasks": [{"id": "a", "type": "construction",
        "data": {"kind": "place_block", "block": "block.wood_log", "count": 2, "title": "k.title"}}]});
    let translated = |k: &str| (k == "k.title").then(|| "Build the frame".to_string());
    let list = StageObjectives::parse(&doc, &registry(), &rules(), &translated).unwrap();
    assert_eq!(list[0].title, "Build the frame");
    let untranslated = StageObjectives::parse(&doc, &registry(), &rules(), &no_titles).unwrap();
    assert_eq!(untranslated[0].title, "Wood Log");
}

#[test]
fn malformed_objectives_are_rejected_with_useful_errors() {
    let task = |data: serde_json::Value| json!({"tasks": [{"id": "t", "type": "construction", "data": data}]});
    assert!(matches!(
        parse(logs(0)),
        Err(ObjectiveError::InvalidField { .. })
    ));
    assert!(matches!(
        parse(task(
            json!({"kind": "place_block", "block": "block.wood_log", "count": -2})
        )),
        Err(ObjectiveError::InvalidField { .. })
    ));
    assert!(matches!(
        parse(task(
            json!({"kind": "place_block", "block": "block.wood_log"})
        )),
        Err(ObjectiveError::InvalidField { .. })
    ));
    assert!(matches!(
        parse(task(json!({"kind": "place_block", "count": 1}))),
        Err(ObjectiveError::InvalidField { .. })
    ));
    assert!(matches!(
        parse(task(
            json!({"kind": "stack_height", "block": "block.wood_log", "count": 1})
        )),
        Err(ObjectiveError::UnknownKind { .. })
    ));
    assert!(matches!(
        parse(task(
            json!({"kind": "place_block", "block": "brick", "count": 1})
        )),
        Err(ObjectiveError::UnknownBlock { .. })
    ));
    let err = parse(task(
        json!({"kind": "place_block", "block": "block.wood_log", "count": 1}),
    ))
    .map(|_| ());
    assert_eq!(err, Ok(()));
}

#[test]
fn blocks_must_be_allowed_by_the_stage() {
    let strict =
        StagePlacementRules::from_construction(&json!({"allowed_blocks": [{"id": "block.stone"}]}));
    let err = StageObjectives::parse(&logs(3), &registry(), &strict, &no_titles).unwrap_err();
    assert!(matches!(err, ObjectiveError::NotAllowed { .. }));
}

#[test]
fn duplicate_objective_ids_are_rejected() {
    let doc = json!({"tasks": [
        {"id": "t", "type": "construction", "data": {"kind": "place_block", "block": "block.wood_log", "count": 1}},
        {"id": "t", "type": "construction", "data": {"kind": "place_block", "block": "block.stone", "count": 1}},
    ]});
    assert_eq!(parse(doc), Err(ObjectiveError::DuplicateId("t".into())));
}

#[test]
fn legacy_and_unrelated_tasks_load_without_objectives() {
    // No objectives key, empty tasks, and stage-001 style math / region-target tasks.
    for doc in [
        json!({}),
        json!({"tasks": []}),
        json!({"tasks": [
            {"id": "t_math", "type": "math", "math": {"type": "multiplication"}},
            {"id": "t_build", "type": "construction", "target": "wall_a", "depends_on": ["t_math"]},
        ], "completion": {"all": []}}),
    ] {
        assert_eq!(parse(doc), Ok(Vec::new()));
    }
    let from = StageObjectives::from_stage(
        &json!({"tasks": [{"id": "x", "type": "construction", "data": {"kind": "bogus"}}]}),
        &registry(),
        &rules(),
        &no_titles,
    );
    assert!(
        from.objectives.is_empty() && from.error.is_some(),
        "a malformed list is reported, not applied"
    );
}

// --------------------------------------------------------- evaluation

fn tracked(count: u64) -> (Vec<Objective>, ObjectiveTracker) {
    let list = parse(logs(count)).unwrap();
    let mut tracker = ObjectiveTracker::default();
    tracker.reset(list.len());
    (list, tracker)
}

#[test]
fn placing_matching_blocks_raises_progress_and_others_do_not() {
    let (list, mut tracker) = tracked(3);
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    evaluate(&list, &world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 0);
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    place(
        &mut world,
        &mut ecs,
        &BlockId::new("block.stone"),
        GridCoord::new(5, 0, 5),
        GridRotation::Deg0,
    );
    evaluate(&list, &world, &mut tracker);
    assert_eq!(
        tracker.states()[0].progress,
        1,
        "the stone block does not count"
    );
}

#[test]
fn rotated_multi_cell_blocks_count_once() {
    let doc = json!({"tasks": [{"id": "s", "type": "construction",
        "data": {"kind": "place_block", "block": "block.slab", "count": 5}}]});
    let list = parse(doc).unwrap();
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    let mut tracker = ObjectiveTracker::default();
    place(
        &mut world,
        &mut ecs,
        &BlockId::new("block.slab"),
        GridCoord::new(6, 0, 6),
        GridRotation::Deg90,
    );
    evaluate(&list, &world, &mut tracker);
    assert_eq!(world.occupied_cells().count(), 2);
    assert_eq!(
        tracker.states()[0].progress,
        1,
        "one instance, not two cells"
    );
}

#[test]
fn inventory_and_purchases_are_not_construction() {
    let (list, mut tracker) = tracked(2);
    let world = PlacedWorld::default();
    let mut inventory = Inventory::default();
    inventory.add(ItemRef::Block(wood_log_id()), 50).unwrap();
    evaluate(&list, &world, &mut tracker);
    assert_eq!(tracker.states()[0], ObjectiveState::default());
    assert!(!tracker.all_complete());
}

#[test]
fn removal_lowers_progress_before_completion() {
    let (list, mut tracker) = tracked(3);
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    let a = place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(2, 0, 0),
        GridRotation::Deg0,
    );
    evaluate(&list, &world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 2);
    world.remove(a);
    evaluate(&list, &world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 1);
    assert!(!tracker.states()[0].completed);
}

#[test]
fn completion_is_latched_and_reported_once() {
    let (list, mut tracker) = tracked(2);
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    let a = place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(2, 0, 0),
        GridRotation::Deg0,
    );
    assert_eq!(evaluate(&list, &world, &mut tracker), vec![0]);
    assert!(tracker.states()[0].completed);
    // Re-evaluating (every world change) never re-reports it.
    assert!(evaluate(&list, &world, &mut tracker).is_empty());
    // Removing a block afterwards keeps the objective complete at count / count.
    world.remove(a);
    assert!(evaluate(&list, &world, &mut tracker).is_empty());
    assert!(tracker.states()[0].completed);
    assert_eq!(tracker.states()[0].progress, 2);
    assert_eq!(
        list[0].label(&tracker.states()[0]),
        "Wood Log: 2 / 2  (done)"
    );
}

#[test]
fn progress_never_exceeds_the_requirement() {
    let (list, mut tracker) = tracked(1);
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    for x in 0..3 {
        place(
            &mut world,
            &mut ecs,
            &wood_log_id(),
            GridCoord::new(x * 2, 0, 0),
            GridRotation::Deg0,
        );
    }
    evaluate(&list, &world, &mut tracker);
    assert_eq!(tracker.states()[0].progress, 1);
}

#[test]
fn aggregate_completion_needs_every_objective_and_at_least_one() {
    let empty = ObjectiveTracker::default();
    assert!(
        !empty.all_complete(),
        "no objectives is not a finished mission"
    );
    let doc = json!({"tasks": [
        {"id": "a", "type": "construction", "data": {"kind": "place_block", "block": "block.wood_log", "count": 1}},
        {"id": "b", "type": "construction", "data": {"kind": "place_block", "block": "block.stone", "count": 1}},
    ]});
    let list = parse(doc).unwrap();
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    let mut tracker = ObjectiveTracker::default();
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    evaluate(&list, &world, &mut tracker);
    assert!(!tracker.all_complete());
    place(
        &mut world,
        &mut ecs,
        &BlockId::new("block.stone"),
        GridCoord::new(4, 0, 4),
        GridRotation::Deg0,
    );
    evaluate(&list, &world, &mut tracker);
    assert!(tracker.all_complete());
}

#[test]
fn a_new_stage_starts_clean() {
    let (list, mut tracker) = tracked(1);
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    evaluate(&list, &world, &mut tracker);
    assert!(tracker.all_complete());
    // Entering another stage: the world is cleared and the tracker reset.
    world.clear();
    let next = parse(logs(4)).unwrap();
    tracker.reset(next.len());
    evaluate(&next, &world, &mut tracker);
    assert_eq!(tracker.states(), [ObjectiveState::default()]);
}

// ------------------------------------------- challenge prerequisites

use crate::challenges::answer::AnswerSpec;
use crate::challenges::{Challenge, ChallengeTracker, StageChallenges};

fn gated(count: u64) -> serde_json::Value {
    json!({"tasks": [{"id": "t_logs", "type": "construction",
        "data": {"kind": "place_block", "block": "block.wood_log", "count": count, "requires_challenge": "t_math"}}]})
}

fn challenges() -> StageChallenges {
    StageChallenges {
        challenges: vec![Challenge {
            id: "t_math".into(),
            prompt: "?".into(),
            answer: AnswerSpec::Integer(1),
        }],
        error: None,
    }
}

#[test]
fn a_prerequisite_is_parsed_and_unchanged_objectives_have_none() {
    assert_eq!(
        parse(gated(1)).unwrap()[0].requires_challenge.as_deref(),
        Some("t_math")
    );
    assert_eq!(parse(logs(1)).unwrap()[0].requires_challenge, None);
    let bad = json!({"tasks": [{"id": "t", "type": "construction",
        "data": {"kind": "place_block", "block": "block.wood_log", "count": 1, "requires_challenge": 7}}]});
    assert!(matches!(
        parse(bad),
        Err(ObjectiveError::InvalidField { .. })
    ));
}

#[test]
fn missing_challenge_references_are_reported_not_ignored() {
    let ok = StageObjectives::from_stage(&gated(1), &registry(), &rules(), &no_titles);
    let c = challenges();
    let checked = ok
        .clone()
        .with_checked_challenge_refs(&|id| c.find(id).is_some());
    assert_eq!(checked, ok);
    let broken = ok.with_checked_challenge_refs(&|_| false);
    assert!(broken.objectives.is_empty());
    assert!(
        broken
            .error
            .unwrap()
            .contains("challenge 't_math' does not exist")
    );
    // Objectives without a prerequisite never need a challenge.
    let plain = StageObjectives::from_stage(&logs(1), &registry(), &rules(), &no_titles)
        .with_checked_challenge_refs(&|_| false);
    assert_eq!(plain.objectives.len(), 1);
}

#[test]
fn both_the_construction_and_the_challenge_are_required() {
    let list = parse(gated(2)).unwrap();
    let (mut world, mut ecs) = (PlacedWorld::default(), World::new());
    let mut tracker = ObjectiveTracker::default();
    let solved = &|_: &str| true;
    let unsolved = &|_: &str| false;

    // Challenge solved but nothing built.
    assert!(evaluate_with(&list, &world, solved, &mut tracker).is_empty());
    assert!(!tracker.states()[0].completed && !tracker.states()[0].awaiting_challenge);

    // Built but challenge unsolved.
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    place(
        &mut world,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(2, 0, 0),
        GridRotation::Deg0,
    );
    assert!(evaluate_with(&list, &world, unsolved, &mut tracker).is_empty());
    assert!(tracker.states()[0].awaiting_challenge && !tracker.all_complete());
    assert!(
        list[0]
            .label(&tracker.states()[0])
            .contains("solve the math challenge")
    );

    // Both satisfied: completes once and stays latched.
    assert_eq!(evaluate_with(&list, &world, solved, &mut tracker), vec![0]);
    assert!(tracker.all_complete() && !tracker.states()[0].awaiting_challenge);
    assert!(evaluate_with(&list, &world, solved, &mut tracker).is_empty());
}

#[test]
fn solving_a_challenge_never_completes_an_unbuilt_objective_and_building_never_solves_it() {
    let list = parse(gated(1)).unwrap();
    let world = PlacedWorld::default();
    let mut tracker = ObjectiveTracker::default();
    evaluate_with(&list, &world, &|_| true, &mut tracker);
    assert!(!tracker.states()[0].completed);
    // `evaluate` has no challenge source: a prerequisite then counts as unsolved.
    let (mut w2, mut ecs) = (PlacedWorld::default(), World::new());
    place(
        &mut w2,
        &mut ecs,
        &wood_log_id(),
        GridCoord::new(0, 0, 0),
        GridRotation::Deg0,
    );
    let mut t2 = ObjectiveTracker::default();
    evaluate(&list, &w2, &mut t2);
    assert!(!t2.states()[0].completed);
    let solved = ChallengeTracker::default();
    assert!(!solved.is_solved(&challenges(), "t_math"));
}
