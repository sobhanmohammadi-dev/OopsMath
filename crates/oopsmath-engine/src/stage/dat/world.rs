//! Decoder for the OopsMath Binary Voxel World v1 (`WRLD` section).
//!
//! This mirrors the encoder in `tools/oopsmath-stage-compiler/oopsmath_stage/world.py`.
//!
//! Layout (little endian):
//!
//! ```text
//! header: "<4sHHIIIII"  (24 bytes)
//!   0  magic[4]          "OWLD"
//!   4  u16 version       1
//!   6  u16 chunk_size    16
//!   8  u32 size_x        world size in voxels per axis
//!  12  u32 size_y
//!  16  u32 size_z
//!  20  u32 palette_count
//!  ... wait: the Python struct is 24 bytes:
//!       magic 4 + 2 + 2 + 4*3 (size) + u32 palette + u32 chunk_count
//! ```
//!
//! Palette: `palette_count` entries of `u16 length + UTF-8 name bytes`.
//! Index 0 is the air block.
//!
//! Chunk records: `"<HHHBI"` (11 bytes) = chunk x, y, z (u16 chunk coordinates),
//! encoding (u8: 0 dense, 1 RLE), payload size (u32). Payload encodes 4096
//! cell values (`u16` indices when the palette has more than 256 entries,
//! otherwise `u8` indices). Dense payload is 4096 raw indices; RLE payload is
//! a sequence of `(run_length, value)` index pairs covering exactly 4096
//! cells. Cell order inside a chunk is x fastest, then y, then z:
//! `cell = (z % 16) * 256 + (y % 16) * 16 + (x % 16)`.
//!
//! The decoder in Python sets `wide = palette_count > 256`; wide chunks use
//! `u16` indices while narrow chunks use `u8` indices.

use std::collections::HashMap;

use crate::stage::dat::error::WorldError;

/// WRLD magic `OWLD`.
pub(crate) const WORLD_MAGIC: [u8; 4] = *b"OWLD";
/// WRLD format version.
pub(crate) const WORLD_VERSION: u16 = 1;
/// Fixed chunk edge length.
pub(crate) const WORLD_CHUNK_SIZE: u16 = 16;
/// Number of cells in a chunk.
pub(crate) const WORLD_CHUNK_VOLUME: usize = (WORLD_CHUNK_SIZE as usize).pow(3);
/// 24-byte WRLD header.
pub(crate) const WORLD_HEADER_SIZE: usize = 24;
/// Maximum palette size (u16 indices are used above 256; u16 max ~65535).
pub(crate) const MAX_PALETTE: u32 = u16::MAX as u32;
/// Sanity limit for chunk records of one world.
pub(crate) const MAX_CHUNKS: u32 = 16_000_000;
/// Maximum supported axis length (from the compiler's MAX_WORLD_AXIS).
pub(crate) const MAX_WORLD_AXIS: u32 = 1_000_000;

/// A decoded voxel world: world size in voxels, block-name palette, and the
/// sparse voxel map excluding air (`(x, y, z) -> palette name`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoxelWorld {
    pub size: [u32; 3],
    pub palette: Vec<String>,
    /// Sparse map; index 0 (air) cells are omitted.
    pub voxels: HashMap<[u32; 3], u32>,
    pub block_names: HashMap<[u32; 3], String>,
}

