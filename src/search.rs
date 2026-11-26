//! Bounded, local project search using ripgrep's MIT-licensed libraries.
use std::path::Path;
use std::time::{Duration, Instant};
use anyhow::{bail, Context, Result};
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{sinks::UTF8, Searcher};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use crate::project::{Project, MAX_TEXT_FILE_BYTES};

const MAX_RESULTS: usize = 300;
const MAX_FILES: usize = 10_000;
const MAX_PATTERN: usize = 1024;
const MAX_DURATION: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOverlay { pub path: String, pub content: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit { pub file: String, pub line: u64, pub column: usize, pub preview: String }

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReport { pub hits: Vec<SearchHit>, pub files_searched: usize, pub skipped: usize, pub truncated: bool }

fn searchable(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "tex" | "ltx" | "bib" | "sty" | "cls" | "bst" | "bbx" | "cbx" | "txt" | "md" | "dtx" | "ins" | "cfg" | "def" | "clo" | "json" | "yaml" | "yml" | "csv" | "tsv" | "lua" | "py" | "r" | "m" | "tikz" | "pgf" | "dat" | "asy")
}

fn preview(line: &str, byte_start: usize) -> String {
    let before = line[..byte_start].chars().count();
    let start = before.saturating_sub(120);
    let piece: String = line.chars().skip(start).take(400).collect();
    let end_truncated = line.chars().count() > start + 400;
    format!("{}{}{}", if start > 0 { "…" } else { "" }, piece, if end_truncated { "…" } else { "" })
}

