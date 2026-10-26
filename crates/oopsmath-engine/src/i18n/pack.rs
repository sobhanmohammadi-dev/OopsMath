//! Reading language packs: folders and `.zip` archives with the same layout.
//!
//! ```text
//! <lang>/_meta.ron
//! <lang>/core.ftl
//! <lang>/levels/<stage id>.ftl
//! <lang>/fonts/<font file>.ttf|otf        (optional)
//! ```
//!
//! Everything is read into memory with size limits; nothing is extracted to
//! disk and paths that try to leave the pack are ignored.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use thiserror::Error;

pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PACK_BYTES: u64 = 96 * 1024 * 1024;
const MAX_DEPTH: usize = 4;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("{0}")]
    Io(String),
    #[error("invalid zip: {0}")]
    Zip(String),
    #[error("{0}")]
    TooLarge(String),
}

/// The files of one or more languages, keyed by `lang/relative/path`.
#[derive(Debug, Default, Clone)]
pub struct PackFiles {
    /// Where the files came from, for diagnostics.
    pub origin: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Whether an entry name is a file the game uses and is safe to read.
pub fn accepted(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.ends_with('/') {
        return false;
    }
    if name
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    ["ron", "ftl", "ttf", "otf"]
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

impl PackFiles {
    /// Reads a zip archive held in memory.
    pub fn from_zip_bytes(origin: &str, bytes: &[u8]) -> Result<Self, PackError> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|err| PackError::Zip(err.to_string()))?;
        let names: Vec<String> = archive
            .file_names()
            .filter_map(|name| name.ok())
            .map(|name| name.into_owned())
            .collect();
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for name in names {
            if !accepted(&name) {
                continue;
            }
            let mut entry = archive
                .by_name(&name)
                .map_err(|err| PackError::Zip(format!("{name}: {err}")))?;
            if entry.size() > MAX_FILE_BYTES {
                return Err(PackError::TooLarge(format!("{name} is larger than 16 MiB")));
            }
            let mut buf = Vec::new();
            (&mut entry)
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut buf)
                .map_err(|err| PackError::Zip(format!("{name}: {err}")))?;
            if buf.len() as u64 > MAX_FILE_BYTES {
                return Err(PackError::TooLarge(format!("{name} is larger than 16 MiB")));
            }
            total += buf.len() as u64;
            if total > MAX_PACK_BYTES {
                return Err(PackError::TooLarge(
                    "the pack is larger than 96 MiB".to_string(),
                ));
            }
            files.insert(name, buf);
        }
        Ok(Self {
            origin: origin.to_string(),
            files,
        })
    }

    /// Reads the folder of one language (`root`), keying files `lang/...`.
    pub fn from_dir(origin: &str, root: &Path, lang: &str) -> Result<Self, PackError> {
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        collect_dir(root, lang, 0, &mut files, &mut total)?;
        Ok(Self {
            origin: origin.to_string(),
            files,
        })
    }

    /// The language codes (first path segments) present.
    pub fn languages(&self) -> BTreeSet<String> {
        self.files
            .keys()
            .filter_map(|key| key.split('/').next())
            .map(str::to_string)
            .collect()
    }
}

fn collect_dir(
    dir: &Path,
    prefix: &str,
    depth: usize,
    files: &mut BTreeMap<String, Vec<u8>>,
    total: &mut u64,
) -> Result<(), PackError> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|err| PackError::Io(format!("cannot read {}: {err}", dir.display())))?;
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let key = format!("{prefix}/{name}");
        if path.is_dir() {
            collect_dir(&path, &key, depth + 1, files, total)?;
        } else if accepted(&key) {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size > MAX_FILE_BYTES {
                return Err(PackError::TooLarge(format!(
                    "{} is larger than 16 MiB",
                    path.display()
                )));
            }
            *total += size;
            if *total > MAX_PACK_BYTES {
                return Err(PackError::TooLarge(format!(
                    "{} is larger than 96 MiB",
                    dir.display()
                )));
            }
            let bytes = std::fs::read(&path)
                .map_err(|err| PackError::Io(format!("cannot read {}: {err}", path.display())))?;
            files.insert(key, bytes);
        }
    }
    Ok(())
}

/// Reads every language folder and `.zip` in `dir` (folders first, then zips,
/// each sorted by name). Problems are returned as messages, never as a failure.
pub fn read_i18n_dir(dir: &Path) -> (Vec<PackFiles>, Vec<String>) {
    let mut diagnostics = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        diagnostics.push(format!("language folder {} was not found", dir.display()));
        return (Vec::new(), diagnostics);
    };
    let mut folders = Vec::new();
    let mut zips = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            folders.push(path);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            zips.push(path);
        }
    }
    folders.sort();
    zips.sort();
    let mut packs = Vec::new();
    for folder in folders {
        let Some(lang) = folder.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        match PackFiles::from_dir(&folder.display().to_string(), &folder, lang) {
            Ok(pack) => packs.push(pack),
            Err(err) => diagnostics.push(err.to_string()),
        }
    }
    for zip in zips {
        let origin = zip.display().to_string();
        match std::fs::read(&zip) {
            Ok(bytes) => match PackFiles::from_zip_bytes(&origin, &bytes) {
                Ok(pack) => packs.push(pack),
                Err(err) => diagnostics.push(format!("{origin}: {err}")),
            },
            Err(err) => diagnostics.push(format!("cannot read {origin}: {err}")),
        }
    }
    (packs, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_safe_game_files_are_accepted() {
        for ok in [
            "fa/_meta.ron",
            "fa/core.ftl",
            "fa/levels/x.ftl",
            "fa/fonts/A.TTF",
            "fa/fonts/b.otf",
        ] {
            assert!(accepted(ok), "{ok}");
        }
        for bad in [
            "",
            "/fa/core.ftl",
            "fa/../core.ftl",
            "fa\\core.ftl",
            "fa/",
            "fa//core.ftl",
            "fa/readme.txt",
            "fa/run.exe",
            "./fa/core.ftl",
        ] {
            assert!(!accepted(bad), "{bad}");
        }
    }

    #[test]
    fn a_folder_is_read_with_lang_prefixed_keys() {
        let root = std::env::temp_dir().join(format!("oopsmath_pack_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lang = root.join("fa");
        std::fs::create_dir_all(lang.join("levels")).unwrap();
        std::fs::write(lang.join("_meta.ron"), "(x)").unwrap();
        std::fs::write(lang.join("core.ftl"), "a = b").unwrap();
        std::fs::write(lang.join("levels/s1.ftl"), "k = v").unwrap();
        std::fs::write(lang.join("notes.txt"), "ignored").unwrap();
        let pack = PackFiles::from_dir("test", &lang, "fa").unwrap();
        let keys: Vec<_> = pack.files.keys().map(String::as_str).collect();
        assert_eq!(keys, ["fa/_meta.ron", "fa/core.ftl", "fa/levels/s1.ftl"]);
        assert_eq!(pack.languages().into_iter().collect::<Vec<_>>(), ["fa"]);

        let (packs, diagnostics) = read_i18n_dir(&root);
        assert_eq!(packs.len(), 1);
        assert!(diagnostics.is_empty());
        let (none, diagnostics) = read_i18n_dir(&root.join("missing"));
        assert!(none.is_empty() && !diagnostics.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn garbage_is_not_a_zip() {
        assert!(matches!(
            PackFiles::from_zip_bytes("x", b"definitely not a zip"),
            Err(PackError::Zip(_))
        ));
    }
}
