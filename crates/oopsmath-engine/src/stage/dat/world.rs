//! Decoder for the OopsMath Binary Voxel World v1 (`WRLD` section).
//!
//! This mirrors the encoder in `tools/oopsmath-stage-compiler/oopsmath_stage/world.py`.
//!
//! Layout (little endian):
//!
//! ```text
//! header: "<4sHHIIIII"  (28 bytes)
//!   0  magic[4]          "OWLD"
//!   4  u16 version       1
//!   6  u16 chunk_size    16
//!   8  u32 size_x        world size in voxels per axis
//!  12  u32 size_y
//!  16  u32 size_z
//!  20  u32 palette_count
//!  24  u32 chunk_count
//! ```
//!
//! Palette: `palette_count` entries of `u16 length + UTF-8 name bytes`.
//! Index 0 is the air block.
//!
//! Chunk records: `"<HHHBI"` (11 bytes) = chunk x, y, z (u16 chunk coordinates),
//! encoding (u8: 0 dense, 1 RLE), payload size (u32). A chunk holds 4096 cells
//! and uses `u16` palette indices when the palette has more than 256 entries,
//! otherwise `u8` indices:
//!
//! * dense - exactly 4096 indices;
//! * RLE   - `(run_length, index)` pairs (same width as the index) covering
//!   exactly 4096 cells.
//!
//! Cell order inside a chunk is x fastest, then y, then z:
//! `cell = z * 256 + y * 16 + x` with `x, y, z` local to the chunk.

use std::collections::{HashMap, HashSet};

use crate::stage::dat::bytes::{le_u16, le_u32};
use crate::stage::dat::error::WorldError;
use crate::stage::world::VoxelWorld;

/// WRLD magic `OWLD`.
pub(crate) const WORLD_MAGIC: [u8; 4] = *b"OWLD";
/// WRLD format version.
pub(crate) const WORLD_VERSION: u16 = 1;
/// Fixed chunk edge length.
pub(crate) const WORLD_CHUNK_SIZE: u16 = 16;
/// Number of cells in a chunk.
pub(crate) const WORLD_CHUNK_VOLUME: usize = (WORLD_CHUNK_SIZE as usize).pow(3);
/// Size of the fixed WRLD header in bytes.
pub(crate) const WORLD_HEADER_SIZE: usize = 28;
/// Size of one chunk record in bytes.
const CHUNK_RECORD_SIZE: usize = 11;
/// Largest palette whose indices still fit in one byte.
const NARROW_PALETTE_LIMIT: u32 = 256;
/// Maximum palette size (indices are at most `u16`).
pub(crate) const MAX_PALETTE: u32 = u16::MAX as u32;
/// Sanity limit for chunk records of one world.
pub(crate) const MAX_CHUNKS: u32 = 16_000_000;
/// Maximum supported axis length (from the compiler's MAX_WORLD_AXIS).
pub(crate) const MAX_WORLD_AXIS: u32 = 1_000_000;

const ENCODING_DENSE: u8 = 0;
const ENCODING_RLE: u8 = 1;

/// Decodes a WRLD payload.
pub(crate) fn parse_world(data: &[u8]) -> Result<VoxelWorld, WorldError> {
    let mut cursor = Cursor::new(data);
    let header = WorldHeader::read(&mut cursor)?;
    let palette = read_palette(&mut cursor, header.palette_count)?;
    let voxels = read_chunks(&mut cursor, &header)?;
    Ok(VoxelWorld {
        size: header.size,
        palette,
        voxels,
    })
}

// -----------------------------------------------------------------------------
// Header and palette
// -----------------------------------------------------------------------------

struct WorldHeader {
    size: [u32; 3],
    palette_count: u32,
    chunk_count: u32,
}

