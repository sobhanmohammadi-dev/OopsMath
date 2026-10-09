//! Tests for the WRLD decoder (`dat::world`) and the `VoxelWorld` model.

use crate::stage::dat::error::WorldError;
use crate::stage::dat::world::{WORLD_CHUNK_VOLUME, WORLD_HEADER_SIZE, parse_world};

use super::support::build_world;

const DENSE: u8 = 0;
const RLE: u8 = 1;

fn dense_narrow(cells: &[(usize, u8)]) -> Vec<u8> {
    let mut payload = vec![0u8; WORLD_CHUNK_VOLUME];
    for (cell, value) in cells {
        payload[*cell] = *value;
    }
    payload
}

fn dense_wide(cells: &[(usize, u16)]) -> Vec<u8> {
    let mut payload = vec![0u8; WORLD_CHUNK_VOLUME * 2];
    for (cell, value) in cells {
        payload[cell * 2..cell * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    payload
}

/// Narrow RLE payload from `(run, index)` pairs.
fn rle_narrow(runs: &[(u8, u8)]) -> Vec<u8> {
    runs.iter().flat_map(|(run, value)| [*run, *value]).collect()
}

/// Wide RLE payload from `(run, index)` pairs.
fn rle_wide(runs: &[(u16, u16)]) -> Vec<u8> {
    runs.iter()
        .flat_map(|(run, value)| {
            let mut pair = run.to_le_bytes().to_vec();
            pair.extend_from_slice(&value.to_le_bytes());
            pair
        })
        .collect()
}

/// A palette of `count` entries: "air", then "block_1", "block_2", ...
fn numbered_palette(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| if i == 0 { "air".into() } else { format!("block_{i}") })
        .collect()
}

#[test]
fn decodes_dense_narrow_chunk() {
    let data = build_world(
        [16, 16, 16],
        &["air", "brick"],
        vec![(0, 0, 0, DENSE, dense_narrow(&[(5, 1)]))],
    );
    let world = parse_world(&data).unwrap();
    assert_eq!(world.size, [16, 16, 16]);
    assert_eq!(world.palette, vec!["air", "brick"]);
    assert_eq!(world.solid_count(), 1);
    assert_eq!(world.block_index([5, 0, 0]), Some(1));
    assert_eq!(world.block_name([5, 0, 0]), Some("brick"));
    assert_eq!(world.block_name([4, 0, 0]), None);
}

#[test]
fn decodes_cell_order_x_then_y_then_z() {
    // cell = z * 256 + y * 16 + x
    let cell = 3 * 256 + 2 * 16 + 1;
    let data = build_world(
        [16, 16, 16],
        &["air", "brick"],
        vec![(0, 0, 0, DENSE, dense_narrow(&[(cell, 1)]))],
    );
    let world = parse_world(&data).unwrap();
    assert_eq!(world.block_index([1, 2, 3]), Some(1));
    assert_eq!(world.solid_count(), 1);
}

#[test]
fn offsets_voxels_by_chunk_coordinates() {
    let data = build_world(
        [32, 16, 16],
        &["air", "brick"],
        vec![(1, 0, 0, DENSE, dense_narrow(&[(0, 1)]))],
    );
    let world = parse_world(&data).unwrap();
    assert_eq!(world.block_index([16, 0, 0]), Some(1));
    assert_eq!(world.block_index([0, 0, 0]), None);
}

#[test]
fn decodes_rle_narrow_chunk() {
    // 5 air cells, then 4091 cells of block 1.
    let mut runs = rle_narrow(&[(5, 0), (255, 1), (255, 1), (255, 1), (255, 1), (255, 1)]);
    // 5 + 5 * 255 cells so far; add runs until all 4096 cells are covered.
    let mut covered = 5 + 5 * 255;
    while covered < WORLD_CHUNK_VOLUME {
        let run = (WORLD_CHUNK_VOLUME - covered).min(255);
        runs.extend_from_slice(&[run as u8, 1]);
        covered += run;
    }
    let data = build_world([16, 16, 16], &["air", "brick"], vec![(0, 0, 0, RLE, runs)]);
    let world = parse_world(&data).unwrap();
    assert_eq!(world.block_index([0, 0, 0]), None);
    assert_eq!(world.block_index([5, 0, 0]), Some(1));
    assert_eq!(world.block_index([15, 15, 15]), Some(1));
    assert_eq!(world.solid_count(), WORLD_CHUNK_VOLUME - 5);
}

#[test]
fn decodes_dense_wide_chunk() {
    let palette = numbered_palette(300);
    let names: Vec<&str> = palette.iter().map(String::as_str).collect();
    let data = build_world(
        [16, 16, 16],
        &names,
        vec![(0, 0, 0, DENSE, dense_wide(&[(0, 299), (1, 257)]))],
    );
    let world = parse_world(&data).unwrap();
    assert_eq!(world.block_index([0, 0, 0]), Some(299));
    assert_eq!(world.block_name([0, 0, 0]), Some("block_299"));
    assert_eq!(world.block_name([1, 0, 0]), Some("block_257"));
}

#[test]
fn decodes_rle_wide_chunk() {
    let palette = numbered_palette(300);
    let names: Vec<&str> = palette.iter().map(String::as_str).collect();
    let runs = rle_wide(&[(100, 0), (3996, 257)]);
    let data = build_world([16, 16, 16], &names, vec![(0, 0, 0, RLE, runs)]);
    let world = parse_world(&data).unwrap();
    assert_eq!(world.block_index([3, 0, 0]), None);
    assert_eq!(world.block_name([4, 6, 0]), Some("block_257"));
    assert_eq!(world.solid_count(), 3996);
}

#[test]
fn drops_cells_outside_the_declared_world() {
    // Only x = 0 lies inside a 1x1x1 world; cell 5 is chunk padding.
    let data = build_world(
        [1, 1, 1],
        &["air", "brick"],
        vec![(0, 0, 0, DENSE, dense_narrow(&[(0, 1), (5, 1)]))],
    );
    let world = parse_world(&data).unwrap();
    assert_eq!(world.solid_count(), 1);
    assert_eq!(world.block_index([0, 0, 0]), Some(1));
}

#[test]
fn decodes_world_without_chunks() {
    let data = build_world([0, 0, 0], &["air"], vec![]);
    let world = parse_world(&data).unwrap();
    assert_eq!(world.solid_count(), 0);
    assert_eq!(world.palette, vec!["air"]);
}

// -----------------------------------------------------------------------------
// Rejections
// -----------------------------------------------------------------------------

#[test]
fn rejects_bad_magic() {
    let mut data = vec![0u8; WORLD_HEADER_SIZE + 8];
    data[0] = b'X';
    assert_eq!(parse_world(&data), Err(WorldError::InvalidMagic));
}

#[test]
fn rejects_unsupported_version() {
    let mut data = build_world([16, 16, 16], &["air"], vec![]);
    data[4] = 2;
    assert_eq!(parse_world(&data), Err(WorldError::UnsupportedVersion(2)));
}

#[test]
fn rejects_unsupported_chunk_size() {
    let mut data = build_world([16, 16, 16], &["air"], vec![]);
    data[6] = 32;
    assert_eq!(parse_world(&data), Err(WorldError::UnsupportedChunkSize(32)));
}

#[test]
fn rejects_truncated_header() {
    assert_eq!(parse_world(&[0u8; 10]), Err(WorldError::HeaderTooSmall));
}

#[test]
fn rejects_header_cut_inside_chunk_count_field() {
    // The header is 28 bytes; 24 bytes used to slip past the size check.
    let data = build_world([16, 16, 16], &["air"], vec![]);
    assert_eq!(parse_world(&data[..24]), Err(WorldError::HeaderTooSmall));
}

#[test]
fn rejects_axis_too_large() {
    let data = build_world([2_000_000, 1, 1], &["air"], vec![]);
    assert!(matches!(
        parse_world(&data),
        Err(WorldError::WorldAxisTooLarge { .. })
    ));
}

#[test]
fn rejects_palette_too_large() {
    let mut data = build_world([16, 16, 16], &[], vec![]);
    data[20..24].copy_from_slice(&70_000u32.to_le_bytes());
    assert_eq!(
        parse_world(&data),
        Err(WorldError::PaletteTooLarge {
            declared: 70_000,
            limit: 65_535,
        })
    );
}

#[test]
fn rejects_too_many_chunks() {
    let mut data = build_world([16, 16, 16], &["air"], vec![]);
    data[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        parse_world(&data),
        Err(WorldError::ChunkCountTooLarge { .. })
    ));
}

