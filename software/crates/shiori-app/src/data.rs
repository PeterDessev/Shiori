//! First-run data acquisition: dictionary and frequency list.

use shiori_db::{DictFormRow, FormRole};
use shiori_dict::{download, FrequencyList, JmdictFile};

use crate::{App, Result};

/// What reference data is present in the database for the active
/// language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataStatus {
    pub dict_entries: u64,
    pub frequency_words: u64,
    /// Japanese only; 0 and irrelevant for pack languages.
    pub kanji: u64,
    /// Japanese only; 0 and irrelevant for pack languages.
    pub jlpt_words: u64,
    /// Whether everything the active language declares is present.
    pub ready: bool,
}

impl DataStatus {
    pub fn is_ready(&self) -> bool {
        self.ready
    }
}

impl App {
    pub fn data_status(&self) -> Result<DataStatus> {
        let dict_entries = self.db.dict_entry_count(self.active_dict_source())?;
        let frequency_words = self.db.frequency_count(self.active_lang())?;
        let kanji = self.db.kanji_count()?;
        let jlpt_words = self.db.jlpt_count()?;
        // Japanese requires its full reference bundle; pack languages
        // declare what they ship and only the dictionary is essential.
        let ready = if self.active_lang() == "ja" {
            dict_entries > 0 && frequency_words > 0 && kanji > 0 && jlpt_words > 0
        } else {
            dict_entries > 0
        };
        Ok(DataStatus {
            dict_entries,
            frequency_words,
            kanji,
            jlpt_words,
            ready,
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
        // Pack languages install from their local pack directory; the
        // download pipeline below is Japanese's reference bundle.
        if self.active_lang() != "ja" {
            on_progress("Installing language pack data…");
            self.ensure_pack_data_with_progress(self.active_lang(), &mut on_progress)?;
            on_progress("Reference data ready.");
            return self.data_status();
        }
        if self.db.dict_entry_count(self.active_dict_source())? == 0 {
            on_progress("Downloading JMdict dictionary…");
            let path = download::ensure_jmdict(&self.data_dir)?;
            on_progress("Parsing dictionary…");
            let json = std::fs::read_to_string(path)?;
            on_progress("Importing dictionary into database…");
            self.import_dictionary_json(&json)?;
        }
        if self.db.frequency_count(self.active_lang())? == 0 {
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

    /// Parse a jmdict-simplified JSON document and store it under the
    /// 'jmdict' source.
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
                    role: FormRole::Orthographic,
                    is_common: f.common,
                })
                .chain(entry.kana.iter().map(|f| DictFormRow {
                    text: f.text.clone(),
                    role: FormRole::Phonetic,
                    is_common: f.common,
                }))
                .collect();
            let json = serde_json::to_string(&entry).ok()?;
            Some((seq.to_string(), json, forms))
        });
        Ok(self.db.import_dictionary("jmdict", entries)?)
    }

    /// Parse a one-word-per-line frequency list and store it for the
    /// active language.
    pub fn import_frequency_text(&self, text: &str) -> Result<u64> {
        let list = FrequencyList::parse(text);
        Ok(self.db.import_frequency(self.active_lang(), list.iter())?)
    }
}