impl WorldHeader {
    fn read(cursor: &mut Cursor<'_>) -> Result<Self, WorldError> {
        let bytes = cursor
            .take(WORLD_HEADER_SIZE)
            .ok_or(WorldError::HeaderTooSmall)?;

        if bytes[..4] != WORLD_MAGIC {
            return Err(WorldError::InvalidMagic);
        }
        let version = le_u16(bytes, 4);
        if version != WORLD_VERSION {
            return Err(WorldError::UnsupportedVersion(version));
        }
        let chunk_size = le_u16(bytes, 6);
        if chunk_size != WORLD_CHUNK_SIZE {
            return Err(WorldError::UnsupportedChunkSize(chunk_size));
        }
        let size = [le_u32(bytes, 8), le_u32(bytes, 12), le_u32(bytes, 16)];
        if size.iter().any(|axis| *axis > MAX_WORLD_AXIS) {
            return Err(WorldError::WorldAxisTooLarge { dims: size });
        }
        let palette_count = le_u32(bytes, 20);
        if palette_count > MAX_PALETTE {
            return Err(WorldError::PaletteTooLarge {
                declared: palette_count,
                limit: MAX_PALETTE,
            });
        }
        let chunk_count = le_u32(bytes, 24);
        if chunk_count > MAX_CHUNKS {
            return Err(WorldError::ChunkCountTooLarge {
                declared: chunk_count,
                limit: MAX_CHUNKS,
            });
        }
        Ok(Self {
            size,
            palette_count,
            chunk_count,
        })
    }

    /// Number of chunks along each axis.
    fn chunk_grid(&self) -> [u32; 3] {
        self.size.map(|axis| axis.div_ceil(u32::from(WORLD_CHUNK_SIZE)))
    }

    /// Palettes above 256 entries use two-byte indices.
    fn wide_indices(&self) -> bool {
        self.palette_count > NARROW_PALETTE_LIMIT
    }
}

fn read_palette(cursor: &mut Cursor<'_>, count: u32) -> Result<Vec<String>, WorldError> {
    // The count is only trusted up to a small capacity hint.
    let mut palette = Vec::with_capacity(count.min(4096) as usize);
    for index in 0..count {
        let length = cursor
            .u16()
            .ok_or(WorldError::PaletteLengthOutOfBounds { index, length: 0 })?;
        let bytes = cursor
            .take(usize::from(length))
            .ok_or(WorldError::PaletteLengthOutOfBounds {
                index,
                length: u64::from(length),
            })?;
        let name =
            std::str::from_utf8(bytes).map_err(|_| WorldError::PaletteNotUtf8 { index })?;
        palette.push(name.to_owned());
    }
    Ok(palette)
}

// -----------------------------------------------------------------------------
// Chunks
// -----------------------------------------------------------------------------

/// One 11-byte chunk record header.
struct ChunkRecord {
    x: u16,
    y: u16,
    z: u16,
    encoding: u8,
    payload_len: u32,
}

impl ChunkRecord {
    fn read(cursor: &mut Cursor<'_>) -> Result<Self, WorldError> {
        let offset = cursor.position() as u64;
        let bytes = cursor
            .take(CHUNK_RECORD_SIZE)
            .ok_or(WorldError::ChunkRecordTruncated { offset })?;
        Ok(Self {
            x: le_u16(bytes, 0),
            y: le_u16(bytes, 2),
            z: le_u16(bytes, 4),
            encoding: bytes[6],
            payload_len: le_u32(bytes, 7),
        })
    }

    fn coords(&self) -> (u16, u16, u16) {
        (self.x, self.y, self.z)
    }

