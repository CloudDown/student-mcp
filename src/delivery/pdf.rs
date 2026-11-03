//! Bounded PDF inspection, page selection and rendering.
use crate::pdf::{output as pdf_output, Workspace};
use serde_json::{json, Value};

pub(crate) const MAX_PDF_PAGES: u32 = 24;
const PDF_TAIL_EXTEND: u32 = 8;
const PDF_HARD_MAX: u32 = MAX_PDF_PAGES + PDF_TAIL_EXTEND;
const MAX_PAGE_BYTES: usize = 1_500_000;
const MAX_IMAGES_TOTAL_BYTES: usize = 8_000_000;

pub(crate) fn temp_job_dir() -> Option<Workspace> {
    Workspace::new()
}

#[derive(Clone, Copy)]
pub(crate) struct PdfRange {
    pub from: u32,
    pub to: u32,
}

pub(crate) fn resolve_pdf_range(
    page_from: Option<u32>,
    page_to: Option<u32>,
    total_pages: u32,
) -> Result<PdfRange, String> {
    let from = page_from.unwrap_or(1).max(1);
    if page_to.is_some_and(|to| to < from) {
        return Err("page_to must be >= page_from.".into());
    }
    let hard_cap = from.saturating_add(PDF_HARD_MAX - 1);
    let requested_to = page_to.unwrap_or(from.saturating_add(MAX_PDF_PAGES - 1));
    let mut to = requested_to.min(hard_cap);
    if total_pages > 0 {
        if from > total_pages {
            return Err(format!(
                "page_from ({from}) is past the end of this PDF ({total_pages} pages)."
            ));
        }
        to = to.min(total_pages);
        if page_to.is_none()
            && to < total_pages
            && total_pages - to <= PDF_TAIL_EXTEND
            && total_pages <= hard_cap
        {
            to = total_pages;
        }
    }
    Ok(PdfRange { from, to })
}

fn fold_char(ch: char) -> char {
    match ch {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => ch,
    }
}

pub(crate) fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        let ch = fold_char(ch);
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

pub(crate) fn find_pdf_matches(pages: &[String], needle: &str) -> Vec<(u32, String)> {
    let needle = normalize_text(needle);
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        if page.trim().is_empty() || !contains_section(&normalize_text(page), &needle) {
            continue;
        }
        let snippet = page
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && contains_section(&normalize_text(line), &needle))
            .or_else(|| page.lines().map(str::trim).find(|line| !line.is_empty()))
            .unwrap_or_default();
        matches.push((index as u32 + 1, truncate_chars(snippet, 160)));
        if matches.len() >= 10 {
            break;
        }
    }
    matches
}

pub(crate) fn pdf_outline(pages: &[String]) -> Vec<(u32, String)> {
    let mut outline = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let number = index as u32 + 1;
        if looks_like_toc(number, pages) {
            continue;
        }
        for line in page
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(6)
        {
            if line.chars().count() > 140 {
                continue;
            }
            let head: String = line.chars().take(8).collect();
            let numbered =
                head.chars().next().is_some_and(|ch| ch.is_ascii_digit()) && head.contains('.');
            let lower = normalize_text(line);
            let named = lower.starts_with("chapitre ")
                || lower.starts_with("section ")
                || lower.starts_with("annexe ");
            if numbered || named {
                outline.push((number, truncate_chars(line, 140)));
            }
            if outline.len() >= 40 {
                return outline;
            }
        }
    }
    outline
}

