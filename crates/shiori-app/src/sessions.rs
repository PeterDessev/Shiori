//! Reading-session policy: when the velocity stat is trustworthy and how
//! page time turns into credited reading.

use chrono::Utc;
use shiori_core::DocumentId;

use crate::{App, Result};

/// Don't trust a velocity estimate built on less than this much credited
/// reading time.
const MIN_VELOCITY_SECONDS: f64 = 600.0;

impl App {
    /// Open a session row for a sitting with this document.
    pub fn start_reading_session(&self, document: DocumentId) -> Result<i64> {
        Ok(self.db().start_reading_session(document, Utc::now())?)
    }

    /// Credit active reading time to an open session.
    pub fn add_reading_time(&self, session: i64, seconds: f64, chars: u64) -> Result<()> {
        Ok(self
            .db()
            .add_reading_time(session, seconds, chars, Utc::now())?)
    }

    /// The user's reading velocity in the active language, in characters
    /// per second, if enough reading has been recorded in that language
    /// to make it meaningful (velocity differs wildly across scripts).
    pub fn reading_velocity_cps(&self) -> Result<Option<f64>> {
        let totals = self.db().reading_totals(self.active_lang())?;
        if totals.seconds >= MIN_VELOCITY_SECONDS && totals.chars > 0 {
            Ok(Some(totals.chars as f64 / totals.seconds))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap()
    }

    #[test]
    fn velocity_needs_enough_data() {
        let app = app();
        // No data at all.
        assert_eq!(app.reading_velocity_cps().unwrap(), None);

        let doc = {
            let sentences = vec![shiori_db::NewSentence {
                paragraph: 0,
                text: "猫が好きだ。".into(),
                tokens: vec![],
            }];
            app.db()
                .import_document(
                    "ja",
                    &shiori_core::DocumentMeta {
                        title: "t".into(),
                        ..Default::default()
                    },
                    "h",
                    Utc::now(),
                    &sentences,
                )
                .unwrap()
        };

        // Below the 10-minute floor: still none.
        let s = app.start_reading_session(doc).unwrap();
        app.add_reading_time(s, 300.0, 1500).unwrap();
        assert_eq!(app.reading_velocity_cps().unwrap(), None);

        // Past the floor: 3000 chars over 900s = 3.33 cps.
        app.add_reading_time(s, 600.0, 1500).unwrap();
        let v = app.reading_velocity_cps().unwrap().unwrap();
        assert!((v - 3000.0 / 900.0).abs() < 1e-9);
    }
}
