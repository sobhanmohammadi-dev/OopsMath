//! Headless integration tests for the Blocks plugin (no window or GPU needed).

use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;
use oopsmath_engine::blocks::models::wood_log_id;
use oopsmath_engine::blocks::{
    BlockAssetError, BlockAssets, BlockError, BlockId, BlockRegistry, BlockSpawnError,
    BlocksPlugin, GridCoord, GridRotation, GridSpec, PlacedBlock, spawn_block,
};

#[derive(Resource, Default)]
struct Seen(bool);

#[test]
fn registry_exists_before_startup_systems_run() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, BlocksPlugin))
        .init_resource::<Seen>()
        .add_systems(
            Startup,
            |registry: Res<BlockRegistry>, mut seen: ResMut<Seen>| {
                let log = registry
                    .require(&wood_log_id())
                    .expect("wood log registered");
                assert_eq!(log.model.path, "log.glb");
                seen.0 = true;
            },
        );
    app.update();
    assert!(app.world().resource::<Seen>().0);
}

#[derive(Resource, Default)]
struct Results(Vec<Result<Entity, BlockSpawnError>>);

#[test]
fn spawn_path_validates_and_reports_failures() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, BlocksPlugin))
        .init_resource::<Results>();

    // Hand-built asset table: one ready model, one still loading.
    let ready = wood_log_id();
    let mut assets = BlockAssets::default();
    assets.insert(
        ready.clone(),
        "log.glb".into(),
        Handle::<WorldAsset>::default(),
    );
    assets.mark_ready(&ready);
    let pending = BlockId::new("block.pending");
    assets.insert(
        pending.clone(),
        "p.glb".into(),
        Handle::<WorldAsset>::default(),
    );
    app.insert_resource(assets);

    app.add_systems(
        Startup,
        move |mut commands: Commands,
              registry: Res<BlockRegistry>,
              assets: Res<BlockAssets>,
              mut out: ResMut<Results>| {
            let grid = GridSpec::default();
            let mut go = |id: &BlockId| {
                spawn_block(
                    &mut commands,
                    &registry,
                    &assets,
                    &grid,
                    id.clone(),
                    GridCoord::new(1, 0, 1),
                    GridRotation::Deg0,
                )
            };
            out.0.push(go(&wood_log_id()));
            out.0.push(go(&BlockId::new("block.missing")));
            out.0.push(go(&pending)); // not registered either: block error wins
        },
    );
    app.update();

    let results = &app.world().resource::<Results>().0;
    assert!(results[0].is_ok());
    assert!(matches!(
        results[1],
        Err(BlockSpawnError::Block(BlockError::NotRegistered(_)))
    ));
    assert!(matches!(
        results[2],
        Err(BlockSpawnError::Block(BlockError::NotRegistered(_)))
    ));

    let mut q = app.world_mut().query::<&PlacedBlock>();
    let placed: Vec<_> = q.iter(app.world()).collect();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].block, wood_log_id());
    assert_eq!(placed[0].cell, GridCoord::new(1, 0, 1));
}

#[test]
fn unfinished_models_are_reported_as_asset_errors() {
    let mut assets = BlockAssets::default();
    let id = wood_log_id();
    assert_eq!(
        assets.ready_handle(&id).unwrap_err(),
        BlockAssetError::NotRequested(id.clone())
    );
    assets.insert(
        id.clone(),
        "log.glb".into(),
        Handle::<WorldAsset>::default(),
    );
    assert!(matches!(
        assets.ready_handle(&id),
        Err(BlockAssetError::Pending { .. })
    ));
}

#[test]
fn stage_change_reset_clears_occupancy_target_and_feedback() {
    use oopsmath_engine::blocks::interaction::reset_placement;
    use oopsmath_engine::blocks::{PlacedWorld, PlacementFeedback, PlacementTarget};

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, BlocksPlugin))
        .init_resource::<PlacedWorld>()
        .init_resource::<PlacementTarget>()
        .init_resource::<PlacementFeedback>();

    // Simulate leftovers from a previous stage.
    let registry_ok = {
        let registry = app.world().resource::<BlockRegistry>();
        let grid = GridSpec::default();
        let placed = PlacedBlock::new(
            registry,
            &grid,
            wood_log_id(),
            GridCoord::new(1, 0, 1),
            GridRotation::Deg0,
        )
        .unwrap();
        registry.require(&placed.block).unwrap().clone()
    };
    let entity = app.world_mut().spawn_empty().id();
    let placed = PlacedBlock {
        block: wood_log_id(),
        cell: GridCoord::new(1, 0, 1),
        rotation: GridRotation::Deg0,
    };
    app.world_mut()
        .resource_mut::<PlacedWorld>()
        .insert(&placed, &registry_ok, entity)
        .unwrap();
    app.world_mut().resource_mut::<PlacementFeedback>().0 = Some("old".into());
    app.world_mut().resource_mut::<PlacementTarget>().cell = Some(GridCoord::new(1, 0, 1));

    app.add_systems(Startup, reset_placement);
    app.update();

    assert!(app.world().resource::<PlacedWorld>().is_empty());
    assert_eq!(
        *app.world().resource::<PlacementTarget>(),
        PlacementTarget::default()
    );
    assert_eq!(app.world().resource::<PlacementFeedback>().0, None);
}