pub(crate) fn contains_section(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = haystack[start..].find(needle) {
        let at = start + offset;
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + needle.len()..].chars().next();
        if !before.is_some_and(|ch| ch.is_ascii_digit())
            && !after.is_some_and(|ch| ch.is_ascii_digit())
        {
            return true;
        }
        start = at + haystack[at..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

pub(crate) fn looks_like_toc(page: u32, pages: &[String]) -> bool {
    let text = pages
        .get(page.saturating_sub(1) as usize)
        .map(String::as_str)
        .unwrap_or_default();
    let head = normalize_text(&text.chars().take(600).collect::<String>());
    head.contains("table des matieres")
        || head.contains("table of contents")
        || head.contains("sommaire")
        || head.contains("contenu du cours")
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let mut output: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

pub(crate) fn pdf_result_note(
    truncated: bool,
    from: u32,
    to: u32,
    total_pages: u32,
    next_page_from: Option<u32>,
) -> String {
    if truncated {
        let next = next_page_from.unwrap_or(to + 1);
        let total = if total_pages > 0 {
            total_pages.to_string()
        } else {
            "unknown".into()
        };
        format!("Rendered PDF pages {from}-{to} of {total}. This window is incomplete. Call the same file tool again with the same document identifiers and page_from={next} (and optional page_to) to continue. Prefer the extracted text and page images over guessing missing pages.")
    } else if total_pages > 0 {
        format!("Rendered PDF pages {from}-{to} of {total_pages}. Read the extracted text and page images; do not invent content for pages outside this range.")
    } else {
        format!("Rendered PDF pages {from}-{to}. Read the extracted text and page images; if a section is missing, call the same file tool again with a higher page_from.")
    }
}

pub(crate) fn pdftotext_cli_range(bytes: &[u8], from: u32, to: u32) -> Option<String> {
    let dir = temp_job_dir()?;
    let path = dir.join("file.pdf");
    std::fs::write(&path, bytes).ok()?;
    let output = pdf_output(
        std::process::Command::new("pdftotext")
            .args([
                "-layout",
                "-enc",
                "UTF-8",
                "-f",
                &from.to_string(),
                "-l",
                &to.to_string(),
            ])
            .arg(&path)
            .arg("-"),
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn pdf_page_count(bytes: &[u8]) -> Option<u32> {
    let dir = temp_job_dir()?;
    let path = dir.join("file.pdf");
    std::fs::write(&path, bytes).ok()?;
    let output = pdf_output(std::process::Command::new("pdfinfo").arg(&path)).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(rest) = line
            .trim()
            .strip_prefix("Pages:")
            .or_else(|| line.trim().strip_prefix("pages:"))
        {
            if let Ok(count) = rest.trim().parse::<u32>() {
                return (count <= 10_000).then_some(count);
            }
        }
    }
    None
}

pub(crate) fn render_pdf_pages(bytes: &[u8], from: u32, to: u32) -> Vec<Value> {
    let Some(dir) = temp_job_dir() else {
        return Vec::new();
    };
    let pdf_path = dir.join("file.pdf");
    let prefix = dir.join("page");
    if std::fs::write(&pdf_path, bytes).is_err()
        || !pdf_output(
            std::process::Command::new("pdftoppm")
                .args([
                    "-jpeg",
                    "-jpegopt",
                    "quality=78",
                    "-r",
                    "128",
                    "-scale-to",
                    "1600",
                    "-f",
                    &from.to_string(),
                    "-l",
                    &to.to_string(),
                ])
                .arg(&pdf_path)
                .arg(&prefix),
        )
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg")
                })
        })
        .collect();
    files.sort();
    use base64::Engine;
    let mut pages = Vec::new();
    let mut total_image_bytes = 0usize;
    for (index, path) in files.into_iter().enumerate() {
        let Ok(image) = std::fs::read(&path) else {
            continue;
        };
        if image.is_empty()
            || image.len() > MAX_PAGE_BYTES
            || total_image_bytes.saturating_add(image.len()) > MAX_IMAGES_TOTAL_BYTES
        {
            continue;
        }
        total_image_bytes += image.len();
        pages.push(json!({"page": from.saturating_add(index as u32), "mimeType": "image/jpeg", "data": base64::engine::general_purpose::STANDARD.encode(image)}));
    }
    pages
}
