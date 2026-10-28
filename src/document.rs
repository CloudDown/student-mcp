//! Data-only Office extraction. Never starts Office, follows links, or evaluates macros/formulas.
use calamine::{open_workbook_auto_from_rs, Reader};
use quick_xml::{events::Event, Reader as XmlReader};
use std::io::{Cursor, Read};

const MAX_EXPANDED: u64 = 32 * 1024 * 1024;
const MAX_UNITS: usize = 2048;
const MAX_CHARS: usize = 2_000_000;

pub(crate) struct Document {
    pub unit: &'static str,
    pub parts: Vec<String>,
    pub note: &'static str,
}

pub(crate) fn extract(filename: &str, bytes: &[u8]) -> Result<Option<Document>, String> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "docx" | "docm" | "pptx" | "pptm" | "xlsx" | "xlsm" | "ods" | "odt" | "odp"
    ) {
        validate_zip(bytes)?;
    }
    let result = match ext.as_str() {
        "xlsx" | "xlsm" | "xls" | "ods" => spreadsheet(bytes)?,
        "docx" | "docm" | "pptx" | "pptm" | "odt" | "odp" => xml_document(&ext, bytes)?,
        "doc" | "ppt" => legacy(&ext, bytes)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}

fn validate_zip(bytes: &[u8]) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "Invalid or encrypted Office/OpenDocument archive.".to_string())?;
    if archive.len() > 4096 {
        return Err("Document contains too many archive entries (max 4096).".into());
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|_| "Cannot inspect document archive.")?;
        total = total
            .checked_add(entry.size())
            .ok_or("Document archive is too large.")?;
        if total > MAX_EXPANDED {
            return Err("Expanded document exceeds 32 MiB.".into());
        }
    }
    Ok(())
}

fn xml_document(ext: &str, bytes: &[u8]) -> Result<Document, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Invalid document archive.")?;
    let mut names: Vec<String> = archive
        .file_names()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Invalid document part name.")?
        .into_iter()
        .filter(|name| match ext {
            "docx" | "docm" => name.as_ref() == "word/document.xml",
            "pptx" | "pptm" => {
                name.starts_with("ppt/slides/slide")
                    && name.ends_with(".xml")
                    && !name.contains("/_rels/")
            }
            _ => name.as_ref() == "content.xml",
        })
        .map(|name| name.into_owned())
        .collect();
    // Slide 2 must precede slide 10. These are package indices, not dates or course codes.
    names.sort_by_key(|name| {
        name.strip_prefix("ppt/slides/slide")
            .and_then(|s| s.strip_suffix(".xml"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0)
    });
    if matches!(ext, "pptx" | "pptm") && archive.by_name("ppt/presentation.xml").is_ok() {
        names = presentation_order(&mut archive)?;
    }
    if names.is_empty() || names.len() > MAX_UNITS {
        return Err("Missing document body or too many slides.".into());
    }
    let mut parts = Vec::new();
    let mut chars = 0usize;
    for name in names {
        let mut xml = Vec::new();
        archive
            .by_name(&name)
            .map_err(|_| "Missing document part.")?
            .take(MAX_EXPANDED + 1)
            .read_to_end(&mut xml)
            .map_err(|_| "Cannot read document part.")?;
        if xml.len() as u64 > MAX_EXPANDED {
            return Err("Document part is too large.".into());
        }
        let extracted = xml_parts(&xml, ext == "odp")?;
        for text in extracted {
            chars += text.chars().count();
            if chars > MAX_CHARS || parts.len() >= MAX_UNITS {
                return Err("Extracted document exceeds the text/section safety limit.".into());
            }
            parts.push(text);
        }
    }
    Ok(Document {
        unit: if matches!(ext, "pptx" | "pptm" | "odp") { "slide" } else { "document" },
        parts,
        note: "Text and table cells only; layout, equations, charts, images and speaker notes may be missing. No macros, external links or formulas are executed.",
    })
}

