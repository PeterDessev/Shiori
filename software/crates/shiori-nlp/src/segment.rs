//! Sentence and paragraph segmentation.
//!
//! Pure text processing, independent of the morphological analyzer, so it is
//! cheap to test exhaustively.

/// Characters that terminate a Japanese sentence.
const SENTENCE_ENDERS: &[char] = &['。', '！', '？', '!', '?', '．'];

/// Closing brackets/quotes that should stay attached to the sentence they
/// close, e.g. 「そうか。」 keeps the 」 with the sentence.
const TRAILING_CLOSERS: &[char] = &['」', '』', '）', ')', '”', '"', '’'];

/// Quote pairs inside which sentence enders do not split, so that
/// 「行くの？」と聞いた stays a single sentence.
const OPENERS: &[char] = &['「', '『', '（', '('];
const CLOSERS: &[char] = &['」', '』', '）', ')'];

/// Split text into paragraphs on newline runs. Empty/whitespace-only
/// paragraphs are dropped; paragraph text is trimmed.
pub fn split_paragraphs(text: &str) -> Vec<&str> {
    text.split(['\n', '\r'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Split one paragraph into sentences.
///
/// A sentence ends at 。！？!?． (plus any directly repeated enders, so
/// 「えっ！？」 and 「…。。」 stay together), followed by any closing
/// quotes/brackets. Enders inside 「」『』（） do not split. Trailing text
/// without a final ender still forms a sentence.
pub fn split_sentences(paragraph: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    let mut chars = paragraph.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if OPENERS.contains(&c) {
            depth += 1;
        } else if CLOSERS.contains(&c) {
            depth = (depth - 1).max(0);
        } else if depth == 0 && SENTENCE_ENDERS.contains(&c) {
            // Absorb repeated enders (！？, 。。。) and trailing closers.
            let mut end = i + c.len_utf8();
            while let Some(&(j, next)) = chars.peek() {
                if SENTENCE_ENDERS.contains(&next) || TRAILING_CLOSERS.contains(&next) {
                    end = j + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let sentence = paragraph[start..end].trim();
            if !sentence.is_empty() {
                sentences.push(sentence);
            }
            start = end;
        }
    }

    let rest = paragraph[start..].trim();
    if !rest.is_empty() {
        sentences.push(rest);
    }
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_paragraphs_on_newlines() {
        let text = "一段落目。\n\n二段落目。\r\n三段落目。\n   \n";
        assert_eq!(
            split_paragraphs(text),
            vec!["一段落目。", "二段落目。", "三段落目。"]
        );
    }

    #[test]
    fn splits_basic_sentences() {
        assert_eq!(
            split_sentences("猫が好きだ。犬も好きだ。"),
            vec!["猫が好きだ。", "犬も好きだ。"]
        );
    }

    #[test]
    fn keeps_trailing_text_without_ender() {
        assert_eq!(
            split_sentences("これは最初。これは途中"),
            vec!["これは最初。", "これは途中"]
        );
    }

    #[test]
    fn does_not_split_inside_quotes() {
        assert_eq!(
            split_sentences("「行くの？」と聞いた。"),
            vec!["「行くの？」と聞いた。"]
        );
        assert_eq!(
            split_sentences("『そうか。なるほど。』と思った。"),
            vec!["『そうか。なるほど。』と思った。"]
        );
    }

    #[test]
    fn quoted_speech_stays_with_following_clause() {
        // Convention: enders inside quotes never split, so a quotation and
        // the text that follows it form one sentence until the next ender
        // outside the quotes. A trailing quotation forms its own sentence.
        assert_eq!(
            split_sentences("彼は言った。「もう帰る。」それで終わった。"),
            vec!["彼は言った。", "「もう帰る。」それで終わった。"]
        );
        assert_eq!(
            split_sentences("彼は言った。「もう帰る。」"),
            vec!["彼は言った。", "「もう帰る。」"]
        );
    }

    #[test]
    fn groups_repeated_enders() {
        assert_eq!(
            split_sentences("なんだって！？嘘だろ。"),
            vec!["なんだって！？", "嘘だろ。"]
        );
    }

    #[test]
    fn handles_empty_and_whitespace() {
        assert!(split_sentences("").is_empty());
        assert!(split_sentences("   ").is_empty());
        assert!(split_paragraphs("\n\n\n").is_empty());
    }

    #[test]
    fn unbalanced_closer_does_not_underflow() {
        assert_eq!(split_sentences("」変だ。次。"), vec!["」変だ。", "次。"]);
    }
}
