//! Word frequency ranks.
//!
//! The source format is one word per line, most frequent first; the rank of
//! a word is its 1-based line number. This matches the Leeds-corpus-derived
//! list the downloader fetches.

use std::collections::HashMap;

/// Word → frequency rank (1 = most frequent).
#[derive(Debug, Clone, Default)]
pub struct FrequencyList {
    ranks: HashMap<String, u32>,
}

impl FrequencyList {
    /// Parse the one-word-per-line format. Blank lines are skipped without
    /// disturbing subsequent ranks (rank = position among non-blank lines).
    pub fn parse(text: &str) -> Self {
        let mut ranks = HashMap::new();
        let mut rank = 0u32;
        for line in text.lines() {
            let word = line.trim();
            if word.is_empty() {
                continue;
            }
            rank += 1;
            // First occurrence wins: best (lowest) rank.
            ranks.entry(word.to_string()).or_insert(rank);
        }
        Self { ranks }
    }

    /// Rank of a word, if present (1 = most frequent).
    pub fn rank(&self, word: &str) -> Option<u32> {
        self.ranks.get(word).copied()
    }

    pub fn len(&self) -> usize {
        self.ranks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranks.is_empty()
    }

    /// Iterate over all (word, rank) pairs in arbitrary order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u32)> {
        self.ranks.iter().map(|(w, r)| (w.as_str(), *r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_ranks() {
        let list = FrequencyList::parse("の\nに\n\nは\n食べる\n");
        assert_eq!(list.rank("の"), Some(1));
        assert_eq!(list.rank("に"), Some(2));
        assert_eq!(list.rank("は"), Some(3), "blank lines must not shift ranks");
        assert_eq!(list.rank("食べる"), Some(4));
        assert_eq!(list.rank("不在"), None);
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn duplicate_words_keep_best_rank() {
        let list = FrequencyList::parse("猫\n犬\n猫\n");
        assert_eq!(list.rank("猫"), Some(1));
        assert_eq!(list.rank("犬"), Some(2));
    }

    #[test]
    fn empty_input() {
        let list = FrequencyList::parse("");
        assert!(list.is_empty());
        assert_eq!(list.rank("猫"), None);
    }
}
