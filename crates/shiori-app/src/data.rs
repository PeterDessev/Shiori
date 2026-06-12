//! First-run data acquisition: dictionary and frequency list.

use shiori_db::DictFormRow;
use shiori_dict::{download, FrequencyList, JmdictFile};

use crate::{App, Result};

/// What reference data is present in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataStatus {
    pub dict_entries: u64,
    pub frequency_words: u64,
    pub kanji: u64,
    pub jlpt_words: u64,
}

impl DataStatus {
    pub fn is_ready(&self) -> bool {
        self.dict_entries > 0
            && self.frequency_words > 0
            && self.kanji > 0
            && self.jlpt_words > 0
    }
}

impl App {
    pub fn data_status(&self) -> Result<DataStatus> {
        Ok(DataStatus {
            dict_entries: self.db.dict_entry_count()?,
            frequency_words: self.db.frequency_count()?,
            kanji: self.db.kanji_count()?,
            jlpt_words: self.db.jlpt_count()?,
        })
    }

    /// Download (if not cached on disk) and import the dictionary,
    /// frequency list, and kanji data. Heavy: run on a background
    /// thread. `on_progress` receives human-readable status lines. Each
    /// step skips when its data is already imported, so retries are
    /// incremental.
    pub fn download_and_import_data(
        &self,
        mut on_progress: impl FnMut(&str),
    ) -> Result<DataStatus> {
        if self.db.dict_entry_count()? == 0 {
            on_progress("Downloading JMdict dictionary…");
            let path = download::ensure_jmdict(&self.data_dir)?;
            on_progress("Parsing dictionary…");
            let json = std::fs::read_to_string(path)?;
            on_progress("Importing dictionary into database…");
            self.import_dictionary_json(&json)?;
        }
        if self.db.frequency_count()? == 0 {
            on_progress("Downloading frequency list…");
            let path = download::ensure_frequency_list(&self.data_dir)?;
            let text = std::fs::read_to_string(path)?;
            on_progress("Importing frequency list…");
            self.import_frequency_text(&text)?;
        }
        if self.db.kanji_count()? == 0 {
            on_progress("Downloading KANJIDIC2 (kanji data)…");
            let kanjidic = shiori_dict::kanji::ensure_kanjidic2(&self.data_dir)?;
            on_progress("Downloading KanjiVG (stroke order)…");
            let kanjivg = shiori_dict::kanji::ensure_kanjivg(&self.data_dir)?;
            on_progress("Parsing and importing kanji…");
            self.import_kanji_data(&kanjidic, &kanjivg)?;
        }
        if self.db.jlpt_count()? == 0 {
            on_progress("Downloading JLPT vocabulary lists…");
            let path = shiori_dict::jlpt::ensure_jlpt_lists(&self.data_dir)?;
            on_progress("Importing JLPT lists…");
            let words = shiori_dict::jlpt::load_jlpt_lists(&path)?;
            self.db
                .import_jlpt(words.into_iter().map(|w| (w.level, w.word, w.kana)))?;
        }
        on_progress("Reference data ready.");
        self.data_status()
    }

    /// Parse the two kanji archives and store the joined entries.
    pub fn import_kanji_data(
        &self,
        kanjidic2_gz: &std::path::Path,
        kanjivg_gz: &std::path::Path,
    ) -> Result<u64> {
        let entries = shiori_dict::kanji::load_kanji(kanjidic2_gz, kanjivg_gz)?;
        let rows = entries.into_iter().map(|k| shiori_db::KanjiRow {
            literal: k.literal,
            grade: k.grade,
            stroke_count: k.stroke_count,
            jlpt: k.jlpt,
            freq: k.freq,
            on_readings: k.on_readings,
            kun_readings: k.kun_readings,
            nanori: k.nanori,
            meanings: k.meanings,
            variants: k.variants,
            strokes: k.strokes,
        });
        Ok(self.db.import_kanji(rows)?)
    }

    /// Parse a jmdict-simplified JSON document and store it.
    pub fn import_dictionary_json(&self, json: &str) -> Result<u64> {
        let file = JmdictFile::parse(json)?;
        let entries = file.words.into_iter().filter_map(|entry| {
            let seq = entry.seq();
            if seq == 0 {
                return None;
            }
            let forms = entry
                .kanji
                .iter()
                .map(|f| DictFormRow {
                    text: f.text.clone(),
                    is_kana: false,
                    is_common: f.common,
                })
                .chain(entry.kana.iter().map(|f| DictFormRow {
                    text: f.text.clone(),
                    is_kana: true,
                    is_common: f.common,
                }))
                .collect();
            let json = serde_json::to_string(&entry).ok()?;
            Some((seq, json, forms))
        });
        Ok(self.db.import_dictionary(entries)?)
    }

    /// Parse a one-word-per-line frequency list and store it.
    pub fn import_frequency_text(&self, text: &str) -> Result<u64> {
        let list = FrequencyList::parse(text);
        Ok(self.db.import_frequency(list.iter())?)
    }
}
