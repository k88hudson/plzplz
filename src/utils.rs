use anyhow::Result;
use std::io::Write as _;

pub fn fuzzy_match(query: &str, text: &str) -> bool {
    let query = query.to_lowercase();
    let text = text.to_lowercase();
    let mut chars = query.chars().peekable();
    for c in text.chars() {
        if chars.peek() == Some(&c) {
            chars.next();
        }
    }
    chars.peek().is_none()
}

/// Display width of a string once ANSI escape sequences are stripped.
pub fn visible_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    let mut plain = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // Skip CSI parameter and intermediate bytes up to the final byte
            for c in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&c) {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain.width()
}

/// Number of terminal rows a single logical line occupies once wrapped to
/// `cols`. Assumes deferred wrap, so a line exactly `cols` wide takes one row.
pub fn rows_for_line(s: &str, cols: usize) -> u16 {
    let width = visible_width(s);
    let cols = cols.max(1);
    if width == 0 {
        1
    } else {
        ((width - 1) / cols + 1).min(u16::MAX as usize) as u16
    }
}

/// Buffers the lines of a redrawable prompt and tracks the number of terminal
/// rows they occupy, so the next redraw can move the cursor back far enough
/// even when lines wrap.
struct Frame {
    buf: String,
    rows: u16,
    cols: usize,
}

impl Frame {
    fn new() -> Self {
        let cols = crossterm::terminal::size()
            .map(|(c, _)| c as usize)
            .unwrap_or(80)
            .max(1);
        Self {
            buf: String::new(),
            rows: 0,
            cols,
        }
    }

    fn line(&mut self, s: &str) {
        self.rows = self.rows.saturating_add(rows_for_line(s, self.cols));
        self.buf.push_str(s);
        self.buf.push_str("\r\n");
    }

    /// Erase the previous frame and write this one. Returns the row count to
    /// pass back in on the next call.
    fn flush(self, prev_rows: u16) -> Result<u16> {
        let mut out = std::io::stdout();
        if prev_rows > 0 {
            write!(out, "\x1b[{}A\r\x1b[J", prev_rows)?;
        }
        out.write_all(self.buf.as_bytes())?;
        out.flush()?;
        Ok(self.rows)
    }
}

#[derive(Clone)]
pub struct PickItem {
    pub label: String,
    pub description: String,
    pub preview: Option<String>,
}

pub struct MultiSelectItem {
    pub label: String,
    pub hint: String,
    pub selected: bool,
}

/// Custom multiselect prompt. Enter on an unselected item selects it first,
/// then submits. Returns None on cancel, Some(indices) on submit.
pub fn multiselect(
    prompt: &str,
    items: &mut [MultiSelectItem],
    required: bool,
) -> Result<Option<Vec<usize>>> {
    use crossterm::{event, terminal};

    terminal::enable_raw_mode()?;
    let result = (|| -> Result<Option<Vec<usize>>> {
        let mut cursor: usize = 0;
        let mut prev_lines: u16 = 0;

        let render = |items: &[MultiSelectItem], cursor: usize, prev_lines: u16| -> Result<u16> {
            let mut frame = Frame::new();
            frame.line(&format!("\x1b[36m◆\x1b[0m  {prompt}"));
            for (i, item) in items.iter().enumerate() {
                let hint = if item.hint.is_empty() {
                    String::new()
                } else {
                    format!(" \x1b[2m{}\x1b[0m", item.hint)
                };
                let checkbox = if item.selected {
                    "\x1b[32m◼\x1b[0m"
                } else {
                    "◻"
                };
                if i == cursor {
                    frame.line(&format!(
                        "\x1b[36m│\x1b[0m  {checkbox} \x1b[1m{}\x1b[0m{hint}",
                        item.label
                    ));
                } else {
                    frame.line(&format!(
                        "\x1b[36m│\x1b[0m  {checkbox} {}{hint}",
                        item.label
                    ));
                }
            }
            frame.line("\x1b[36m│\x1b[0m");
            frame.line("\x1b[36m└\x1b[0m  \x1b[2mspace=toggle / enter=confirm / esc=cancel\x1b[0m");
            frame.flush(prev_lines)
        };

        loop {
            prev_lines = render(items, cursor, prev_lines)?;

            let key = loop {
                if let event::Event::Key(key) = event::read()? {
                    break key;
                }
            };

            {
                use event::{KeyCode, KeyModifiers};
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Char(' ') => {
                        items[cursor].selected = !items[cursor].selected;
                    }
                    KeyCode::Enter => {
                        if !items[cursor].selected {
                            items[cursor].selected = true;
                            // Re-render so the user sees the check, then submit
                            render(items, cursor, prev_lines)?;
                        }
                        let selected: Vec<usize> = items
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| item.selected)
                            .map(|(i, _)| i)
                            .collect();
                        if selected.is_empty() && required {
                            continue;
                        }
                        return Ok(Some(selected));
                    }
                    KeyCode::Up => {
                        cursor = cursor.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if !items.is_empty() {
                            cursor = (cursor + 1).min(items.len() - 1);
                        }
                    }
                    _ => {}
                }
            }
        }
    })();

    terminal::disable_raw_mode()?;
    print!("\x1b[J");

    result
}

