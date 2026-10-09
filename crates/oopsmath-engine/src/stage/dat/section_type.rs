//! The four-byte section identifier used by the DAT section directory.

use std::fmt;

/// A four-byte ASCII section type in the OopsMath DAT format.
///
/// Examples: META, STAG, WRLD, LOCL, DOCS, ASIX, ASDT.
///
/// The type is only the raw identifier; the directory parser validates that
/// the bytes are printable ASCII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectionType(pub(crate) [u8; 4]);

impl SectionType {
    pub const META: Self = Self(*b"META");
    pub const STAG: Self = Self(*b"STAG");
    pub const WRLD: Self = Self(*b"WRLD");
    pub const LOCL: Self = Self(*b"LOCL");
    pub const DOCS: Self = Self(*b"DOCS");
    pub const ASIX: Self = Self(*b"ASIX");
    pub const ASDT: Self = Self(*b"ASDT");

    /// Every section type defined by DAT v1.
    pub const KNOWN: [Self; 7] = [
        Self::META,
        Self::STAG,
        Self::WRLD,
        Self::LOCL,
        Self::DOCS,
        Self::ASIX,
        Self::ASDT,
    ];

    pub const fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    /// Returns true when this is one of the section types defined by DAT v1.
    pub fn is_known(&self) -> bool {
        Self::KNOWN.contains(self)
    }

    /// Returns true when all bytes are printable ASCII characters.
    pub(crate) fn is_printable_ascii(&self) -> bool {
        self.0.iter().all(|byte| (0x21..=0x7E).contains(byte))
    }
}

impl fmt::Display for SectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}
