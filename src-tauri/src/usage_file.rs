//! Your own usage meters from a plain CSV file (Settings → Usage file; default ~/.cue/usage.csv), shown
//! in the usage pill next to Claude Code's: a proxy's budget in dollars, credits, tokens… Whatever
//! writes the file (a cron job, a script, a tool) is yours; Cue only reads it, when it changes, and
//! never runs anything. A file that doesn't match the format shows no meters, only what's wrong.
//!
//!   label,spent,limit,unit,resets_at
//!   Team,12.40,50,$,2026-10-12T00:00:00Z
//!
//! `spent` is required; the rest may be left out or empty (no limit: no bar; unit defaults to "$";
//! `resets_at` is an ISO date-time or Unix milliseconds). One to three rows.

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::SystemTime;

const MAX_ROWS: usize = 3;
const COLUMNS: [&str; 5] = ["label", "spent", "limit", "unit", "resets_at"];

/// The file Settings points at, `~` expanded.
pub fn path() -> std::path::PathBuf {
    let p = crate::config::usage_file();
    match p.strip_prefix("~/") {
        Some(rest) => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => std::path::PathBuf::from(p),
    }
}

/// Last read, by the file's path and modification time (it's re-read only when it changed).
static LAST: Mutex<Option<(std::path::PathBuf, Option<SystemTime>, Value)>> = Mutex::new(None);

/// What the usage pill shows: { meters: [{label, spent, limit, unit, resets_ms}], as_of_ms, error, path }.
/// No file: no meters and no error.
pub fn read() -> Value {
    let p = path();
    let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    let mut last = LAST.lock().unwrap();
    if let Some((lp, lm, v)) = last.as_ref() {
        if *lp == p && *lm == modified {
            return v.clone();
        }
    }
    let shown = p.to_string_lossy().replace(&std::env::var("HOME").unwrap_or_default(), "~");
    let v = match (modified, std::fs::read_to_string(&p)) {
        (None, _) => json!({ "meters": [], "path": shown }),
        (Some(m), Ok(text)) => {
            let as_of = m.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            match parse(&text) {
                Ok(meters) => json!({ "meters": meters, "as_of_ms": as_of, "path": shown }),
                Err(e) => json!({ "meters": [], "as_of_ms": as_of, "error": e, "path": shown }),
            }
        }
        (Some(_), Err(e)) => json!({ "meters": [], "error": format!("Can't read it: {e}"), "path": shown }),
    };
    *last = Some((p, modified, v.clone()));
    v
}

/// Whether the file changed since the last read (checked every few seconds; the window then redraws).
pub fn changed() -> bool {
    let p = path();
    let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    LAST.lock().unwrap().as_ref().is_none_or(|(lp, lm, _)| *lp != p || *lm != modified)
}

/// The rows as meters, or what's wrong with the file (the first problem, with its line).
fn parse(text: &str) -> Result<Vec<Value>, String> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim())).filter(|(_, l)| !l.is_empty()).collect();
    let Some(&(_, header)) = lines.first() else { return Err("The file is empty.".into()) };
    let cols: Vec<String> = header.split(',').map(|c| c.trim().to_lowercase()).collect();
    for c in &cols {
        if !COLUMNS.contains(&c.as_str()) {
            return Err(format!("Line 1: \"{c}\" isn't a column Cue knows (label, spent, limit, unit, resets_at)."));
        }
    }
    if !cols.iter().any(|c| c == "spent") {
        return Err("Line 1: there's no \"spent\" column.".into());
    }
    let rows = &lines[1..];
    if rows.is_empty() {
        return Err("There's a header but no rows.".into());
    }
    if rows.len() > MAX_ROWS {
        return Err(format!("{} rows: Cue shows at most {MAX_ROWS}.", rows.len()));
    }
    let mut meters = vec![];
    for &(n, line) in rows {
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        if cells.len() != cols.len() {
            return Err(format!("Line {n}: {} values, but the header has {} columns.", cells.len(), cols.len()));
        }
        let get = |name: &str| cols.iter().position(|c| c == name).map(|i| cells[i]).unwrap_or("");
        let number = |name: &str, v: &str| -> Result<f64, String> {
            v.parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0).ok_or(format!("Line {n}: \"{name}\" isn't a number ({v:?})."))
        };
        let spent = number("spent", get("spent"))?;
        let limit = match get("limit") {
            "" => None,
            v => Some(number("limit", v)?).filter(|l| *l > 0.0),
        };
        let unit = match get("unit") {
            "" => "$",
            u if u.chars().count() <= 12 => u,
            _ => return Err(format!("Line {n}: \"unit\" is longer than 12 characters.")),
        };
        let label = get("label");
        if label.chars().count() > 30 {
            return Err(format!("Line {n}: \"label\" is longer than 30 characters."));
        }
        let resets_ms = match get("resets_at") {
            "" => None,
            v if v.chars().all(|c| c.is_ascii_digit()) => v.parse::<u64>().ok(),
            v => Some(crate::transcript::iso_ms(v).ok_or(format!("Line {n}: \"resets_at\" isn't a date and time (like 2026-10-12T00:00:00Z) ({v:?})."))?),
        };
        meters.push(json!({ "label": label, "spent": spent, "limit": limit, "unit": unit, "resets_ms": resets_ms }));
    }
    Ok(meters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_become_meters() {
        let m = parse("label,spent,limit,unit,resets_at\nTeam,12.40,50,$,2026-10-12T00:00:00Z\nMonth, 48 ,,credits,\n").unwrap();
        assert_eq!(m[0]["label"], "Team");
        assert_eq!(m[0]["spent"], 12.4);
        assert_eq!(m[0]["limit"], 50.0);
        assert!(m[0]["resets_ms"].as_u64().unwrap() > 0);
        assert_eq!(m[1]["limit"], Value::Null, "an empty limit: no bar");
        assert_eq!(m[1]["unit"], "credits");
        // Only `spent` is needed; the unit defaults to dollars.
        let m = parse("spent\n3.5\n").unwrap();
        assert_eq!((m[0]["spent"].as_f64(), m[0]["unit"].as_str()), (Some(3.5), Some("$")));
    }

    #[test]
    fn a_file_that_doesnt_match_says_why() {
        let err = |t: &str| parse(t).unwrap_err();
        assert!(err("").contains("empty"));
        assert!(err("label,limit\nx,5\n").contains("no \"spent\""));
        assert!(err("label,spent,cost\nx,1,2\n").contains("\"cost\" isn't a column"));
        assert!(err("label,spent\nTeam,12,extra\n").contains("Line 2: 3 values"));
        assert!(err("label,spent\nTeam,lots\n").contains("Line 2: \"spent\" isn't a number"));
        assert!(err("spent,resets_at\n1,next monday\n").contains("\"resets_at\" isn't a date"));
        assert!(err("spent\n1\n2\n3\n4\n").contains("at most 3"));
    }
}
