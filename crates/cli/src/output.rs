//! Table and JSON rendering. Commands build a [`Table`] and hand it to
//! [`emit`]; `--json` bypasses the table entirely.

use serde::Serialize;

pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn row<I, S>(&mut self, cells: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows.push(cells.into_iter().map(Into::into).collect());
    }

    pub fn print(&self) {
        if self.rows.is_empty() {
            println!("(no rows)");
            return;
        }

        let mut widths = vec![0usize; self.headers.len()];
        for (i, header) in self.headers.iter().enumerate() {
            widths[i] = display_width(header);
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(display_width(cell));
                }
            }
        }

        let mut line = String::new();
        for (i, header) in self.headers.iter().enumerate() {
            line.push_str(&pad(header, widths[i]));
            if i + 1 < self.headers.len() {
                line.push_str("  ");
            }
        }
        println!("{}", line.trim_end());

        for row in &self.rows {
            let mut line = String::new();
            for (i, cell) in row.iter().enumerate() {
                if i + 1 == row.len() {
                    line.push_str(cell);
                } else {
                    line.push_str(&pad(cell, widths.get(i).copied().unwrap_or(0)));
                    line.push_str("  ");
                }
            }
            println!("{}", line.trim_end());
        }
    }
}

/// Prints either the table or the JSON form of `value`.
pub fn emit<T: Serialize>(json: bool, table: &Table, value: &T) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        table.print();
    }
    Ok(())
}

pub fn emit_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

fn pad(cell: &str, width: usize) -> String {
    let mut out = String::from(cell);
    for _ in display_width(cell)..width {
        out.push(' ');
    }
    out
}

/// Good enough for ASCII + CJK: wide characters occupy two columns.
fn display_width(cell: &str) -> usize {
    cell.chars()
        .map(|c| if is_wide(c) { 2 } else { 1 })
        .sum::<usize>()
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6)
}
