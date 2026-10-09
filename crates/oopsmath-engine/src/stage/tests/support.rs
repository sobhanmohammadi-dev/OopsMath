//! Byte builders shared by the stage unit tests.

/// Size of the DAT header and of one directory entry.
pub(super) const HEADER_LEN: usize = 64;
pub(super) const ENTRY_LEN: usize = 48;

/// Field offsets inside a directory entry.
pub(super) const ENTRY_FLAGS: usize = 4;
pub(super) const ENTRY_OFFSET: usize = 8;
pub(super) const ENTRY_STORED_SIZE: usize = 16;
pub(super) const ENTRY_RAW_SIZE: usize = 24;
pub(super) const ENTRY_CRC: usize = 32;
pub(super) const ENTRY_RESERVED: usize = 40;

/// Start of directory entry `index` in a DAT built by [`build_dat`].
pub(super) fn entry_base(index: usize) -> usize {
    HEADER_LEN + index * ENTRY_LEN
}

pub(super) fn read_u64(data: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(data[at..at + 8].try_into().unwrap())
}

pub(super) fn patch_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn patch_u64(data: &mut [u8], at: usize, value: u64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

// -----------------------------------------------------------------------------
// Header
// -----------------------------------------------------------------------------

/// The variable fields of a 64-byte header; defaults describe a valid empty
/// 128-byte DAT.
#[derive(Clone)]
pub(super) struct HeaderSpec {
    pub format_version: u16,
    pub schema_version: u16,
    pub flags: u32,
    pub file_size: u64,
    pub table_offset: u64,
    pub section_count: u32,
    pub runtime_version: u32,
}

impl Default for HeaderSpec {
    fn default() -> Self {
        Self {
            format_version: 1,
            schema_version: 1,
            flags: 0,
            file_size: 128,
            table_offset: 64,
            section_count: 0,
            runtime_version: 0,
        }
    }
}

impl HeaderSpec {
    /// The raw 64 header bytes with the CRC field left zero.
    pub(super) fn build(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_LEN);
        buf.extend_from_slice(b"OOPSMDAT");
        buf.extend_from_slice(&self.format_version.to_le_bytes());
        buf.extend_from_slice(&self.schema_version.to_le_bytes());
        buf.extend_from_slice(&64u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.file_size.to_le_bytes());
        buf.extend_from_slice(&self.table_offset.to_le_bytes());
        buf.extend_from_slice(&self.section_count.to_le_bytes());
        buf.extend_from_slice(&48u32.to_le_bytes());
        buf.extend_from_slice(&self.runtime_version.to_le_bytes());
        buf.extend_from_slice(&[0u8; 16]); // CRC field + reserved tail
        buf
    }
}

/// Writes the header CRC the way the Python compiler does: CRC32 over the
/// first 48 bytes followed by 16 zero bytes.
pub(super) fn finalize(mut header: Vec<u8>) -> Vec<u8> {
    let mut crc_input = [0u8; HEADER_LEN];
    crc_input[..48].copy_from_slice(&header[..48]);
    let crc = crc32fast::hash(&crc_input);
    header[48..52].copy_from_slice(&crc.to_le_bytes());
    header
}

/// Zero-pads (or truncates) `data` to exactly `len` bytes.
pub(super) fn pad(mut data: Vec<u8>, len: usize) -> Vec<u8> {
    data.resize(len, 0);
    data
}

// -----------------------------------------------------------------------------
// Whole DAT files
// -----------------------------------------------------------------------------

/// One section to be written by [`build_dat`].
#[derive(Clone)]
pub(super) struct SectionSpec {
    pub section_type: [u8; 4],
    /// Extra entry flag bits (criticality, unknown bits, ...).
    pub flags: u32,
    pub raw: Vec<u8>,
    pub zstd: bool,
}

impl SectionSpec {
    pub(super) fn new(section_type: &[u8; 4], raw: Vec<u8>) -> Self {
        Self {
            section_type: *section_type,
            flags: 0,
            raw,
            zstd: false,
        }
    }

    pub(super) fn zstd(mut self) -> Self {
        self.zstd = true;
        self
    }

    pub(super) fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }
}

fn align_up(value: usize) -> usize {
    value.div_ceil(16) * 16
}

/// Builds a structurally valid DAT v1 file: header, directory directly after
/// it, then every section 16-byte aligned. CRCs and sizes are computed from
/// the raw payloads.
pub(super) fn build_dat(header_flags: u32, sections: &[SectionSpec]) -> Vec<u8> {
    let table_end = HEADER_LEN + ENTRY_LEN * sections.len();
    let mut file = vec![0u8; align_up(table_end)];

    for (index, spec) in sections.iter().enumerate() {
        let stored = if spec.zstd {
            zstd::bulk::compress(&spec.raw, 3).expect("zstd compression")
        } else {
            spec.raw.clone()
        };
        let offset = file.len();
        file.extend_from_slice(&stored);
        file.resize(align_up(file.len()), 0);

        let base = entry_base(index);
        file[base..base + 4].copy_from_slice(&spec.section_type);
        patch_u32(&mut file, base + ENTRY_FLAGS, spec.flags | u32::from(spec.zstd));
        patch_u64(&mut file, base + ENTRY_OFFSET, offset as u64);
        patch_u64(&mut file, base + ENTRY_STORED_SIZE, stored.len() as u64);
        patch_u64(&mut file, base + ENTRY_RAW_SIZE, spec.raw.len() as u64);
        patch_u32(&mut file, base + ENTRY_CRC, crc32fast::hash(&spec.raw));
    }

    let header = finalize(
        HeaderSpec {
            flags: header_flags,
            file_size: file.len() as u64,
            section_count: sections.len() as u32,
            runtime_version: 1 << 16,
            ..HeaderSpec::default()
        }
        .build(),
    );
    file[..HEADER_LEN].copy_from_slice(&header);
    file
}

// -----------------------------------------------------------------------------
// WRLD payloads
// -----------------------------------------------------------------------------

/// `(chunk x, chunk y, chunk z, encoding, payload)`
pub(super) type ChunkSpec = (u16, u16, u16, u8, Vec<u8>);

/// Builds a WRLD payload in the byte layout the Python encoder emits.
pub(super) fn build_world(size: [u32; 3], palette: &[&str], chunks: Vec<ChunkSpec>) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"OWLD");
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    for axis in size {
        data.extend_from_slice(&axis.to_le_bytes());
    }
    data.extend_from_slice(&(palette.len() as u32).to_le_bytes());
    data.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    for name in palette {
        data.extend_from_slice(&(name.len() as u16).to_le_bytes());
        data.extend_from_slice(name.as_bytes());
    }
    for (x, y, z, encoding, payload) in chunks {
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
        data.extend_from_slice(&z.to_le_bytes());
        data.push(encoding);
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(&payload);
    }
    data
}
