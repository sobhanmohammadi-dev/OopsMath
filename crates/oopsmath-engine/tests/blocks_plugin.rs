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