#[test]
fn rejects_truncated_palette() {
    let data = build_world([16, 16, 16], &["air", "brick"], vec![]);
    // Cut in the middle of the second palette name.
    let cut = data.len() - 2;
    assert!(matches!(
        parse_world(&data[..cut]),
        Err(WorldError::PaletteLengthOutOfBounds { index: 1, .. })
    ));
}

#[test]
fn rejects_non_utf8_palette_name() {
    let mut data = build_world([16, 16, 16], &["xx"], vec![]);
    data[WORLD_HEADER_SIZE + 2] = 0xFF;
    data[WORLD_HEADER_SIZE + 3] = 0xFE;
    assert_eq!(
        parse_world(&data),
        Err(WorldError::PaletteNotUtf8 { index: 0 })
    );
}

#[test]
fn rejects_truncated_chunk_record() {
    let data = build_world(
        [16, 16, 16],
        &["air"],
        vec![(0, 0, 0, DENSE, dense_narrow(&[]))],
    );
    let record_start = WORLD_HEADER_SIZE + 2 + 3;
    assert_eq!(
        parse_world(&data[..record_start + 5]),
        Err(WorldError::ChunkRecordTruncated {
            offset: record_start as u64,
        })
    );
}

#[test]
fn rejects_truncated_chunk_payload() {
    let data = build_world(
        [16, 16, 16],
        &["air"],
        vec![(0, 0, 0, DENSE, dense_narrow(&[]))],
    );
    assert!(matches!(
        parse_world(&data[..data.len() - 10]),
        Err(WorldError::ChunkPayloadTruncated { .. })
    ));
}

