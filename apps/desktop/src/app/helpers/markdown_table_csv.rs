//! A Markdown table's text as CSV, shared by the chat's table actions and the Files editor's.

/// The cells of one GFM table row, with the escaped pipes put back.
fn table_row_cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut characters = line.trim().trim_matches('|').chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&'|') => {
                characters.next();
                if let Some(cell) = cells.last_mut() {
                    cell.push('|');
                }
            }
            '|' => cells.push(String::new()),
            _ => {
                if let Some(cell) = cells.last_mut() {
                    cell.push(character);
                }
            }
        }
    }
    cells
        .into_iter()
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// A cell's words with the inline markers taken off, which is what a spreadsheet wants.
///
/// React reads the rendered cell instead (session-chat-table-clipboard.ts (deleted 2026-10-01)), because its chips know
/// their own source text; the native table renders the Markdown itself, so the source is read here.
fn table_cell_text(cell: &str) -> String {
    let mut text = cell.replace("**", "").replace('`', "");
    while let Some(open) = text.find("](") {
        let Some(label_start) = text[..open].rfind('[') else {
            break;
        };
        let Some(close) = text[open + 2..].find(')') else {
            break;
        };
        let label = text[label_start + 1..open].to_owned();
        text.replace_range(label_start..open + 2 + close + 1, &label);
    }
    text
}

/// GFM's delimiter row, the one line of a table that is punctuation rather than content.
fn is_table_delimiter_row(line: &str) -> bool {
    let body = line.trim();
    !body.is_empty()
        && body
            .chars()
            .all(|character| matches!(character, '|' | '-' | ':' | ' '))
        && body.contains('-')
}

/// React's `sessionChatTableToCsv`: the cells as they read, quoted only when they have to be.
pub(crate) fn table_csv(source: &str) -> String {
    source
        .lines()
        .filter(|line| line.contains('|') && !is_table_delimiter_row(line))
        .map(|line| {
            table_row_cells(line)
                .iter()
                .map(|cell| {
                    let value = table_cell_text(cell);
                    if value.contains(['"', '\n', ',']) {
                        format!("\"{}\"", value.replace('"', "\"\""))
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
