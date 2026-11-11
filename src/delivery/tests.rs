use super::*;

/// Self-contained vector/text PDF, no Ghostscript, OCR, or school files.
fn pdf_fixture(count: u32, blank: bool) -> Vec<u8> {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Count {count} /Kids [{}] >>",
            (0..count)
                .map(|i| format!("{} 0 R", 4 + i * 2))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    for i in 0..count {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", 5 + i * 2));
        let stream = if blank {
            String::new()
        } else {
            format!("BT /F1 10 Tf 10 380 Td (Page {} Needle{} SECRET) Tj {} ET 0 0 1 RG 40 40 250 180 re S", i + 1, i + 1,
                "0 -14 Td (Text rich lecture definition and numerical formula 12 + 3 = 15.) Tj ".repeat(18))
        };
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[test]
fn study_keeps_vector_pages_even_with_plenty_of_text_and_labels_every_image() {
    if std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_err()
    {
        return;
    }
    let pdf = Arc::new(pdf_fixture(6, false));
    for format in [FileFormat::Text, FileFormat::Images] {
        let data = read_document(
            "notes.pdf",
            "application/pdf",
            Arc::clone(&pdf),
            &FileQuery {
                format,
                ..Default::default()
            },
            "SECRET",
        )
        .unwrap();
        assert_eq!(data["renderedPages"], 4);
        assert_eq!(data["nextPageFrom"], 5);
        assert_eq!(data["truncated"], true);
        let text = data["text"].as_str().unwrap();
        assert!(text.contains("--- page 4 ---"));
        assert!(!text.contains("SECRET"));
        let content = mcp_content_for_file(data, "moodle");
        assert_eq!(
            content
                .iter()
                .filter(|item| item["type"] == "image")
                .count(),
            4
        );
        assert!(content
            .iter()
            .any(|item| item["text"] == "Page 4 (complete page image):"));
    }
    let data = read_document(
        "notes.pdf",
        "application/pdf",
        pdf,
        &FileQuery {
            format: FileFormat::Text,
            find: Some("Needle5".into()),
            ..Default::default()
        },
        "SECRET",
    )
    .unwrap();
    assert_eq!(data["matchedPage"], 5);
    assert_eq!(data["pages"][0]["page"], 5);
    assert_eq!(data["renderedPages"], 2);
    assert_eq!(data["truncated"], false);
}

#[test]
fn scanned_or_blank_pages_still_arrive_as_images_without_ocr() {
    if std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_err()
    {
        return;
    }
    let data = read_document(
        "scan.pdf",
        "application/pdf",
        Arc::new(pdf_fixture(2, true)),
        &FileQuery {
            format: FileFormat::Text,
            ..Default::default()
        },
        "",
    )
    .unwrap();
    assert_eq!(data["renderedPages"], 2);
    assert!(data.get("ocrText").is_none());
}

#[test]
fn an_incomplete_render_is_an_error_not_a_success_with_missing_images() {
    let error = study_pdf(
        "incomplete.pdf",
        "application/pdf",
        Arc::new(pdf_fixture(1, false)),
        2,
        None,
        &FileQuery {
            format: FileFormat::Text,
            ..Default::default()
        },
        "",
    )
    .unwrap_err();
    assert!(error.contains("smaller page window"));
}

#[test]
fn text_paginates_numeric_csv_and_searches_later_blocks() {
    let bytes = Arc::new(format!("{}\nneedle", "12,34\n".repeat(4000)).into_bytes());
    let data = read_document(
        "data.csv",
        "text/csv",
        bytes,
        &FileQuery {
            format: FileFormat::Text,
            find: Some("needle".into()),
            ..Default::default()
        },
        "",
    )
    .unwrap();
    assert_eq!(data["pageFrom"], 4);
    assert!(data["text"].as_str().unwrap().contains("needle"));
}

#[test]
fn empty_text_does_not_claim_an_invalid_page_window() {
    assert!(read_document(
        "empty.txt",
        "text/plain",
        Arc::new(Vec::new()),
        &FileQuery {
            format: FileFormat::Text,
            ..Default::default()
        },
        ""
    )
    .is_err());
}

#[test]
fn explicit_office_fallback_is_text_not_fake_page_images() {
    let original = store::Document::new(
        "notes.docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Arc::new(vec![1]),
        None,
    );
    assert!(original
        .prepared
        .set(Ok(Prepared::Office {
            document: crate::document::Document {
                unit: "document",
                parts: vec!["Lecture with a chart".into()],
                note: "Extracted body."
            },
            limitation: "Converter unavailable; charts and layout are omitted.".into(),
        }))
        .is_ok());
    let data = document_result(
        original,
        &FileQuery {
            format: FileFormat::Images,
            ..Default::default()
        },
        "",
    )
    .unwrap();
    assert_eq!(data["conversion"], "unavailable");
    assert!(data["visualLimitations"]
        .as_str()
        .unwrap()
        .contains("charts"));
    assert!(data.get("pages").is_none());
    assert_eq!(data["blocks"][0]["text"], "Lecture with a chart");
}

#[test]
fn converted_office_uses_pdf_windows_but_download_stays_original() {
    if std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_err()
    {
        return;
    }
    let source = Arc::new(b"original-office-bytes".to_vec());
    let document = store::Document::new(
        "lecture.pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Arc::clone(&source),
        None,
    );
    assert!(document
        .prepared
        .set(Ok(Prepared::Pdf {
            bytes: Arc::new(pdf_fixture(2, false)),
            total: 2,
            text: Some(vec!["Slide one".into(), "Slide two".into()]),
            converted: true
        }))
        .is_ok());
    let study = document_result(
        Arc::clone(&document),
        &FileQuery {
            format: FileFormat::Text,
            ..Default::default()
        },
        "",
    )
    .unwrap();
    assert_eq!(study["conversion"], "local-pdf");
    assert_eq!(study["renderedPages"], 2);
    assert_eq!(study["size"], source.len());
    assert!(study["text"].as_str().unwrap().contains("Slide two"));
    let download = document_result(document, &FileQuery::default(), "").unwrap();
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(download["file"]["data"].as_str().unwrap())
            .unwrap(),
        *source
    );
    let content = mcp_content_for_file(download, "moodle");
    assert_eq!(
        content[1]["resource"]["mimeType"],
        "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    );
}