pub fn pick_from_list(items: &[PickItem], footer_hint: &str) -> Result<Option<usize>> {
    use crossterm::{event, terminal};

    let filtered = |query: &str| -> Vec<usize> {
        if query.is_empty() {
            return (0..items.len()).collect();
        }
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                fuzzy_match(query, &item.label) || fuzzy_match(query, &item.description)
            })
            .map(|(i, _)| i)
            .collect()
    };

    terminal::enable_raw_mode()?;
    let result = (|| -> Result<Option<usize>> {
        let mut query = String::new();
        let mut cursor_idx: usize = 0;
        let mut prev_lines: u16 = 0;

        loop {
            let matches = filtered(&query);
            if cursor_idx >= matches.len() {
                cursor_idx = matches.len().saturating_sub(1);
            }

            let mut frame = Frame::new();
            frame.line(&format!(
                "\x1b[36m◆\x1b[0m  Type the name of a task: \x1b[4m{}\x1b[0m",
                query
            ));

            for (i, &idx) in matches.iter().enumerate() {
                let item = &items[idx];
                let desc = if item.description.is_empty() {
                    String::new()
                } else {
                    format!(" \x1b[2m{}\x1b[0m", item.description)
                };
                if i == cursor_idx {
                    frame.line(&format!(
                        "\x1b[36m│\x1b[0m  \x1b[36m●\x1b[0m \x1b[1m{}\x1b[0m{desc}",
                        item.label
                    ));
                } else {
                    frame.line(&format!("\x1b[36m│\x1b[0m  ○ {}{desc}", item.label));
                }
            }

            if matches.is_empty() {
                frame.line("\x1b[36m│\x1b[0m  \x1b[2mNo matches\x1b[0m");
            }

            if let Some(&idx) = matches.get(cursor_idx)
                && let Some(ref preview) = items[idx].preview
            {
                frame.line("\x1b[36m│\x1b[0m");
                for line in preview.lines() {
                    frame.line(&format!("\x1b[36m│\x1b[0m  \x1b[33m{line}\x1b[0m"));
                }
            }

            frame.line("\x1b[36m│\x1b[0m");
            frame.line(&format!("\x1b[36m└\x1b[0m  \x1b[2m{footer_hint}\x1b[0m"));

            prev_lines = frame.flush(prev_lines)?;

            let key = loop {
                if let event::Event::Key(key) = event::read()? {
                    break key;
                }
            };

            {
                use event::{KeyCode, KeyModifiers};
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => {
                        if let Some(&idx) = matches.get(cursor_idx) {
                            return Ok(Some(idx));
                        }
                    }
                    KeyCode::Up => {
                        cursor_idx = cursor_idx.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if !matches.is_empty() {
                            cursor_idx = (cursor_idx + 1).min(matches.len() - 1);
                        }
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        cursor_idx = 0;
                    }
                    KeyCode::Char(c) => {
                        query.push(c);
                        cursor_idx = 0;
                    }
                    _ => {}
                }
            }
        }
    })();

    terminal::disable_raw_mode()?;
    print!("\x1b[J");

    result
}
