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
    use std::io::stdout;

    terminal::enable_raw_mode()?;
    let result = (|| -> Result<Option<Vec<usize>>> {
        let mut cursor: usize = 0;
        let mut prev_lines: u16 = 0;

        let render = |items: &[MultiSelectItem], cursor: usize, prev_lines: u16| -> Result<u16> {
            let mut out = stdout();
            if prev_lines > 0 {
                write!(out, "\x1b[{}A\r", prev_lines)?;
                write!(out, "\x1b[J")?;
            }
            let mut lines: u16 = 0;
            write!(out, "\x1b[36m◆\x1b[0m  {prompt}\r\n")?;
            lines += 1;
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
                    write!(
                        out,
                        "\x1b[36m│\x1b[0m  {checkbox} \x1b[1m{}\x1b[0m{hint}\r\n",
                        item.label
                    )?;
                } else {
                    write!(out, "\x1b[36m│\x1b[0m  {checkbox} {}{hint}\r\n", item.label)?;
                }
                lines += 1;
            }
            write!(out, "\x1b[36m│\x1b[0m\r\n")?;
            write!(
                out,
                "\x1b[36m└\x1b[0m  \x1b[2mspace=toggle / enter=confirm / esc=cancel\x1b[0m\r\n"
            )?;
            lines += 2;
            out.flush()?;
            Ok(lines)
        };

        loop {
            prev_lines = render(items, cursor, prev_lines)?;

            if let event::Event::Key(key) = event::read()? {
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
    use std::io::stdout;

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

            let mut out = stdout();

            if prev_lines > 0 {
                write!(out, "\x1b[{}A\r", prev_lines)?;
                write!(out, "\x1b[J")?;
            }

            let mut lines: u16 = 0;

            write!(
                out,
                "\x1b[36m◆\x1b[0m  Type the name of a task: \x1b[4m{}\x1b[0m\r\n",
                query
            )?;
            lines += 1;

            for (i, &idx) in matches.iter().enumerate() {
                let item = &items[idx];
                let desc = if item.description.is_empty() {
                    String::new()
                } else {
                    format!(" \x1b[2m{}\x1b[0m", item.description)
                };
                if i == cursor_idx {
                    write!(
                        out,
                        "\x1b[36m│\x1b[0m  \x1b[36m●\x1b[0m \x1b[1m{}\x1b[0m{desc}\r\n",
                        item.label
                    )?;
                } else {
                    write!(out, "\x1b[36m│\x1b[0m  ○ {}{desc}\r\n", item.label)?;
                }
                lines += 1;
            }

            if matches.is_empty() {
                write!(out, "\x1b[36m│\x1b[0m  \x1b[2mNo matches\x1b[0m\r\n")?;
                lines += 1;
            }

            if let Some(&idx) = matches.get(cursor_idx)
                && let Some(ref preview) = items[idx].preview
            {
                write!(out, "\x1b[36m│\x1b[0m\r\n")?;
                lines += 1;
                for line in preview.lines() {
                    write!(out, "\x1b[36m│\x1b[0m  \x1b[33m{line}\x1b[0m\r\n")?;
                    lines += 1;
                }
            }

            write!(out, "\x1b[36m│\x1b[0m\r\n")?;
            write!(out, "\x1b[36m└\x1b[0m  \x1b[2m{footer_hint}\x1b[0m\r\n")?;
            lines += 2;

            out.flush()?;
            prev_lines = lines;

            if let event::Event::Key(key) = event::read()? {
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
