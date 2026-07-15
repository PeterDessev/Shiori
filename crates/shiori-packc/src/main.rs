//! shiori-packc — the pack compiler.
//!
//! Builds language packs from upstream open data. Runs in CI (or on a
//! developer machine), never inside the app: the app only ever consumes
//! finished packs.
//!
//! ```sh
//! shiori-packc build-grc --morphgnt <dir> --glosses <tsv> --out <dir> \
//!     --license "CC BY-SA 4.0"
//! ```
//!
//! `--morphgnt` points at a checkout of the MorphGNT sblgnt files
//! (`*-morphgnt.txt`); `--glosses` is a `lemma<TAB>gloss` file (e.g.
//! derived from the public-domain Dodson lexicon). The license gate
//! refuses NonCommercial sources outright: NC data (PROIEL, CATSS,
//! OpenGNT, the gcelano LSJ conversion…) must never reach a
//! redistributable pack.

use std::collections::HashMap;
use std::path::Path;

mod grc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("build-grc") => build_grc(&args[1..]),
        _ => {
            eprintln!(
                "usage: shiori-packc build-grc --morphgnt <dir> --glosses <tsv> \
                 --out <dir> --license <spdx-ish>"
            );
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn build_grc(args: &[String]) -> Result<(), String> {
    let opts = parse_flags(args)?;
    let morphgnt = opts.required("morphgnt")?;
    let out = opts.required("out")?;
    let license = opts.required("license")?;
    reject_non_commercial(&license)?;

    let glosses = match opts.get("glosses") {
        Some(path) => load_glosses(Path::new(path))?,
        None => HashMap::new(),
    };

    let report = grc::build_pack(Path::new(&morphgnt), &glosses, Path::new(&out), &license)
        .map_err(|e| e.to_string())?;
    println!(
        "pack written to {out}: {} texts, {} sentences, {} lemmas, {} tags",
        report.texts, report.sentences, report.lemmas, report.tags
    );
    Ok(())
}

/// The gate that keeps NC data out of redistributable packs.
fn reject_non_commercial(license: &str) -> Result<(), String> {
    let l = license.to_lowercase();
    if l.contains("nc") && (l.contains("cc") || l.contains("creative"))
        || l.contains("noncommercial")
    {
        return Err(format!(
            "license '{license}' is NonCommercial — it cannot ship in a \
             redistributable pack (use SBLGNT/Nestle1904/Byzantine data instead)"
        ));
    }
    Ok(())
}

fn load_glosses(path: &Path) -> Result<HashMap<String, String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(raw
        .lines()
        .filter_map(|line| {
            let (lemma, gloss) = line.split_once('\t')?;
            Some((lemma.trim().to_string(), gloss.trim().to_string()))
        })
        .collect())
}

struct Flags(HashMap<String, String>);

impl Flags {
    fn required(&self, name: &str) -> Result<String, String> {
        self.get(name)
            .map(str::to_string)
            .ok_or_else(|| format!("missing --{name}"))
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut out = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let key = args[i]
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument '{}'", args[i]))?;
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("--{key} needs a value"))?;
        out.insert(key.to_string(), value.clone());
        i += 2;
    }
    Ok(Flags(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nc_licenses_are_refused() {
        assert!(reject_non_commercial("CC BY-NC-SA 4.0").is_err());
        assert!(reject_non_commercial("NonCommercial research use").is_err());
        assert!(reject_non_commercial("CC BY-SA 4.0").is_ok());
        assert!(reject_non_commercial("CC BY 4.0").is_ok());
        assert!(reject_non_commercial("Public Domain").is_ok());
    }
}