fn presentation_order(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>, String> {
    let mut records = Vec::new();
    for (path, tag) in [
        ("ppt/presentation.xml", "sldId"),
        ("ppt/_rels/presentation.xml.rels", "Relationship"),
    ] {
        let mut xml = String::new();
        archive
            .by_name(path)
            .map_err(|_| "Missing PowerPoint presentation relationships.")?
            .take(MAX_EXPANDED + 1)
            .read_to_string(&mut xml)
            .map_err(|_| "Cannot read PowerPoint slide order.")?;
        if xml.len() as u64 > MAX_EXPANDED {
            return Err("PowerPoint metadata is too large.".into());
        }
        let mut reader = XmlReader::from_str(&xml);
        let mut entries = Vec::new();
        loop {
            match reader.read_event() {
                Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == tag => {
                    let mut attrs = std::collections::HashMap::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|_| "Invalid PowerPoint attributes.")?;
                        attrs.insert(
                            attr.key.as_ref().to_string(),
                            attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(|_| "Invalid PowerPoint relationship.")?
                                .into_owned(),
                        );
                    }
                    entries.push(attrs);
                }
                Ok(Event::DocType(_)) => {
                    return Err("Document XML with a DTD is not supported.".into())
                }
                Ok(Event::Eof) => break,
                Err(_) => return Err("Malformed PowerPoint metadata.".into()),
                _ => {}
            }
        }
        records.push(entries);
    }
    let mut names = Vec::new();
    for slide in &records[0] {
        let id = slide
            .get("r:id")
            .ok_or("Missing PowerPoint slide relationship ID.")?;
        let rel = records[1]
            .iter()
            .find(|r| r.get("Id") == Some(id))
            .ok_or("Missing PowerPoint slide relationship.")?;
        if rel.get("TargetMode").is_some_and(|mode| mode == "External") {
            return Err("External slides are not supported.".into());
        }
        let target = rel
            .get("Target")
            .ok_or("Missing PowerPoint slide target.")?;
        let name = if target.starts_with("/ppt/slides/") {
            target.trim_start_matches('/').to_string()
        } else if target.starts_with("slides/") {
            format!("ppt/{target}")
        } else {
            return Err("Unsupported PowerPoint slide path.".into());
        };
        if name.contains("..") {
            return Err("Invalid PowerPoint slide path.".into());
        }
        names.push(name);
    }
    Ok(names)
}

fn xml_parts(xml: &[u8], split_slides: bool) -> Result<Vec<String>, String> {
    let mut reader = XmlReader::from_reader(xml);
    let mut text = String::new();
    let mut parts = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(event)) => {
                text.push_str(
                    &quick_xml::escape::unescape(event.as_ref())
                        .map_err(|_| "Invalid XML entity.")?,
                );
            }
            Ok(Event::CData(event)) => text.push_str(event.as_ref()),
            Ok(Event::GeneralRef(event)) => {
                let entity: &str = event.as_ref();
                text.push_str(
                    &quick_xml::escape::unescape(&format!("&{entity};"))
                        .map_err(|_| "Unsupported XML entity.")?,
                );
            }
            Ok(Event::DocType(_)) => return Err("Document XML with a DTD is not supported.".into()),
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "tab" => text.push('\t'),
                "br" | "line-break" => text.push('\n'),
                _ => {}
            },
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "p" | "tr" | "table-row" | "h" => text.push('\n'),
                "tc" | "table-cell" => text.push('\t'),
                "page" if split_slides => parts.push(std::mem::take(&mut text)),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => return Err("Malformed document XML.".into()),
            _ => {}
        }
        if text.len() > MAX_CHARS * 4 {
            return Err("Document text exceeds safety limit.".into());
        }
    }
    if !text.trim().is_empty() || parts.is_empty() {
        parts.push(text);
    }
    Ok(parts)
}

