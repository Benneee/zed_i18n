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

// Things are private to their files unless marked with pub
pub fn get_word_at_position(content: &str, position: Position) -> Option<&str> {
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

    // End: walk forwards to the first non-key character
    let end = line_text[cursor..]
        .find(|c: char| !is_key_char(c))
        .map(|idx| cursor + idx)
        .unwrap_or(line_text.len());

    if start < end {
        Some(&line_text[start..end])
    } else {
        None
    }
}
