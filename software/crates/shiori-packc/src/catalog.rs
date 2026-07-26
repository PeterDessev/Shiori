//! `catalog` subcommand: zip finished packs and emit the `catalog.json`
//! the app's browse section consumes.
//!
//! Input is a directory of pack directories (the same layout the app's
//! `packs/` uses); output is one `<lang>.zip` per pack plus a
//! `catalog.json` whose entries carry the real SHA-256 and size of each
//! zip, pointing at `<base-url>/<lang>.zip`. Publish by uploading the
//! whole output directory to that base URL (for the default catalog: as
//! assets of the app repository's `pack-catalog` release tag).

use std::io::Write;
use std::path::{Path, PathBuf};

use shiori_pack::catalog::{parse_pack_catalog, PackCatalogEntry, PackCatalogFile, CATALOG_SCHEMA};
use shiori_pack::Pack;

#[derive(Debug)]
pub struct CatalogReport {
    pub packs: usize,
    pub catalog_path: PathBuf,
}

/// Zip every pack under `packs_dir` into `out_dir` and write the
/// catalog describing them.
pub fn build_catalog(
    packs_dir: &Path,
    base_url: &str,
    version: &str,
    out_dir: &Path,
) -> Result<CatalogReport, String> {
    let mut pack_dirs: Vec<PathBuf> = std::fs::read_dir(packs_dir)
        .map_err(|e| format!("{}: {e}", packs_dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("manifest.toml").exists())
        .collect();
    pack_dirs.sort();
    if pack_dirs.is_empty() {
        return Err(format!(
            "no packs (directories containing manifest.toml) under {}",
            packs_dir.display()
        ));
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;

    let base_url = base_url.trim_end_matches('/');
    let mut entries = Vec::new();
    for dir in &pack_dirs {
        let pack = Pack::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let lang = pack.manifest.lang.clone();
        if !shiori_pack::is_safe_lang_code(&lang) {
            return Err(format!(
                "{}: unusable language code '{lang}'",
                dir.display()
            ));
        }
        if lang == "ja" {
            return Err(format!(
                "{}: 'ja' is built into the app and cannot ship as a pack",
                dir.display()
            ));
        }
        crate::reject_non_commercial(&pack.manifest.license)?;

        let zip_name = format!("{lang}.zip");
        let zip_path = out_dir.join(&zip_name);
        zip_pack_dir(dir, &lang, &zip_path)?;
        let bytes = std::fs::read(&zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;

        entries.push(PackCatalogEntry {
            lang,
            name: pack.manifest.name.clone(),
            description: pack.manifest.description.clone(),
            license: pack.manifest.license.clone(),
            url: format!("{base_url}/{zip_name}"),
            sha256: sha256_hex(&bytes),
            size_bytes: bytes.len() as u64,
            version: version.to_string(),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let count = entries.len();
    let file = PackCatalogFile {
        catalog: CATALOG_SCHEMA,
        packs: entries,
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    // The generated document must parse exactly the way the app will
    // parse it — with every entry surviving the filters.
    let parsed = parse_pack_catalog(&json).map_err(|e| e.to_string())?;
    if parsed.len() != count {
        return Err("generated catalog lost entries to the app-side filters".into());
    }
    let catalog_path = out_dir.join("catalog.json");
    std::fs::write(&catalog_path, &json).map_err(|e| format!("{}: {e}", catalog_path.display()))?;
    Ok(CatalogReport {
        packs: count,
        catalog_path,
    })
}

/// Zip a pack directory with everything under a single top-level
/// `<lang>/` folder (the shape the app's zip installer expects from
/// hosted archives).
fn zip_pack_dir(dir: &Path, lang: &str, zip_path: &Path) -> Result<(), String> {
    let file =
        std::fs::File::create(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    let mut files = Vec::new();
    collect_files(dir, &mut files).map_err(|e| format!("{}: {e}", dir.display()))?;
    files.sort();
    for path in files {
        let rel = path
            .strip_prefix(dir)
            .map_err(|e| e.to_string())?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        writer
            .start_file(format!("{lang}/{rel}"), opts)
            .map_err(|e| e.to_string())?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        writer.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn write_minimal_pack(dir: &Path, lang: &str, name: &str, license: &str) {
        std::fs::create_dir_all(dir.join("texts")).unwrap();
        std::fs::write(
            dir.join("manifest.toml"),
            format!(
                "schema = 1\nlang = \"{lang}\"\nname = \"{name}\"\n\
                 dict_source = \"{lang}-pack\"\n\
                 description = \"A test pack.\"\nlicense = \"{license}\"\n\n\
                 [prompt]\nlanguage_name = \"{name}\"\n\
                 chat_persona = \"a speaker\"\n\
                 immerse_instruction = \"Write {name}.\"\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("frequency.tsv"), "abc\t1\n").unwrap();
    }

    #[test]
    fn catalog_generation_round_trips_through_the_app_parser() {
        let root = std::env::temp_dir().join(format!("packc-catalog-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        write_minimal_pack(
            &root.join("packs").join("grc"),
            "grc",
            "Koine Greek",
            "CC BY 4.0",
        );
        write_minimal_pack(
            &root.join("packs").join("la"),
            "la",
            "Latin",
            "Public Domain",
        );
        let out = root.join("dist");

        let report = build_catalog(
            &root.join("packs"),
            "https://example.com/packs/",
            "2026.07",
            &out,
        )
        .unwrap();
        assert_eq!(report.packs, 2);

        // The document parses with the same code the app uses, keeping
        // every entry, sorted by name, with real hashes and sizes.
        let json = std::fs::read_to_string(report.catalog_path).unwrap();
        let entries = parse_pack_catalog(&json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Koine Greek");
        assert_eq!(entries[0].url, "https://example.com/packs/grc.zip");
        assert_eq!(entries[0].version, "2026.07");
        assert_eq!(entries[0].description, "A test pack.");
        let zip_bytes = std::fs::read(out.join("grc.zip")).unwrap();
        assert_eq!(entries[0].sha256, sha256_hex(&zip_bytes));
        assert_eq!(entries[0].size_bytes, zip_bytes.len() as u64);

        // The zip wraps everything in a single <lang>/ folder and the
        // manifest survives the round trip.
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            names.contains(&"grc/manifest.toml".to_string()),
            "{names:?}"
        );
        let mut manifest = String::new();
        archive
            .by_name("grc/manifest.toml")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        assert!(shiori_pack::Manifest::parse(&manifest).is_ok());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn nc_packs_are_refused_and_empty_dirs_error() {
        let root = std::env::temp_dir().join(format!("packc-catalog-nc-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        write_minimal_pack(
            &root.join("packs").join("xx"),
            "xx",
            "Testish",
            "CC BY-NC 4.0",
        );
        let err =
            build_catalog(&root.join("packs"), "https://x", "", &root.join("dist")).unwrap_err();
        assert!(err.contains("NonCommercial"), "{err}");

        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(build_catalog(&empty, "https://x", "", &root.join("dist")).is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