fn spreadsheet(bytes: &[u8]) -> Result<Document, String> {
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|_| {
        "Cannot read this spreadsheet (invalid, encrypted or unsupported).".to_string()
    })?;
    let names = workbook.sheet_names().to_vec();
    if names.len() > 256 {
        return Err("Spreadsheet exceeds 256 sheets.".into());
    }
    let mut parts = Vec::new();
    let mut chars = 0usize;
    for name in names {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|_| "Cannot read spreadsheet sheet.")?;
        if range.get_size().0.saturating_mul(range.get_size().1) > 1_000_000 {
            return Err("Spreadsheet exceeds 1 million cells per sheet.".into());
        }
        let mut text = format!("Sheet: {name}\n");
        let start = range.start().unwrap_or((0, 0));
        text.push_str(&format!("First column: {} (1-based). Rows below keep their original row numbers. Values are tab-separated.\n", start.1 + 1));
        for (row_index, row) in range.rows().enumerate() {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| cell.to_string().replace(['\t', '\r', '\n'], " "))
                .collect();
            text.push_str(&format!(
                "{}\t{}\n",
                start.0 as usize + row_index + 1,
                cells.join("\t")
            ));
            if text.len() > MAX_CHARS * 4 {
                return Err("Spreadsheet text exceeds safety limit.".into());
            }
        }
        chars += text.chars().count();
        if chars > MAX_CHARS {
            return Err("Spreadsheet text exceeds safety limit.".into());
        }
        parts.push(text);
    }
    Ok(Document { unit: "sheet", parts, note: "Stored cell values only, with sheet names and row numbers. Formulas are not recalculated; cached values may be stale. Charts, images and formatting are not extracted." })
}