    fn cell_count_mismatch(&self) -> WorldError {
        WorldError::ChunkCellCountMismatch {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

fn read_chunks(
    cursor: &mut Cursor<'_>,
    header: &WorldHeader,
) -> Result<HashMap<[u32; 3], u16>, WorldError> {
    let grid = header.chunk_grid();
    let wide = header.wide_indices();
    let mut seen: HashSet<(u16, u16, u16)> = HashSet::new();
    let mut voxels: HashMap<[u32; 3], u16> = HashMap::new();

    for _ in 0..header.chunk_count {
        let record = ChunkRecord::read(cursor)?;
        let (x, y, z) = record.coords();

        if !seen.insert(record.coords()) {
            return Err(WorldError::DuplicateChunk { x, y, z });
        }
        if u32::from(x) >= grid[0] || u32::from(y) >= grid[1] || u32::from(z) >= grid[2] {
            return Err(WorldError::ChunkOutOfBounds {
                x,
                y,
                z,
                dims: header.size,
            });
        }

        let remaining = cursor.remaining();
        let payload = cursor
            .take(record.payload_len as usize)
            .ok_or(WorldError::ChunkPayloadTruncated {
                x,
                y,
                z,
                expected: u64::from(record.payload_len),
                actual: remaining as u64,
            })?;

        let cells = decode_chunk(&record, payload, wide)?;
        insert_voxels(&record, &cells, header, &mut voxels)?;
    }
    Ok(voxels)
}

/// Decodes one chunk payload into exactly `WORLD_CHUNK_VOLUME` palette indices.
fn decode_chunk(record: &ChunkRecord, payload: &[u8], wide: bool) -> Result<Vec<u16>, WorldError> {
    match record.encoding {
        ENCODING_DENSE => decode_dense(record, payload, wide),
        ENCODING_RLE => decode_rle(record, payload, wide),
        encoding => Err(WorldError::UnknownChunkEncoding {
            x: record.x,
            y: record.y,
            z: record.z,
            encoding,
        }),
    }
}

fn decode_dense(record: &ChunkRecord, payload: &[u8], wide: bool) -> Result<Vec<u16>, WorldError> {
    let index_size = if wide { 2 } else { 1 };
    if payload.len() != WORLD_CHUNK_VOLUME * index_size {
        return Err(record.cell_count_mismatch());
    }
    Ok(if wide {
        payload
            .chunks_exact(2)
            .map(|pair| le_u16(pair, 0))
            .collect()
    } else {
        payload.iter().map(|byte| u16::from(*byte)).collect()
    })
}

fn decode_rle(record: &ChunkRecord, payload: &[u8], wide: bool) -> Result<Vec<u16>, WorldError> {
    // Each run is (length, index), both as wide as the palette indices.
    let run_size = if wide { 4 } else { 2 };
    let runs = payload.len() / run_size;
    if runs > WORLD_CHUNK_VOLUME {
        return Err(WorldError::ChunkRunCountTooLarge {
            x: record.x,
            y: record.y,
            z: record.z,
            runs: runs as u64,
            limit: WORLD_CHUNK_VOLUME as u64,
        });
    }
    if payload.len() % run_size != 0 {
        return Err(WorldError::MalformedRlePayload {
            x: record.x,
            y: record.y,
            z: record.z,
            len: payload.len() as u64,
        });
    }

    let mut cells: Vec<u16> = Vec::with_capacity(WORLD_CHUNK_VOLUME);
    for run in payload.chunks_exact(run_size) {
        let (length, index) = if wide {
            (usize::from(le_u16(run, 0)), le_u16(run, 2))
        } else {
            (usize::from(run[0]), u16::from(run[1]))
        };
        if cells.len() + length > WORLD_CHUNK_VOLUME {
            return Err(record.cell_count_mismatch());
        }
        cells.resize(cells.len() + length, index);
    }
    if cells.len() != WORLD_CHUNK_VOLUME {
        return Err(record.cell_count_mismatch());
    }
    Ok(cells)
}

/// Validates the palette indices of a decoded chunk and stores its non-air
/// cells that lie inside the world. Cells in the padding of edge chunks
/// (beyond the declared world size) are dropped.
fn insert_voxels(
    record: &ChunkRecord,
    cells: &[u16],
    header: &WorldHeader,
    voxels: &mut HashMap<[u32; 3], u16>,
) -> Result<(), WorldError> {
    let chunk = u32::from(WORLD_CHUNK_SIZE);
    let origin = [
        u32::from(record.x) * chunk,
        u32::from(record.y) * chunk,
        u32::from(record.z) * chunk,
    ];
    for (cell, &index) in cells.iter().enumerate() {
        if index == 0 {
            continue; // air
        }
        if u32::from(index) >= header.palette_count {
            return Err(WorldError::InvalidPaletteIndex {
                index,
                count: header.palette_count,
            });
        }
        let position = [
            origin[0] + (cell % 16) as u32,
            origin[1] + ((cell / 16) % 16) as u32,
            origin[2] + (cell / 256) as u32,
        ];
        if position.iter().zip(header.size).all(|(p, s)| *p < s) {
            voxels.insert(position, index);
        }
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Bounds-checked byte cursor
// -----------------------------------------------------------------------------

/// A forward-only reader whose every read is bounds-checked.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn position(&self) -> usize {
        self.pos
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Consumes and returns the next `len` bytes, or `None` (consuming
    /// nothing) when fewer remain.
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|bytes| le_u16(bytes, 0))
    }
}