/// Decodes a WRLD payload.
pub(crate) fn parse_world(data: &[u8]) -> Result<VoxelWorld, WorldError> {
    if data.len() < WORLD_HEADER_SIZE {
        return Err(WorldError::HeaderTooSmall);
    }
    let magic: [u8; 4] = data[0..4].try_into().ok().unwrap_or_default();
    if magic != WORLD_MAGIC {
        return Err(WorldError::InvalidMagic);
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != WORLD_VERSION {
        return Err(WorldError::UnsupportedVersion(version));
    }
    let chunk_size = u16::from_le_bytes([data[6], data[7]]);
    if chunk_size != WORLD_CHUNK_SIZE {
        return Err(WorldError::UnsupportedChunkSize(chunk_size));
    }
    let size = [
        u32::from_le_bytes(data[8..12].try_into().unwrap()),
        u32::from_le_bytes(data[12..16].try_into().unwrap()),
        u32::from_le_bytes(data[16..20].try_into().unwrap()),
    ];
    if size.iter().any(|axis| *axis > MAX_WORLD_AXIS) {
        return Err(WorldError::WorldAxisTooLarge { dims: size });
    }
    let palette_count = u32::from_le_bytes(data[20..24].try_into().unwrap());
    if palette_count > MAX_PALETTE {
        return Err(WorldError::PaletteTooLarge {
            declared: palette_count,
            limit: MAX_PALETTE,
        });
    }
    let chunk_count = u32::from_le_bytes(data[24..28].try_into().unwrap());
    if chunk_count > MAX_CHUNKS {
        return Err(WorldError::ChunkCountTooLarge {
            declared: chunk_count,
            limit: MAX_CHUNKS,
        });
    }

    // Palette.
    let mut pos = 28usize;
    let mut palette = Vec::with_capacity(palette_count.min(4096) as usize);
    for index in 0..palette_count {
        if pos + 2 > data.len() {
            return Err(WorldError::PaletteLengthOutOfBounds { index, length: 0 });
        }
        let length = u16::from_le_bytes([data[pos], data[pos + 1]]) as u64;
        pos += 2;
        let start = pos as u64;
        if start + length > data.len() as u64 {
            return Err(WorldError::PaletteLengthOutOfBounds { index, length });
        }
        let end = pos + length as usize;
        let bytes = &data[pos..end];
        match std::str::from_utf8(bytes) {
            Ok(name) => palette.push(name.to_string()),
            Err(_) => return Err(WorldError::PaletteNotUtf8 { index }),
        }
        pos = end;
    }
    let wide = palette_count > 256;

    // Chunk records.
    let chunk_len = size[0].div_ceil(WORLD_CHUNK_SIZE as u32);
    let chunk_hgt = size[1].div_ceil(WORLD_CHUNK_SIZE as u32);
    let chunk_dpt = size[2].div_ceil(WORLD_CHUNK_SIZE as u32);
    let mut voxels: HashMap<[u32; 3], u32> = HashMap::new();
    let mut seen: HashMap<(u16, u16, u16), ()> = HashMap::new();
    for _record in 0..chunk_count {
        if pos + 11 > data.len() {
            return Err(WorldError::ChunkRecordTruncated { offset: pos as u64 });
        }
        let cx = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let cy = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
        let cz = u16::from_le_bytes([data[pos + 4], data[pos + 5]]);
        let encoding = data[pos + 6];
        let payload_len =
            u32::from_le_bytes([data[pos + 7], data[pos + 8], data[pos + 9], data[pos + 10]])
                as u64;
        // The chunk record "<HHHBI" is 11 bytes wide (2+2+2+1+4).
        pos += 11;
        if seen.insert((cx, cy, cz), ()).is_some() {
            return Err(WorldError::DuplicateChunk {
                x: cx,
                y: cy,
                z: cz,
            });
        }
        if (cx as u32) >= chunk_len || (cy as u32) >= chunk_hgt || (cz as u32) >= chunk_dpt {
            return Err(WorldError::ChunkOutOfBounds {
                x: cx,
                y: cy,
                z: cz,
                dims: size,
            });
        }
        if pos as u64 + payload_len > data.len() as u64 {
            return Err(WorldError::ChunkPayloadTruncated {
                x: cx,
                y: cy,
                z: cz,
                expected: payload_len,
                actual: (data.len() as u64).saturating_sub(pos as u64),
            });
        }
        let payload = &data[pos..pos + payload_len as usize];
        pos += payload_len as usize;

        let index_bit_width = if wide { 2usize } else { 1usize };
        let index_at = |cell: usize| -> Result<u32, WorldError> {
            let start = cell * index_bit_width;
            let value = if wide {
                u16::from_le_bytes([payload[start], payload[start + 1]]) as u32
            } else {
                u32::from(payload[start])
            };
            Ok(value)
        };

        let cells: Result<Vec<u32>, WorldError> = match (encoding, wide) {
            (0, false) => Ok(payload.iter().map(|b| u32::from(*b)).collect()),
            (0, true) => {
                if payload.len() != WORLD_CHUNK_VOLUME * 2 {
                    return Err(WorldError::ChunkCellCountMismatch {
                        x: cx,
                        y: cy,
                        z: cz,
                    });
                }
                (0..WORLD_CHUNK_VOLUME).map(index_at).collect()
            }
            (1, false) => decode_rle_u8(cx, cy, cz, payload),
            (1, true) => decode_rle_u16(cx, cy, cz, payload),
            (_, _) => {
                return Err(WorldError::UnknownChunkEncoding {
                    x: cx,
                    y: cy,
                    z: cz,
                    encoding,
                });
            }
        };
        let cells = cells?;
        if cells.len() != WORLD_CHUNK_VOLUME {
            return Err(WorldError::ChunkCellCountMismatch {
                x: cx,
                y: cy,
                z: cz,
            });
        }
        for (cell, value) in cells.iter().enumerate() {
            if *value == 0 {
                continue; // air
            }
            if *value >= palette_count {
                return Err(WorldError::InvalidPaletteIndex {
                    index: *value as u16,
                    count: palette_count,
                });
            }
            let x = cx as u32 * WORLD_CHUNK_SIZE as u32 + (cell % 16) as u32;
            let y = cy as u32 * WORLD_CHUNK_SIZE as u32 + ((cell / 16) % 16) as u32;
            let z = cz as u32 * WORLD_CHUNK_SIZE as u32 + (cell / 256) as u32;
            if x < size[0] && y < size[1] && z < size[2] {
                voxels.insert([x, y, z], *value);
            }
        }
    }

    let block_names: HashMap<[u32; 3], String> = voxels
        .iter()
        .map(|(k, v)| (*k, palette[*v as usize].clone()))
        .collect();

    Ok(VoxelWorld {
        size,
        palette,
        voxels,
        block_names,
    })
}

fn decode_rle_u8(cx: u16, cy: u16, cz: u16, payload: &[u8]) -> Result<Vec<u32>, WorldError> {
    let max_runs = payload.len() / 2;
    if max_runs > WORLD_CHUNK_VOLUME {
        return Err(WorldError::ChunkRunCountTooLarge {
            x: cx,
            y: cy,
            z: cz,
            runs: max_runs as u64,
            limit: WORLD_CHUNK_VOLUME as u64,
        });
    }
    let mut cells = Vec::with_capacity(WORLD_CHUNK_VOLUME);
    for pair in payload.chunks_exact(2) {
        let run = u32::from(pair[0]);
        let value = u32::from(pair[1]);
        for _ in 0..run {
            cells.push(value);
            if cells.len() > WORLD_CHUNK_VOLUME {
                return Err(WorldError::ChunkCellCountMismatch {
                    x: cx,
                    y: cy,
                    z: cz,
                });
            }
        }
    }
    Ok(cells)
}

fn decode_rle_u16(cx: u16, cy: u16, cz: u16, payload: &[u8]) -> Result<Vec<u32>, WorldError> {
    let max_runs = payload.len() / 4;
    if max_runs > WORLD_CHUNK_VOLUME {
        return Err(WorldError::ChunkRunCountTooLarge {
            x: cx,
            y: cy,
            z: cz,
            runs: max_runs as u64,
            limit: WORLD_CHUNK_VOLUME as u64,
        });
    }
    let mut cells = Vec::with_capacity(WORLD_CHUNK_VOLUME);
    for quad in payload.chunks_exact(4) {
        let run = u16::from_le_bytes([quad[0], quad[1]]) as u32;
        let value = u16::from_le_bytes([quad[2], quad[3]]) as u32;
        for _ in 0..run {
            cells.push(value);
            if cells.len() > WORLD_CHUNK_VOLUME {
                return Err(WorldError::ChunkCellCountMismatch {
                    x: cx,
                    y: cy,
                    z: cz,
                });
            }
        }
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a WRLD payload in the same byte layout the Python encoder emits.
    #[allow(clippy::type_complexity)]
    fn build_world(
        size: [u32; 3],
        palette: &[&str],
        records: Vec<(u16, u16, u16, u8, Vec<u8>)>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"OWLD");
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&16u16.to_le_bytes());
        data.extend_from_slice(&size[0].to_le_bytes());
        data.extend_from_slice(&size[1].to_le_bytes());
        data.extend_from_slice(&size[2].to_le_bytes());
        data.extend_from_slice(&(palette.len() as u32).to_le_bytes());
        data.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for name in palette {
            data.extend_from_slice(&(name.len() as u16).to_le_bytes());
            data.extend_from_slice(name.as_bytes());
        }
        for (cx, cy, cz, encoding, payload) in records {
            data.extend_from_slice(&cx.to_le_bytes());
            data.extend_from_slice(&cy.to_le_bytes());
            data.extend_from_slice(&cz.to_le_bytes());
            data.push(encoding);
            data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            data.extend_from_slice(&payload);
        }
        data
    }

    fn full_rle(run_value_pairs: &[(u8, u8)]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (run, value) in run_value_pairs {
            payload.push(*run);
            payload.push(*value);
        }
        payload
    }

    #[test]
    fn decodes_dense_narrow_chunk() {
        let mut dense = [0u8; WORLD_CHUNK_VOLUME];
        dense[5] = 1;
        let data = build_world(
            [16, 16, 16],
            &["air", "brick"],
            vec![(0, 0, 0, 0, dense.to_vec())],
        );
        let world = parse_world(&data).unwrap();
        assert_eq!(world.size, [16, 16, 16]);
        assert_eq!(world.palette, vec!["air", "brick"]);
        assert_eq!(world.voxels.get(&[5, 0, 0]), Some(&1));
        assert_eq!(
            world.block_names.get(&[5, 0, 0]).map(String::as_str),
            Some("brick")
        );
    }

    #[test]
    fn decodes_rle_narrow_chunk() {
        let mut runs = vec![(5u8, 0u8), (255u8, 1u8)];
        let mut remaining = WORLD_CHUNK_VOLUME - 5 - 255;
        while remaining > 255 {
            runs.push((255u8, 1u8));
            remaining -= 255;
        }
        runs.push((remaining as u8, 1u8));
        let data = build_world(
            [16, 16, 16],
            &["air", "brick"],
            vec![(0, 0, 0, 1, full_rle(&runs))],
        );
        let world = parse_world(&data).unwrap();
        assert_eq!(world.voxels.get(&[0, 0, 0]), None); // air cells 0..4
        assert_eq!(world.voxels.get(&[5, 0, 0]), Some(&1));
        assert_eq!(
            world.voxels.get(&[4095 % 16, (4095 / 16) % 16, 4095 / 256]),
            Some(&1)
        );
    }

    #[test]
    fn rejects_rle_covering_wrong_cell_count() {
        let runs = vec![(5u8, 0u8), (10u8, 1u8)];
        let data = build_world(
            [16, 16, 16],
            &["air", "brick"],
            vec![(0, 0, 0, 1, full_rle(&runs))],
        );
        assert_eq!(
            parse_world(&data),
            Err(WorldError::ChunkCellCountMismatch { x: 0, y: 0, z: 0 })
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = vec![0u8; WORLD_HEADER_SIZE + 8];
        data[0] = b'X';
        assert_eq!(parse_world(&data), Err(WorldError::InvalidMagic));
    }

    #[test]
    fn rejects_truncated_header() {
        let data = vec![0u8; 10];
        assert_eq!(parse_world(&data), Err(WorldError::HeaderTooSmall));
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
    fn rejects_wild_palette_index() {
        let mut dense = [0u8; WORLD_CHUNK_VOLUME];
        dense[0] = 5; // only two palette entries declared
        let data = build_world(
            [16, 16, 16],
            &["air", "brick"],
            vec![(0, 0, 0, 0, dense.to_vec())],
        );
        assert_eq!(
            parse_world(&data),
            Err(WorldError::InvalidPaletteIndex { index: 5, count: 2 })
        );
    }

    #[test]
    fn rejects_duplicate_chunk() {
        let dense = [0u8; WORLD_CHUNK_VOLUME];
        let data = build_world(
            [16, 16, 16],
            &["air"],
            vec![(0, 0, 0, 0, dense.to_vec()), (0, 0, 0, 0, dense.to_vec())],
        );
        assert_eq!(
            parse_world(&data),
            Err(WorldError::DuplicateChunk { x: 0, y: 0, z: 0 })
        );
    }

    #[test]
    fn rejects_chunk_out_of_bounds() {
        let dense = [0u8; WORLD_CHUNK_VOLUME];
        let data = build_world([16, 16, 16], &["air"], vec![(5, 0, 0, 0, dense.to_vec())]);
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

    #[test]
    fn rejects_axis_too_large() {
        let data = build_world([2_000_000, 1, 1], &["air"], vec![]);
        assert!(matches!(
            parse_world(&data),
            Err(WorldError::WorldAxisTooLarge { .. })
        ));
    }
}