#[test]
fn rejects_unknown_chunk_encoding() {
    let data = build_world([16, 16, 16], &["air"], vec![(0, 0, 0, 7, vec![0u8; 8])]);
    assert_eq!(
        parse_world(&data),
        Err(WorldError::UnknownChunkEncoding {
            x: 0,
            y: 0,
            z: 0,
            encoding: 7,
        })
    );
}

#[test]
fn rejects_dense_chunk_with_wrong_length() {
    let data = build_world(
        [16, 16, 16],
        &["air"],
        vec![(0, 0, 0, DENSE, vec![0u8; 100])],
    );
    assert_eq!(
        parse_world(&data),
        Err(WorldError::ChunkCellCountMismatch { x: 0, y: 0, z: 0 })
    );
}

#[test]
fn rejects_rle_covering_too_few_cells() {
    let data = build_world(
        [16, 16, 16],
        &["air", "brick"],
        vec![(0, 0, 0, RLE, rle_narrow(&[(5, 0), (10, 1)]))],
    );
    assert_eq!(
        parse_world(&data),
        Err(WorldError::ChunkCellCountMismatch { x: 0, y: 0, z: 0 })
    );
}

#[test]
fn rejects_rle_covering_too_many_cells() {
    let runs: Vec<(u8, u8)> = vec![(255, 1); 17]; // 4335 cells
    let data = build_world(
        [16, 16, 16],
        &["air", "brick"],
        vec![(0, 0, 0, RLE, rle_narrow(&runs))],
    );
    assert_eq!(
        parse_world(&data),
        Err(WorldError::ChunkCellCountMismatch { x: 0, y: 0, z: 0 })
    );
}

#[test]
fn rejects_rle_with_trailing_partial_run() {
    let mut payload = rle_narrow(&[(255, 0)]);
    payload.push(7);
    let data = build_world([16, 16, 16], &["air"], vec![(0, 0, 0, RLE, payload)]);
    assert_eq!(
        parse_world(&data),
        Err(WorldError::MalformedRlePayload {
            x: 0,
            y: 0,
            z: 0,
            len: 3,
        })
    );
}

#[test]
fn rejects_rle_with_too_many_runs() {
    let payload = vec![0u8; (WORLD_CHUNK_VOLUME + 1) * 2];
    let data = build_world([16, 16, 16], &["air"], vec![(0, 0, 0, RLE, payload)]);
    assert!(matches!(
        parse_world(&data),
        Err(WorldError::ChunkRunCountTooLarge { .. })
    ));
}

#[test]
fn rejects_palette_index_outside_palette() {
    let data = build_world(
        [16, 16, 16],
        &["air", "brick"],
        vec![(0, 0, 0, DENSE, dense_narrow(&[(0, 5)]))],
    );
    assert_eq!(
        parse_world(&data),
        Err(WorldError::InvalidPaletteIndex { index: 5, count: 2 })
    );
}

#[test]
fn rejects_duplicate_chunk() {
    let chunk = || (0, 0, 0, DENSE, dense_narrow(&[]));
    let data = build_world([16, 16, 16], &["air"], vec![chunk(), chunk()]);
    assert_eq!(
        parse_world(&data),
        Err(WorldError::DuplicateChunk { x: 0, y: 0, z: 0 })
    );
}

#[test]
fn rejects_chunk_outside_world() {
    let data = build_world(
        [16, 16, 16],
        &["air"],
        vec![(5, 0, 0, DENSE, dense_narrow(&[]))],
    );
    assert_eq!(
        parse_world(&data),
        Err(WorldError::ChunkOutOfBounds {
            x: 5,
            y: 0,
            z: 0,
            dims: [16, 16, 16],
        })
    );
}