fn legacy(ext: &str, bytes: &[u8]) -> Result<Document, String> {
    let tool = if ext == "doc" { "antiword" } else { "catppt" };
    let dir = crate::pdf::Workspace::new().ok_or("Cannot create private document workspace.")?;
    let path = dir.join(format!("input.{ext}"));
    std::fs::write(&path, bytes).map_err(|_| "Cannot prepare document.")?;
    let result = crate::pdf::output(std::process::Command::new(tool).arg(&path))
        .map_err(|_| format!("Reading .{ext} requires {tool} installed locally. Alternatively upload an exported PDF or modern Office file."))?;
    if !result.status.success() {
        return Err(format!("{tool} could not read this document."));
    }
    let text = String::from_utf8_lossy(&result.stdout).into_owned();
    if text.chars().count() > MAX_CHARS {
        return Err("Legacy document text exceeds safety limit.".into());
    }
    Ok(Document { unit: "document", parts: vec![text], note: "Legacy Office text extraction; layout, figures and equations may be missing. No Office application or macros are run." })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn docx_preserves_paragraphs_unicode_and_table_cells() {
        let bytes = archive(&[("word/document.xml", "<w:document xmlns:w='w'><w:body><w:p><w:r><w:t>Énergie &amp; force</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Masse</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>12 kg</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"), ("word/vbaProject.bin", "ignored")]);
        let doc = extract("cours.docx", &bytes).unwrap().unwrap();
        assert_eq!(doc.unit, "document");
        assert!(doc.parts[0].contains("Énergie & force\n"));
        assert!(doc.parts[0].contains("Masse\n\t12 kg"));
        assert!(!doc.parts[0].contains("ignored"));
    }

    #[test]
    fn pptx_keeps_slide_boundaries_and_numeric_order() {
        let bytes = archive(&[
            (
                "ppt/slides/slide10.xml",
                "<p:sld xmlns:p='p'><p:p>Dix</p:p></p:sld>",
            ),
            (
                "ppt/slides/slide2.xml",
                "<p:sld xmlns:p='p'><p:p>Deux</p:p></p:sld>",
            ),
        ]);
        let doc = extract("cours.pptx", &bytes).unwrap().unwrap();
        assert_eq!(doc.unit, "slide");
        assert_eq!(doc.parts.len(), 2);
        assert!(doc.parts[0].contains("Deux"));
        assert!(doc.parts[1].contains("Dix"));
    }

    #[test]
    fn pptx_respects_reordered_presentation_slides() {
        let bytes = archive(&[
            ("ppt/presentation.xml", "<p:presentation xmlns:p='p' xmlns:r='r'><p:sldIdLst><p:sldId id='1' r:id='rId10'/><p:sldId id='2' r:id='rId2'/></p:sldIdLst></p:presentation>"),
            ("ppt/_rels/presentation.xml.rels", "<Relationships><Relationship Id='rId10' Target='slides/slide10.xml'/><Relationship Id='rId2' Target='slides/slide2.xml'/></Relationships>"),
            ("ppt/slides/slide10.xml", "<p>Dix</p>"),
            ("ppt/slides/slide2.xml", "<p>Deux</p>"),
        ]);
        let doc = extract("cours.pptx", &bytes).unwrap().unwrap();
        assert!(doc.parts[0].contains("Dix"));
        assert!(doc.parts[1].contains("Deux"));
    }

    #[test]
    fn odt_and_odp_keep_text_and_slide_boundaries() {
        let bytes = archive(&[("content.xml", "<office:document xmlns:office='o' xmlns:draw='d' xmlns:text='t'><draw:page><text:p>Slide une</text:p></draw:page><draw:page><text:p>Slide deux</text:p></draw:page></office:document>")]);
        let doc = extract("cours.odp", &bytes).unwrap().unwrap();
        assert_eq!(doc.parts.len(), 2);
        assert!(doc.parts[1].contains("Slide deux"));
        assert_eq!(
            extract("cours.odt", &bytes).unwrap().unwrap().parts.len(),
            1
        );
    }

    #[test]
    fn xlsx_reads_cells_without_evaluating_formula_or_links() {
        let bytes = archive(&[
            ("_rels/.rels", "<Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'><Relationship Id='rId1' Type='http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument' Target='xl/workbook.xml'/></Relationships>"),
            ("[Content_Types].xml", "<Types xmlns='http://schemas.openxmlformats.org/package/2006/content-types'><Override PartName='/xl/workbook.xml' ContentType='application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml'/></Types>"),
            ("xl/workbook.xml", "<workbook xmlns='http://schemas.openxmlformats.org/spreadsheetml/2006/main' xmlns:r='http://schemas.openxmlformats.org/officeDocument/2006/relationships'><sheets><sheet name='Mesures' sheetId='1' r:id='rId1'/></sheets></workbook>"),
            ("xl/_rels/workbook.xml.rels", "<Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'><Relationship Id='rId1' Type='http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet' Target='worksheets/sheet1.xml'/></Relationships>"),
            ("xl/worksheets/sheet1.xml", "<worksheet xmlns='http://schemas.openxmlformats.org/spreadsheetml/2006/main'><dimension ref='A1:B2'/><sheetData><row r='1'><c r='A1' t='inlineStr'><is><t>Force</t></is></c><c r='B1'><v>12</v></c></row><row r='2'><c r='A2'><f>1+1</f><v>2</v></c></row></sheetData></worksheet>"),
        ]);
        open_workbook_auto_from_rs(Cursor::new(&bytes)).expect("xlsx fixture is readable");
        let doc = extract("lab.xlsx", &bytes).unwrap().unwrap();
        assert_eq!(doc.unit, "sheet");
        assert!(doc.parts[0].contains("Sheet: Mesures"));
        assert!(doc.parts[0].contains("1\tForce\t12"));
        assert!(doc.parts[0].contains("2\t2"));
    }

    #[test]
    fn malformed_encrypted_and_dtd_documents_fail_explicitly() {
        assert!(extract("bad.docx", b"not zip").is_err());
        assert!(extract("bad.xlsx", b"encrypted").is_err());
        let bytes = archive(&[(
            "word/document.xml",
            "<!DOCTYPE x [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><x>&x;</x>",
        )]);
        assert!(extract("bad.docx", &bytes).is_err());
        assert!(extract("unknown.bin", b"opaque").unwrap().is_none());
    }

    #[test]
    fn zip_expansion_is_bounded_before_document_parsing() {
        let text = "x".repeat(MAX_EXPANDED as usize + 1);
        let bytes = archive(&[("word/document.xml", &text)]);
        assert!(extract("large.docx", &bytes)
            .err()
            .unwrap()
            .contains("32 MiB"));
    }
}
