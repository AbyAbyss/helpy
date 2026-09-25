//! Turning streamed Markdown answers into text worth reading aloud.

/// Strips Markdown so a voice doesn't read symbols: emphasis, code, headings,
/// links (keeps their text), and code blocks (dropped entirely).
pub fn speakable(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_code_block = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        let line = trimmed.trim_start_matches('#').trim_start();
        let line = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .unwrap_or(line);
        out.push_str(&strip_inline(line));
        out.push('\n');
    }
    out.trim().to_string()
}

fn strip_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' | '_' | '`' => {}
            // [text](url) → text
            '[' => {
                if let Some(close) = chars[i..].iter().position(|c| *c == ']').map(|p| p + i) {
                    if chars.get(close + 1) == Some(&'(') {
                        if let Some(end) = chars[close..]
                            .iter()
                            .position(|c| *c == ')')
                            .map(|p| p + close)
                        {
                            out.extend(&chars[i + 1..close]);
                            i = end + 1;
                            continue;
                        }
                    }
                }
                out.push('[');
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// For "read step instructions only": the numbered or bulleted items of an
/// answer, or its first paragraph when it has no list.
pub fn steps_only(markdown: &str) -> String {
    let steps: Vec<String> = markdown
        .lines()
        .map(str::trim_start)
        .filter(|l| is_list_item(l))
        .map(speakable)
        .collect();
    if !steps.is_empty() {
        return steps.join("\n");
    }
    speakable(markdown.split("\n\n").next().unwrap_or_default())
}

fn is_list_item(line: &str) -> bool {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && line[digits..].starts_with(". "))
        || line.starts_with("- ")
        || line.starts_with("* ")
}

/// Splits streaming text into sentences, so speech can start before the
/// whole answer has arrived. List items and paragraphs also end a sentence.
#[derive(Default)]
pub struct SentenceSplitter {
    buf: String,
}

impl SentenceSplitter {
    pub fn push(&mut self, piece: &str) -> Vec<String> {
        self.buf.push_str(piece);
        let mut out = Vec::new();
        while let Some(end) = self.boundary() {
            let sentence: String = self.buf.drain(..end).collect();
            let s = speakable(&sentence);
            if !s.is_empty() {
                out.push(s);
            }
        }
        out
    }

    /// Whatever is left once the answer is complete.
    pub fn finish(&mut self) -> Option<String> {
        let rest = speakable(&std::mem::take(&mut self.buf));
        (!rest.is_empty()).then_some(rest)
    }

    pub fn reset(&mut self) {
        self.buf.clear();
    }

    /// Byte index just after the first complete sentence, if any.
    fn boundary(&self) -> Option<usize> {
        let b = self.buf.as_bytes();
        let mut in_code = false;
        for i in 0..b.len() {
            // Bytes, not str: `i` can be inside a multi-byte character.
            if b[i..].starts_with(b"```") {
                in_code = !in_code;
            }
            if in_code {
                continue;
            }
            if b[i] == b'\n' {
                return Some(i + 1);
            }
            // ". " and friends end a sentence, but not "e.g. " or "1. ".
            if matches!(b[i], b'.' | b'!' | b'?') && b.get(i + 1) == Some(&b' ') {
                let word_start = self.buf[..i].rfind(' ').map_or(0, |p| p + 1);
                let word = &self.buf[word_start..i];
                let is_number = !word.is_empty() && word.chars().all(|c| c.is_ascii_digit());
                if !is_number && !["e.g", "i.e", "etc", "vs", "Mr", "Mrs", "Dr"].contains(&word) {
                    return Some(i + 2);
                }
            }
        }
        None
    }
}

/// Normalises text for wake-phrase matching: lowercase letters and digits,
/// single spaces.
fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push(
                (prev[j] + usize::from(ca != cb))
                    .min(prev[j + 1] + 1)
                    .min(cur[j] + 1),
            );
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Whether a transcript contains the wake phrase, allowing a small spelling
/// slip ("hey helpie"). Compares against every run of words the phrase's length.
pub fn heard_wake_phrase(transcript: &str, phrase: &str) -> bool {
    let phrase = normalise(phrase);
    let heard = normalise(transcript);
    if phrase.is_empty() || heard.is_empty() {
        return false;
    }
    let n = phrase.split(' ').count();
    let words: Vec<&str> = heard.split(' ').collect();
    let allowed = (phrase.chars().count() / 6).max(1);
    words
        .windows(n.min(words.len()))
        .any(|w| edit_distance(&w.join(" "), &phrase) <= allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown_for_speech() {
        let md = "## Steps\n1. Open **Junk Email** in the `folder list`.\n- See [the guide](https://x.y) first\n```\ncode here\n```\nDone.";
        assert_eq!(
            speakable(md),
            "Steps\n1. Open Junk Email in the folder list.\nSee the guide first\nDone."
        );
    }

    #[test]
    fn steps_only_reads_list_items_or_the_first_paragraph() {
        let md = "Here's how.\n\n1. Open Outlook.\n2. Click **Junk Email**.\n\nThat's it.";
        assert_eq!(steps_only(md), "1. Open Outlook.\n2. Click Junk Email.");
        assert_eq!(
            steps_only("Spam lives in Junk Email.\n\nMore detail here."),
            "Spam lives in Junk Email."
        );
    }

    #[test]
    fn splits_text_with_emoji_and_other_multibyte_characters() {
        let mut s = SentenceSplitter::default();
        // Streamed piece by piece, as a model sends it.
        let mut out = s.push("😊");
        out.extend(s.push(" Glad I could help! Café é"));
        out.extend(s.push("tait 🎉 done. Next"));
        assert_eq!(out, vec!["😊 Glad I could help!", "Café était 🎉 done."]);
        assert_eq!(s.finish().as_deref(), Some("Next"));
    }

    #[test]
    fn splits_streamed_text_into_sentences() {
        let mut s = SentenceSplitter::default();
        assert!(s.push("Open the folder").is_empty());
        assert_eq!(s.push(" list. Then click"), vec!["Open the folder list."]);
        assert_eq!(
            s.push(" it, e.g. with the mouse. Done!"),
            vec!["Then click it, e.g. with the mouse."]
        );
        assert_eq!(s.finish(), Some("Done!".to_string()));
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn numbered_steps_and_code_blocks_are_not_cut_mid_way() {
        let mut s = SentenceSplitter::default();
        assert_eq!(s.push("1. Open Outlook\n2. Click"), vec!["1. Open Outlook"]);
        assert_eq!(s.push(" Junk Email\n"), vec!["2. Click Junk Email"]);
        let mut s = SentenceSplitter::default();
        assert!(s.push("```\nx. y\n").is_empty());
    }

    #[test]
    fn wake_phrase_matching_tolerates_small_slips() {
        assert!(heard_wake_phrase("Hey, Helpy!", "hey helpy"));
        assert!(heard_wake_phrase(
            "okay so hey helpi what's this",
            "hey helpy"
        ));
        assert!(!heard_wake_phrase("hello there", "hey helpy"));
        // Two slips is too many: ordinary speech mustn't wake Helpy.
        assert!(!heard_wake_phrase("hey hello", "hey helpy"));
        assert!(!heard_wake_phrase("", "hey helpy"));
        assert!(heard_wake_phrase("computer", "computer"));
    }
}
