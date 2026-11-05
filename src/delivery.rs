//! Platform-neutral document delivery. Connectors own authentication and downloads.
use crate::redact::scrub_secret;
use serde_json::{json, Value};
use std::sync::Arc;
use url::Url;

pub(crate) mod pdf;
use pdf::*;

#[derive(Default, Clone)]
pub(crate) struct FileQuery {
    pub format: FileFormat,
    pub page_from: Option<u32>,
    pub page_to: Option<u32>,
    pub find: Option<String>,
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

pub(crate) fn read_document(
    filename: &str,
    mime: &str,
    document: Arc<Vec<u8>>,
    query: &FileQuery,
    secret: &str,
) -> Result<Value, String> {
    let bytes = document.as_slice();
    let is_pdf = mime.to_ascii_lowercase().contains("pdf") || bytes.starts_with(b"%PDF");
    if !is_pdf {
        if let Some(document) = crate::document::extract(filename, bytes)? {
            if query.format == FileFormat::Images {
                return Err("Office image rendering is not supported. Request format=text, or export to PDF for diagrams and page images.".into());
            }
            return office_document_result(filename, mime, bytes.len(), document, query, secret);
        }
        let text = extract_text(mime, bytes).map(|text| scrub_secret(text, secret));
        let mut result = json!({"filename": filename, "mimeType": mime, "size": bytes.len()});
        match text {
            Some(text) if !text.trim().is_empty() => result["text"] = json!(text),
            _ => result["note"] = json!("Unsupported binary format. No content was extracted; export to PDF or a supported Office/OpenDocument format."),
        }
        return Ok(result);
    }
    if query.format == FileFormat::File {
        let mut result =
            json!({"filename": filename, "mimeType": "application/pdf", "size": bytes.len()});
        if crate::share::enabled() {
            if let Some(id) = crate::share::publish_bytes(
                filename,
                "application/pdf",
                Arc::clone(&document),
                "",
                0,
            ) {
                if let Some(links) = crate::share::links_for(&id) {
                    let local_only = links.download.contains("://127.0.0.1");
                    result["links"] = json!({"download": links.download, "expiresInHours": 3, "localOnly": local_only});
                }
            } else {
                result["linkNote"] = json!("The temporary link store is full. The original file is attached instead; existing links remain valid until expiry or app shutdown.");
            }
        }
        if bytes.len() <= MAX_INLINE_BLOB_BYTES || result.get("links").is_none() {
            use base64::Engine;
            result["file"] = json!({"filename": filename, "mimeType": "application/pdf", "data": base64::engine::general_purpose::STANDARD.encode(bytes)});
        }
        result["note"] = json!("Original PDF. Share links.download when the user asks for a PDF link; never substitute an internal course file URL. If links.localOnly is true, the link only works on the machine running this app. If no link is available, say so. Temporary links expire after 3 hours and require this app to remain running. For scans or diagrams, request format=images with page_from/page_to. No local OCR is performed.");
        return Ok(result);
    }
    let total_pages = pdf_page_count(bytes).ok_or("Cannot inspect this PDF. Install Poppler, or request format=file to read the original without local processing.")?;
    let full_text = if query.find.is_some() {
        pdftotext_cli_range(bytes, 1, if total_pages > 0 { total_pages } else { 2000 })
            .ok_or("Cannot search this PDF's text layer. Request format=file instead.")?
    } else {
        String::new()
    };
    let page_texts = crate::pdf::split_pages(&full_text, total_pages);
    let matches = query
        .find
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|needle| find_pdf_matches(&page_texts, needle))
        .unwrap_or_default();
    let matched_page = matches
        .iter()
        .map(|(page, _)| *page)
        .find(|page| *page > 2 && !looks_like_toc(*page, &page_texts))
        .or_else(|| matches.first().map(|(page, _)| *page));
    let start_page = query.page_from.or(matched_page).unwrap_or(1);
    let page_to = if query.format == FileFormat::Images {
        Some(
            query
                .page_to
                .unwrap_or(start_page.saturating_add(1))
                .min(start_page.saturating_add(7)),
        )
    } else {
        query.page_to
    };
    let range = resolve_pdf_range(Some(start_page), page_to, total_pages)?;
    let text = if query.format == FileFormat::Text {
        pdftotext_cli_range(bytes, range.from, range.to).ok_or(
            "PDF text extraction failed or timed out. Check Poppler, or request format=file.",
        )?
    } else {
        String::new()
    };
    let window_pages = (range.to.saturating_sub(range.from) + 1) as usize;
    let pages = if query.format == FileFormat::Images {
        render_pdf_pages(bytes, range.from, range.to)
    } else {
        Vec::new()
    };
    if query.format == FileFormat::Images && pages.is_empty() {
        return Err("PDF rendering failed, timed out or exceeded the image limit. Check Poppler, or request format=file.".into());
    }
    let truncated = total_pages > 0 && range.to < total_pages;
    let next_page_from = if truncated { Some(range.to + 1) } else { None };
    let (text, text_truncated) = match usable_text(text) {
        Some(text) => {
            let over = text.chars().count() > MAX_TEXT_CHARS;
            let text = if over { truncate_text(&text) } else { text };
            (Some(scrub_secret(text, secret)), over)
        }
        None => (None, false),
    };
    let shared = if crate::share::enabled() && query.find.is_some() {
        crate::share::publish_bytes(
            filename,
            "application/pdf",
            Arc::clone(&document),
            &scrub_secret(full_text, secret),
            total_pages,
        )
    } else {
        None
    };
    let links = shared.as_deref().and_then(share_links_json);
    let mut result = json!({
        "filename": filename, "mimeType": mime, "size": bytes.len(),
        "totalPages": if total_pages > 0 { json!(total_pages) } else { Value::Null },
        "pageFrom": range.from, "pageTo": range.to, "renderedPages": pages.len(),
        "truncated": truncated, "nextPageFrom": next_page_from, "textTruncated": text_truncated,
    });
    let outline = pdf_outline(&page_texts);
    if !outline.is_empty() {
        result["outline"] = json!(outline
            .iter()
            .map(|(page, title)| json!({"page": page, "title": title}))
            .collect::<Vec<_>>());
    }
    if let Some(needle) = query
        .find
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        result["find"] = json!(needle);
        result["matchedPage"] = matched_page.map_or(Value::Null, |page| json!(page));
        if !matches.is_empty() {
            result["matches"] = json!(matches
                .iter()
                .map(|(page, snippet)| json!({"page": page, "snippet": snippet}))
                .collect::<Vec<_>>());
        }
    }
    match &text {
        Some(text) if !text.trim().is_empty() => result["text"] = json!(text),
        _ if pages.is_empty() => result["note"] = json!("This PDF has no usable text layer. Request format=images for selected pages, or format=file for the original PDF."),
        _ => {}
    }
    if text.is_some() {
        result["textSource"] = json!("pdf-text");
    }
    if let Some(links) = links {
        if let Some(url) = links.get("download").and_then(Value::as_str) {
            result["downloadUrl"] = json!(url);
        }
        result["links"] = links;
    }
    let rendered_pages = pages.len();
    if !pages.is_empty() {
        result["pageCount"] = json!(rendered_pages);
        result["pages"] = json!(pages);
    }
    let mut note = pdf_result_note(truncated, range.from, range.to, total_pages, next_page_from);
    if query.format == FileFormat::Text && text.is_none() {
        note.push_str(" No usable text layer: request format=images for selected pages or format=file for the original PDF.");
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
    if query.format == FileFormat::Images && rendered_pages < window_pages {
        note.push_str(" Some page images are unavailable or too large to inline; request a smaller window or the original file.");
    }
    if result.get("links").is_some() {
        note.push_str(" links.download opens the original PDF. Temporary links require this app to remain running.");
    }
    result["note"] = json!(note);
    Ok(result)
}

