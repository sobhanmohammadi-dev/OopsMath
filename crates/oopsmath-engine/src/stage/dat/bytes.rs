//! Little-endian field readers for fixed-layout structures.
//!
//! These index the slice directly and therefore panic on short input. Callers
//! must have validated the length beforehand (the header, directory entries
//! and world cursor all do).

pub(crate) fn le_u16(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

pub(crate) fn le_u32(data: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&data[at..at + 4]);
    u32::from_le_bytes(buf)
}

pub(crate) fn le_u64(data: &[u8], at: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&data[at..at + 8]);
    u64::from_le_bytes(buf)
}
