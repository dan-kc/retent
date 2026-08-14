//! Narrow GitHub-style table tokenization for history blocks.

pub(super) fn cells(line: &str) -> Vec<&str> {
    let mut line = line.trim();
    if let Some(rest) = line.strip_prefix('|') {
        line = rest;
    }
    if let Some(rest) = line.strip_suffix('|') {
        line = rest;
    }
    line.split('|').map(str::trim).collect()
}

pub(super) fn valid_separator(cells: &[&str]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let core = cell
                .strip_prefix(':')
                .unwrap_or(cell)
                .strip_suffix(':')
                .unwrap_or(cell.strip_prefix(':').unwrap_or(cell));
            core.len() >= 3 && core.bytes().all(|byte| byte == b'-')
        })
}