fn office_document_result(
    filename: &str,
    mime: &str,
    size: usize,
    document: crate::document::Document,
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
    Ok(json!({"filename": filename, "mimeType": mime, "size": size,
        "sourceUnit": document.unit, "totalBlocks": total, "pageFrom": from, "pageTo": to,
        "blocks": selected, "matches": matches, "truncated": to < total,
        "nextPageFrom": if to < total { Some(to + 1) } else { None },
        "note": format!("{} page_from/page_to select text blocks (8000 characters), not physical pages; sourceUnit identifies the original slide/sheet/document. Follow nextPageFrom for the rest.", document.note)}))
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
        return usable_text(truncate_text(&String::from_utf8_lossy(bytes)));
    }
    None
}

pub(crate) fn usable_text(text: String) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let total = trimmed.chars().count().max(1);
    let junk = trimmed.chars().filter(|ch| matches!(ch, '\0'..='\u{08}' | '\u{0B}' | '\u{0C}' | '\u{0E}'..='\u{1F}' | '\u{7F}')).count();
    let letters = trimmed.chars().filter(|ch| ch.is_alphabetic()).count();
    if junk * 10 > total || (total > 80 && letters * 20 < total) {
        return None;
    }
    Some(text)
}

fn truncate_text(text: &str) -> String {
    let mut text = text.replace('\0', "");
    if text.chars().count() > MAX_TEXT_CHARS {
        text = text.chars().take(MAX_TEXT_CHARS).collect();
        text.push_str("\n…");
    }
    text
}

fn share_links_json(id: &str) -> Option<Value> {
    let links = crate::share::links_for(id)?;
    let local_only = links.download.contains("://127.0.0.1");
    Some(
        json!({"download": links.download, "text": links.text, "expiresInHours": 3, "localOnly": local_only}),
    )
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
        content.push(json!({"type": "image", "data": image, "mimeType": mime}));
    }
    if let Some(links) = links {
        for (key, name, description) in [
            ("download", "PDF file", "Direct link to the original PDF. Open or download it, then attach it to the chat if the client supports uploads."),
            ("text", "PDF text", "Extracted text, when requested."),
        ] {
            let Some(uri) = links.get(key).and_then(Value::as_str) else { continue; };
            content.push(json!({"type": "resource_link", "uri": uri, "name": name, "description": description,
                "mimeType": if key == "download" { "application/pdf" } else { "text/plain" }}));
        }
    }
    content
}
