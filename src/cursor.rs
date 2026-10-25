use tower_lsp_server::ls_types::Position;

// Characters that can be part of a translation key like dash
fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '-'
}

// LSP columns count UTF-16 code units, but Rust slices by bytes.
// This converts one into the other. Returns None if the column is
// past the end of the line or lands in the middle of a character.
fn utf16_to_byte_offset(line: &str, utf16_col: usize) -> Option<usize> {
    let mut utf16_count = 0;
    for (byte_idx, c) in line.char_indices() {
        if utf16_count == utf16_col {
            return Some(byte_idx);
        }
        utf16_count += c.len_utf16();
        if utf16_count > utf16_col {
            return None; // the column found a multi-unit character
        }
    }
    // And if the cursor is at the end of the line
    if utf16_count == utf16_col {
        Some(line.len())
    } else {
        None
    }
}

// Represents the following:
// word: the hovered copy key
// before: the opening parts of the copy key container
// end: the terminating part of the copy key container
pub struct WordContext {
    pub word: String,
    pub before: String,
    pub after: String,
}

pub fn get_word_with_surroundings(content: &str, position: Position) -> Option<WordContext> {
    // The line the user is hovering over
    let line_text = match content.lines().nth(position.line as usize) {
        Some(line) => line,
        None => {
            eprintln!("hover on a line that does not exist");
            return None;
        }
    };

    // Cursor as a byte offset
    let cursor = match utf16_to_byte_offset(line_text, position.character as usize) {
        Some(cursor_point) => cursor_point,
        None => {
            eprintln!("could not provide byte offset");
            return None;
        }
    };

    // Start: track backwards to the first non-key character
    let start = line_text[..cursor]
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_key_char(c))
        .map(|(idx, c)| idx + c.len_utf8()) // word starts after that character
        .unwrap_or(0);
    eprintln!("start text: {}", &line_text[..start]);

    // End: walk forwards to the first non-key character
    let end = line_text[cursor..]
        .find(|c: char| !is_key_char(c))
        .map(|idx| cursor + idx)
        .unwrap_or(line_text.len());
    eprintln!("end text: {}", &line_text[end..]);

    if start < end {
        let result = WordContext {
            word: line_text[start..end].to_string(),
            before: line_text[..start].to_string(),
            after: line_text[end..].to_string(),
        };

        Some(result)
    } else {
        None
    }
}

// Text that comes right before the opening quote of a translation key
const CALL_OPENINGS: &[&str] = &[
    "t(", // t("…"), i18n.t("…"), this.intl.t("…"), $t("…")  (i18next, next-intl, vue-i18n, ember-intl in JS)
    "tc(", // $tc("…")                     (vue-i18n v8, plurals)
    "_(", // $_("…")                      (svelte-i18n)
    "instant(", // translate.instant("…")       (ngx-translate, in TS code)
    "formatMessage({ id: ", // intl.formatMessage({ id: "…" })  (react-intl)
    "formatMessage({id:", // same, without spaces
    "{{t ", // {{t "…"}}                    (ember-intl templates)
    "(t ", // (t "…")                      (ember-intl sub-expression)
    "i18nKey=", // <Trans i18nKey="…">          (react-i18next)
    "v-t=\"", // v-t="'…'"                    (vue-i18n directive)
    "<FormattedMessage id=", // <FormattedMessage id="…">    (react-intl component)
];

// The quotes a key can be wrapped in
const QUOTES: [char; 3] = ['"', '\'', '`'];

// Angular-style pipes: 'app.welcome' | translate
const PIPE_NAMES: &[&str] = &["translate", "transloco"];

// Is the word between `before` and `after` a translation key?
pub fn is_translation_call(before: &str, after: &str) -> bool {
    // 1. The key must sit between two matching quotes
    let Some(quote) = before.chars().last() else {
        return false;
    };
    if !QUOTES.contains(&quote) || !after.starts_with(quote) {
        return false;
    }

    // The test on each side, without the quotes
    let before_quote = &before[..before.len() - quote.len_utf8()]; // e.g `const a = t(`
    let after_quote = &after[quote.len_utf8()..]; // e.g `);`

    // 2. Call style: t("..."), $t("..."), {{t "..."}}, ...e.t.c
    for opening in CALL_OPENINGS {
        if let Some(rest) = before_quote.strip_suffix(opening) {
            // Stop get("...") and split ("...") from matching "t(";
            // the character before the opening cannot be a letter, digit, or ...
            let part_to_a_word = matches!(
                rest.chars().last(),
                Some(c) if c.is_alphanumeric() || c == '_'
            );
            if !part_to_a_word {
                return true;
            }
        }
    }

    // Angular with the pipe :) style: 'app.welcome' | translate
    if let Some(rest) = after_quote.trim_start().strip_prefix('|') {
        let rest = rest.trim_start();
        for pipe in PIPE_NAMES {
            if rest.starts_with(pipe) {
                return true;
            }
        }
    }

    false
}
