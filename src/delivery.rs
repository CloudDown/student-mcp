//! Platform-neutral document delivery. Connectors own authentication and downloads.
use crate::redact::scrub_secret;
use serde_json::{json, Value};
use std::sync::Arc;
use url::Url;

mod office;
pub(crate) mod pdf;
pub(crate) mod store;
use pdf::*;

#[derive(Default, Clone)]
pub(crate) struct FileQuery {
    pub format: FileFormat,
    pub page_from: Option<u32>,
    pub page_to: Option<u32>,
    pub find: Option<String>,
    pub document_id: Option<String>,
    pub scope: Option<[u8; 32]>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileFormat {
    #[default]
    File,
    Text,
    Images,
}

const MAX_TEXT_CHARS: usize = 20_000;
const MAX_INLINE_BLOB_BYTES: usize = 6 * 1024 * 1024;

pub(super) enum Prepared {
    Pdf {
        bytes: Arc<Vec<u8>>,
        total: u32,
        text: Option<Vec<String>>,
        converted: bool,
    },
    Office {
        document: crate::document::Document,
        limitation: String,
    },
    Text(String),
    Unsupported,
}

fn prepare(document: &store::Document) -> Result<Prepared, String> {
    let bytes = &document.bytes;
    let is_pdf = document.mime.to_ascii_lowercase().contains("pdf") || bytes.starts_with(b"%PDF");
    let converted = if !is_pdf && office::supported(&document.filename) {
        Some(office::convert(&document.filename, bytes))
    } else {
        None
    };
    let pdf = if is_pdf {
        Some(Arc::clone(bytes))
    } else {
        converted
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(Arc::clone)
    };
    if let Some(bytes) = pdf {
        if let Some(total) = pdf_page_count(&bytes).filter(|count| *count > 0) {
            let text = pdftotext_cli_range(&bytes, 1, total)
                .map(|text| crate::pdf::split_pages(&text, total));
            return Ok(Prepared::Pdf {
                bytes,
                total,
                text,
                converted: !is_pdf,
            });
        }
        if is_pdf {
            return Err("Cannot inspect this PDF. Install Poppler, or request format=file for the original.".into());
        }
    }
    if let Some(extracted) = crate::document::extract(&document.filename, bytes)? {
        let limitation = match converted {
            Some(Err(reason)) => reason,
            Some(Ok(_)) => "Local PDF conversion could not be inspected. Text fallback may omit charts, equations and layout.".into(),
            None => "Structured cell values only; formulas are not recalculated and charts are not rendered.".into(),
        };
        return Ok(Prepared::Office {
            document: extracted,
            limitation,
        });
    }
    if let Some(text) = extract_text(&document.mime, bytes) {
        return Ok(Prepared::Text(text));
    }
    Ok(Prepared::Unsupported)
}

pub(crate) fn read_cached(query: &FileQuery, secret: &str) -> Result<Value, String> {
    let scope = query
        .scope
        .ok_or("Select a connected account before reading a document ID.")?;
    let id = query.document_id.as_deref().ok_or("Provide document_id.")?;
    document_result(store::get(scope, id)?, query, secret)
}

pub(crate) fn read_document(
    filename: &str,
    mime: &str,
    document: Arc<Vec<u8>>,
    query: &FileQuery,
    secret: &str,
) -> Result<Value, String> {
    document_result(
        store::remember(store::Document::new(filename, mime, document, query.scope)),
        query,
        secret,
    )
}

fn document_result(
    document: Arc<store::Document>,
    query: &FileQuery,
    secret: &str,
) -> Result<Value, String> {
    let mut result = if query.format == FileFormat::File {
        original_file(&document)
    } else {
        match document
            .prepared
            .get_or_init(|| prepare(&document))
            .as_ref()
            .map_err(Clone::clone)?
        {
            Prepared::Pdf {
                bytes,
                total,
                text,
                converted,
            } => {
                let mut result = study_pdf(
                    &document.filename,
                    &document.mime,
                    Arc::clone(bytes),
                    *total,
                    text.as_deref(),
                    query,
                    secret,
                )?;
                result["conversion"] = json!(if *converted { "local-pdf" } else { "none" });
                result["size"] = json!(document.bytes.len());
                result
            }
            Prepared::Office {
                document: extracted,
                limitation,
            } => {
                let mut result = office_document_result(
                    &document.filename,
                    &document.mime,
                    &document.bytes,
                    extracted,
                    query,
                    secret,
                )?;
                result["visualLimitations"] = json!(limitation);
                result["conversion"] = json!(if office::supported(&document.filename) {
                    "unavailable"
                } else {
                    "not-applicable"
                });
                result
            }
            Prepared::Text(text) => {
                let extracted = crate::document::Document {
                    unit: "text",
                    parts: vec![text.clone()],
                    note: "Plain text/CSV; no images or OCR.",
                };
                let mut result = office_document_result(
                    &document.filename,
                    &document.mime,
                    &document.bytes,
                    &extracted,
                    query,
                    secret,
                )?;
                result["text"] = json!(result["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"));
                result
            }
            Prepared::Unsupported => {
                return Err(
                    "Unsupported study format. Request format=file for the original.".into(),
                )
            }
        }
    };
    if query.scope.is_some() {
        result["documentId"] = json!(document.id);
        result["expiresInSeconds"] = json!(document.remaining_seconds());
    }
    Ok(result)
}

fn original_file(document: &store::Document) -> Value {
    let filename = &document.filename;
    let bytes = &document.bytes;
    let mime = if bytes.starts_with(b"%PDF") {
        "application/pdf"
    } else {
        &document.mime
    };
    let mut result = json!({"filename": filename, "mimeType": mime, "size": bytes.len()});
    if crate::share::enabled() {
        if let Some(id) = crate::share::publish_bytes(filename, mime, Arc::clone(bytes), "", 0) {
            if let Some(links) = crate::share::links_for(&id) {
                let local_only = links.download.contains("://127.0.0.1");
                result["links"] = json!({"download": links.download, "text":links.text, "expiresInHours": 3, "localOnly": local_only});
            }
        } else {
            result["linkNote"] = json!("The temporary link store is full. The original file is attached instead; existing links remain valid until expiry or app shutdown.");
        }
    }
    if bytes.len() <= MAX_INLINE_BLOB_BYTES || result.get("links").is_none() {
        use base64::Engine;
        result["file"] = json!({"filename": filename, "mimeType": mime, "data": base64::engine::general_purpose::STANDARD.encode(bytes.as_slice())});
    }
    result["note"] = json!("Original file, not a guarantee that the assistant can read its contents. Share links.download when asked for a download; never substitute an internal school URL. localOnly links work only on this computer. Links expire after 3 hours and require the app to stay running. To study, request format=text or images with document_id and page_from/page_to. No local OCR.");
    result
}

fn study_pdf(
    filename: &str,
    mime: &str,
    document: Arc<Vec<u8>>,
    total_pages: u32,
    extracted: Option<&[String]>,
    query: &FileQuery,
    secret: &str,
) -> Result<Value, String> {
    let bytes = document.as_slice();
    let page_texts = extracted.unwrap_or(&[]);
    if query.find.is_some() && extracted.is_none() {
        return Err("PDF text extraction unavailable; cannot search. Request a page window without find to read images.".into());
    }
    let matches = query
        .find
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|needle| find_pdf_matches(page_texts, needle))
        .unwrap_or_default();
    let matched_page = matches
        .iter()
        .map(|(page, _)| *page)
        .find(|page| *page > 2 && !looks_like_toc(*page, page_texts))
        .or_else(|| matches.first().map(|(page, _)| *page));
    let start_page = query.page_from.or(matched_page).unwrap_or(1);
    let range = resolve_pdf_range(Some(start_page), query.page_to, total_pages)?;
    let window_pages = (range.to.saturating_sub(range.from) + 1) as usize;
    let text = (range.from..=range.to)
        .map(|page| {
            format!(
                "--- page {page} ---\n{}",
                page_texts
                    .get(page as usize - 1)
                    .map(String::as_str)
                    .unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let pages = render_pdf_pages(bytes, range.from, range.to);
    // Never claim a complete study window while silently omitting a diagram.
    if pages.len() != window_pages {
        return Err("PDF window could not be fully rendered within time/size limits. Request a smaller page window or format=file. No page images were delivered.".into());
    }
    let truncated = total_pages > 0 && range.to < total_pages;
    let next_page_from = if truncated { Some(range.to + 1) } else { None };
    let (text, text_truncated) = if extracted.is_some() {
        let over = text.chars().count() > MAX_TEXT_CHARS;
        let text = if over { truncate_text(&text) } else { text };
        (Some(scrub_secret(text, secret)), over)
    } else {
        (None, false)
    };
    let mut result = json!({
        "filename": filename, "mimeType": mime, "size": bytes.len(),
        "totalPages": if total_pages > 0 { json!(total_pages) } else { Value::Null },
        "pageFrom": range.from, "pageTo": range.to, "renderedPages": pages.len(),
        "truncated": truncated, "nextPageFrom": next_page_from, "textTruncated": text_truncated,
    });
    let outline = pdf_outline(page_texts);
    if !outline.is_empty() {
        result["outline"] = json!(outline
            .iter()
            .map(
                |(page, title)| json!({"page": page, "title": scrub_secret(title.clone(), secret)})
            )
            .collect::<Vec<_>>());
    }
    if let Some(needle) = query
        .find
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        result["find"] = json!(scrub_secret(needle.to_string(), secret));
        result["matchedPage"] = matched_page.map_or(Value::Null, |page| json!(page));
        if !matches.is_empty() {
            result["matches"] = json!(matches
                .iter()
                .map(|(page, snippet)| json!({"page": page, "snippet": scrub_secret(snippet.clone(), secret)}))
                .collect::<Vec<_>>());
        }
    }
    if let Some(text) = &text {
        result["text"] = json!(text);
    }
    if text.is_some() {
        result["textSource"] = json!("pdf-text");
    } else {
        result["textSource"] = json!("unavailable");
    }
    result["pageCount"] = json!(pages.len());
    result["pages"] = json!(pages);
    let mut note = pdf_result_note(truncated, range.from, range.to, total_pages, next_page_from);
    if extracted.is_none() || page_texts.iter().all(|text| text.trim().is_empty()) {
        note.push_str(" No usable text layer was extracted; read the page images. No local OCR was performed.");
    }
    if let Some(needle) = query
        .find
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        note = if let Some(page) = matched_page {
            format!("Found \"{needle}\" on page {page}. {note}")
        } else {
            format!(
                "Could not find \"{needle}\" in this PDF's text layer (it may be scanned). {note}"
            )
        };
    }
    if text_truncated {
        note.push_str(" Text was capped at 20,000 characters; request a smaller page window for the omitted text.");
    }
    result["note"] = json!(scrub_secret(note, secret));
    Ok(result)
}

fn office_document_result(
    filename: &str,
    mime: &str,
    bytes: &[u8],
    document: &crate::document::Document,
    query: &FileQuery,
    secret: &str,
) -> Result<Value, String> {
    let mut blocks = Vec::new();
    for (index, part) in document.parts.iter().enumerate() {
        let clean = scrub_secret(part.clone(), secret);
        let chars: Vec<char> = clean.chars().collect();
        for chunk in chars.chunks(8_000) {
            blocks.push((index + 1, chunk.iter().collect::<String>()));
        }
    }
    let total = blocks.len() as u32;
    if total == 0 {
        return Err("No text or cell values could be extracted. Request format=file for the original; no page images are available on this fallback path.".into());
    }
    let needle = query.find.as_deref().map(str::to_lowercase);
    let matches: Vec<u32> = needle
        .as_ref()
        .map(|needle| {
            blocks
                .iter()
                .enumerate()
                .filter(|(_, (_, text))| text.to_lowercase().contains(needle))
                .map(|(i, _)| i as u32 + 1)
                .collect()
        })
        .unwrap_or_default();
    let from = query
        .page_from
        .or_else(|| matches.first().copied())
        .unwrap_or(1);
    if from == 0 || (total > 0 && from > total) {
        return Err("page_from is outside this document's text blocks.".into());
    }
    let to = query.page_to.unwrap_or(from.saturating_add(1));
    if to < from {
        return Err("page_to must be >= page_from.".into());
    }
    let to = to.min(from.saturating_add(1)).min(total);
    let selected: Vec<Value> = blocks
        .iter()
        .enumerate()
        .skip(from.saturating_sub(1) as usize)
        .take(to.saturating_sub(from).saturating_add(1) as usize)
        .map(|(i, (unit, text))| json!({"block": i + 1, "sourceUnit": unit, "text": text}))
        .collect();
    let note = format!("{} Text/cells only: no faithful page images. page_from/page_to select text blocks (8000 characters), not physical pages; sourceUnit identifies the original slide/sheet/document. Follow nextPageFrom for the rest.", document.note);
    let result = json!({"filename": filename, "mimeType": mime, "size": bytes.len(),
        "sourceUnit": document.unit, "totalBlocks": total, "pageFrom": from, "pageTo": to,
        "blocks": selected, "matches": matches, "truncated": to < total,
        "nextPageFrom": if to < total { Some(to + 1) } else { None },
        "note": note});
    Ok(result)
}

pub(crate) fn filename_from(disposition: Option<&str>, fileurl: &str) -> String {
    if let Some(header) = disposition {
        if let Some(value) = header.split("filename=").nth(1) {
            let value = value
                .trim()
                .trim_start_matches('"')
                .trim_end_matches('"')
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    Url::parse(fileurl)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "file".into())
}

fn extract_text(mime: &str, bytes: &[u8]) -> Option<String> {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("text/") || mime.contains("json") || mime.contains("xml") {
        // Keep numeric CSV/formula-like text too. Input bytes are connector-bounded.
        return Some(String::from_utf8_lossy(bytes).replace('\0', ""));
    }
    None
}

fn truncate_text(text: &str) -> String {
    let mut text = text.replace('\0', "");
    if text.chars().count() > MAX_TEXT_CHARS {
        text = text.chars().take(MAX_TEXT_CHARS).collect();
        text.push_str("\n…");
    }
    text
}

pub(crate) fn mcp_content_for_file(mut data: Value, service: &str) -> Vec<Value> {
    let file = data
        .as_object_mut()
        .and_then(|object| object.remove("file"));
    let links = data.get("links").cloned();
    let pages = match data
        .as_object_mut()
        .and_then(|object| object.remove("pages"))
    {
        Some(Value::Array(pages)) => pages,
        _ => Vec::new(),
    };
    let mut content = vec![
        json!({"type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default()}),
    ];
    if let Some(mut file) = file {
        if let Some(blob @ Value::String(_)) = file.get_mut("data").map(Value::take) {
            let filename = file
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or("file.pdf");
            let mime = file
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("application/pdf");
            let uri_name: String = filename
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect();
            content.push(json!({"type": "resource", "resource": {
                "uri": format!("{service}://file/{uri_name}"), "mimeType": mime, "blob": blob,
            }}));
        }
    }
    for mut page in pages {
        let Some(image @ Value::String(_)) = page.get_mut("data").map(Value::take) else {
            continue;
        };
        let mime = page
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("image/jpeg");
        if let Some(number) = page.get("page").and_then(Value::as_u64) {
            content.push(
                json!({"type":"text", "text":format!("Page {number} (complete page image):")}),
            );
        }
        content.push(json!({"type": "image", "data": image, "mimeType": mime}));
    }
    if let Some(links) = links {
        for (key, name, description) in [
            ("download", "Original file", "Direct link to the original file. Open or download it, then attach it to the chat if supported."),
            ("text", "Document text", "Extracted text, when requested."),
        ] {
            let Some(uri) = links.get(key).and_then(Value::as_str) else { continue; };
            content.push(json!({"type": "resource_link", "uri": uri, "name": name, "description": description,
                "mimeType": if key == "download" { data.get("mimeType").and_then(Value::as_str).unwrap_or("application/octet-stream") } else { "text/plain" }}));
        }
    }
    content
}

#[cfg(test)]
mod tests;