/// Searches supported text files inside `project`. Symlinks and build/internal directories
/// are never followed. An optional live editor buffer replaces its on-disk counterpart.
pub fn search_project<F: Fn() -> bool>(
    project: &Project, query: &str, is_regex: bool, case_sensitive: bool,
    overlay: Option<SearchOverlay>, cancelled: F,
) -> Result<SearchReport> {
    if query.is_empty() || query.len() > MAX_PATTERN { bail!("Search text must be between 1 and {MAX_PATTERN} bytes"); }
    if let Some(ref o) = overlay {
        project.resolve(&o.path).context("invalid editor buffer path")?;
        if o.content.len() > MAX_TEXT_FILE_BYTES as usize { bail!("The current editor buffer is too large to search"); }
    }
    let matcher = RegexMatcherBuilder::new().fixed_strings(!is_regex).case_insensitive(!case_sensitive)
        .size_limit(5_000_000).dfa_size_limit(5_000_000).build(query)
        .context("Invalid or too complex search pattern")?;
    let started = Instant::now();
    let mut report = SearchReport::default();
    let mut walker = WalkBuilder::new(project.root());
    walker.hidden(false).parents(false).ignore(false).git_ignore(false).git_exclude(false)
        .follow_links(false).max_depth(Some(17)).filter_entry(|entry| {
            if entry.depth() == 0 { return true; }
            let name = entry.file_name().to_string_lossy();
            !entry.file_type().is_some_and(|t| t.is_dir()) ||
                ![".git", ".texcraft", "node_modules", "target"].iter().any(|n| name.eq_ignore_ascii_case(n))
        });
    for entry in walker.build() {
        if cancelled() { bail!("Search cancelled"); }
        if started.elapsed() > MAX_DURATION || report.files_searched >= MAX_FILES || report.hits.len() >= MAX_RESULTS {
            report.truncated = true; break;
        }
        let entry = match entry { Ok(e) => e, Err(_) => { report.skipped += 1; continue; } };
        if !entry.file_type().is_some_and(|t| t.is_file()) || !searchable(entry.path()) { continue; }
        let relative = project.relative(entry.path());
        let path = match project.resolve(&relative) { Ok(p) => p, Err(_) => { report.skipped += 1; continue; } };
        // Reject links even if they point inside the project; never search through them.
        if std::fs::symlink_metadata(entry.path()).map(|m| !m.file_type().is_file() || m.len() > MAX_TEXT_FILE_BYTES).unwrap_or(true) {
            report.skipped += 1; continue;
        }
        let bytes = if let Some(ref o) = overlay.as_ref().filter(|o| o.path == relative) {
            o.content.as_bytes().to_vec()
        } else { match std::fs::read(&path) { Ok(b) => b, Err(_) => { report.skipped += 1; continue; } } };
        if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() { report.skipped += 1; continue; }
        report.files_searched += 1;
        let mut searcher = Searcher::new();
        let file = relative.clone();
        searcher.search_slice(&matcher, &bytes, UTF8(|line_number, line| {
            if cancelled() { return Ok(false); }
            if let Some(found) = matcher.find(line.as_bytes()).map_err(std::io::Error::other)? {
                let line = line.trim_end_matches(['\r', '\n']);
                let start = found.start().min(line.len());
                if !line.is_char_boundary(start) { return Ok(true); }
                report.hits.push(SearchHit {
                    file: file.clone(), line: line_number,
                    column: line[..start].encode_utf16().count() + 1,
                    preview: preview(line, start),
                });
            }
            Ok(report.hits.len() < MAX_RESULTS && started.elapsed() <= MAX_DURATION)
        })).context("search failed")?;
    }
    if cancelled() { bail!("Search cancelled"); }
    if report.hits.len() >= MAX_RESULTS || started.elapsed() > MAX_DURATION { report.truncated = true; }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> (std::path::PathBuf, Project) {
        let dir = std::env::temp_dir().join(format!("localleaf-search-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("chapters")).unwrap();
        std::fs::create_dir_all(dir.join(".texcraft")).unwrap();
        std::fs::write(dir.join("main.tex"), "😀 equation alpha\nSecond ALPHA\n").unwrap();
        std::fs::write(dir.join("chapters/intro.tex"), "alpha beta\n").unwrap();
        std::fs::write(dir.join(".texcraft/secret.tex"), "alpha\n").unwrap();
        let project = Project::open(&dir).unwrap();
        (dir, project)
    }
    #[test]
    fn matches_case_regex_overlay_and_utf16_column() {
        let (dir, p) = fixture();
        let r = search_project(&p, "alpha", false, false, None, || false).unwrap();
        assert_eq!(r.hits.len(), 3);
        let hit = r.hits.iter().find(|h| h.file == "main.tex" && h.line == 1).unwrap();
        assert_eq!(hit.column, 13); // emoji occupies two UTF-16 units
        let o = SearchOverlay { path: "main.tex".into(), content: "beta live\n".into() };
        let r = search_project(&p, "^beta", true, true, Some(o), || false).unwrap();
        assert_eq!(r.hits.len(), 1); assert_eq!(r.hits[0].file, "main.tex");
        assert!(search_project(&p, "[", true, false, None, || false).is_err());
        assert!(search_project(&p, "a", false, false, None, || true).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn respects_case_and_reports_result_limits() {
        let (dir, p) = fixture();
        let upper = search_project(&p, "ALPHA", false, true, None, || false).unwrap();
        assert_eq!(upper.hits.len(), 1);
        assert_eq!(upper.hits[0].line, 2);
        std::fs::write(dir.join("many.tex"), "alpha\n".repeat(350)).unwrap();
        let limited = search_project(&p, "alpha", false, false, None, || false).unwrap();
        assert_eq!(limited.hits.len(), 300);
        assert!(limited.truncated);
        std::fs::write(dir.join("binary.tex"), b"alpha\0beta").unwrap();
        let r = search_project(&p, "beta", false, false, None, || false).unwrap();
        assert_eq!(r.hits.len(), 1); // chapters/intro.tex, not binary.tex
        assert!(r.skipped >= 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ignores_symlinks_and_rejects_outside_overlay() {
        let (dir, p) = fixture();
        std::os::unix::fs::symlink("/etc/passwd", dir.join("external.tex")).unwrap();
        let r = search_project(&p, "root", false, false, None, || false).unwrap();
        assert!(r.hits.is_empty());
        let o = SearchOverlay { path: "../external.tex".into(), content: "root".into() };
        assert!(search_project(&p, "root", false, false, Some(o), || false).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
