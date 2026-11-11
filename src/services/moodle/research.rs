//! Searchable course metadata and bounded reads of its complete textual details.
use super::{
    compact_course_contents, compact_module_files, leftover_search_tokens, normalize_tokens,
    plain_text, truncate,
};
use serde_json::{json, Value};

/// Store only navigation/content fields, not arbitrary Web Service/session data.
pub(super) fn snapshot(raw: &Value) -> Result<Value, String> {
    let sections = raw
        .as_array()
        .ok_or("Moodle course content is not a section list.")?;
    for section in sections {
        if section["id"].as_i64().is_none() || !section["modules"].is_array() {
            return Err("Moodle returned a malformed section; course coverage is unknown.".into());
        }
        if section["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["id"].as_i64().is_none())
        {
            return Err(
                "Moodle returned an activity without an ID; course coverage is unknown.".into(),
            );
        }
    }
    Ok(Value::Array(
        sections
            .iter()
            .map(|section| {
                let modules = section["modules"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|module| {
                        let mut files = compact_module_files(module);
                        for file in &mut files {
                            clean_link(file, "fileurl");
                        }
                        let inline = module["contents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|part| part["type"] == "content")
                            .filter_map(|part| part["content"].as_str())
                            .map(plain_text)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let mut links = html_links(module["description"].as_str().unwrap_or(""));
                        for part in module["contents"].as_array().into_iter().flatten() {
                            links.extend(html_links(part["content"].as_str().unwrap_or("")));
                        }
                        let rich_content = rich_content(module["description"].as_str().unwrap_or("")) ||
                            module["contents"].as_array().into_iter().flatten()
                                .any(|part| rich_content(part["content"].as_str().unwrap_or("")));
                        let mut item = json!({"id":module["id"], "modname":module["modname"],
                "name":plain_text(module["name"].as_str().unwrap_or("")),
                "description":plain_text(module["description"].as_str().unwrap_or("")),
                 "contentText":inline,"url":module["url"],"richContentOmitted":rich_content,"links":links,
                "contents":files});
                        clean_link(&mut item, "url");
                        item
                    })
                    .collect::<Vec<_>>();
                json!({"normalized":true,"id":section["id"], "section":section["section"],
            "name":plain_text(section["name"].as_str().unwrap_or("")),
             "summary":plain_text(section["summary"].as_str().unwrap_or("")),
             "richContentOmitted":rich_content(section["summary"].as_str().unwrap_or("")),
             "links":html_links(section["summary"].as_str().unwrap_or("")), "modules":modules})
            })
            .collect(),
    ))
}

fn rich_content(html: &str) -> bool {
    let html = html.to_ascii_lowercase();
    [
        "<img", "<svg", "<math", "<table", "<iframe", "<object", "<video", "<audio", "<a ",
    ]
    .iter()
    .any(|tag| html.contains(tag))
}

/// Navigation references only; never fetch links while indexing school metadata.
fn html_links(html: &str) -> Vec<Value> {
    html.split('<')
        .filter_map(|part| {
            let tag = part.split('>').next()?;
            let name = tag.split_whitespace().next()?.to_ascii_lowercase();
            let attribute = match name.as_str() {
                "a" => "href",
                "img" => "src",
                _ => return None,
            };
            let value = crate::session::html_attr(tag, attribute)?.replace("&amp;", "&");
            let mut item = json!({"kind":name,"url":value});
            clean_link(&mut item, "url");
            item.get("url")?;
            Some(item)
        })
        .collect()
}

pub(super) fn run(
    client: &super::MoodleClient,
    query: &str,
    page: i64,
    courseid: Option<i64>,
    offset: usize,
    global_only: bool,
) -> Result<Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Provide query.".into());
    }
    if page < 0 {
        return Err("page must be >= 0.".into());
    }
    let courses = client.my_courses(false)?;
    let list = courses["courses"].as_array().cloned().unwrap_or_default();
    let resolved = super::resolve_search_courses(&list, query, courseid);
    let explicit = courseid.is_some() || !resolved.is_empty();
    // If the wording does not name a course, search all visible courses in bounded windows.
    let targets = if explicit { resolved } else { list };
    let start = usize::try_from(page)
        .ok()
        .and_then(|page| page.checked_mul(8))
        .ok_or("page is too large.")?;
    let selected = if global_only {
        Vec::new()
    } else {
        targets.iter().skip(start).take(8).collect::<Vec<_>>()
    };
    if !global_only && page > 0 && start >= targets.len() && !targets.is_empty() {
        return Err("page is past the candidate course windows. Use source=global with page=0 to broaden the search.".into());
    }
    let course_only = explicit && leftover_search_tokens(query, &targets).is_empty();
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut examined = Vec::new();
    if course_only {
        results.extend(
            selected
                .iter()
                .filter_map(|course| super::course_search_hit(course)),
        );
    } else {
        // At most two concurrent requests: accelerate cold reads without flooding a school/WAF.
        for batch in selected.chunks(2) {
            std::thread::scope(|scope| {
                let jobs = batch
                    .iter()
                    .filter_map(|course| {
                        course["id"].as_i64().map(|id| {
                            (
                                *course,
                                id,
                                scope.spawn(move || client.content_snapshot(id)),
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                for (course, id, job) in jobs {
                    match job
                        .join()
                        .unwrap_or_else(|_| Err("Course index worker failed.".into()))
                    {
                        Ok((raw, index)) => {
                            results.extend(search(course, &raw, query));
                            examined.push(json!({"courseid":id,"index":index}));
                        }
                        Err(error) => errors.push(json!({"courseid":id,"error":error})),
                    }
                }
            });
        }
    }
    let end = (start + selected.len()).min(targets.len());
    let more_courses = !global_only && end < targets.len();
    let mut global = json!({"attempted":false});
    let mut global_next = None;
    if global_only || (results.is_empty() && !more_courses && !course_only) {
        let upstream_page = if global_only { page } else { 0 };
        let mut payload = json!({"query":query,"page":upstream_page});
        if courseid.is_some() || (explicit && !global_only) {
            payload["filters"] = json!({"courseids":targets.iter().filter_map(|course| course["id"].as_i64()).collect::<Vec<_>>()});
        }
        match client.call("core_search_get_results", &payload) {
            Ok(raw) => {
                if !raw["results"].is_array() {
                    errors.push(json!({"source":"global-search","error":"Moodle search response has no result list; coverage unknown."}));
                    global = json!({"attempted":true,"failed":true});
                } else {
                    let hits = super::map_global_search_results(&raw);
                    let total = raw["totalcount"].as_u64();
                    // Unknown upstream counts require another page, not an invented total.
                    if !hits.is_empty() && total != Some(hits.len() as u64) {
                        global_next = Some(
                            json!({"query":query,"courseid":courseid,"source":"global","page":upstream_page + 1,"offset":0}),
                        );
                    }
                    results.extend(hits);
                    global = json!({"attempted":true,"page":upstream_page,"totalReported":total,"warnings":raw["warnings"],
                    "note":"Upstream coverage is not guaranteed; metadata search is not a full-text search of every file."});
                    if raw["warnings"]
                        .as_array()
                        .is_some_and(|warnings| !warnings.is_empty())
                    {
                        errors.push(json!({"source":"global-search","warnings":raw["warnings"]}));
                    }
                }
            }
            Err(error) => {
                global = json!({"attempted":true,"failed":true});
                errors.push(json!({"source":"global-search","error":error}));
            }
        }
    }
    results.sort_by_key(|hit| std::cmp::Reverse(hit["score"].as_i64().unwrap_or(0)));
    let count = results.len();
    if offset > count {
        return Err("offset is past the current result window.".into());
    }
    let results = results
        .into_iter()
        .skip(offset)
        .take(40)
        .collect::<Vec<_>>();
    let more_results = offset + results.len() < count;
    let continuation = if more_results {
        Some(
            json!({"query":query,"courseid":courseid,"source":if global_only {"global"} else {"auto"},"page":page,"offset":offset + results.len()}),
        )
    } else if more_courses {
        Some(json!({"query":query,"courseid":courseid,"page":page + 1,"offset":0}))
    } else {
        global_next
    };
    Ok(
        json!({"query":query,"page":page,"offset":offset,"total":count,"returned":results.len(),"results":results,
        "courseids":selected.iter().filter_map(|course| course["id"].as_i64()).collect::<Vec<_>>(),
        "truncated":continuation.is_some(),"continuation":continuation,"partial":!errors.is_empty(),"errors":errors,
        "coverage":{"mode":"metadata-search","courseListIndex":courses["index"],"coursesExamined":examined,
            "candidateCourses":targets.len(),"candidateSelection":if explicit {"course-name/code match"} else {"all visible courses"},
            "firstCandidate":start,"endCandidate":end,"moreCourses":more_courses,
            "fileContentsSearched":false,"globalSearch":global},
        "note":"Matches are navigation leads, not proof that all instructions/files were read. For exhaustive instructions use moodle_course_content detail=full and follow nextPage; read files separately. Follow continuation to finish metadata search. No results is not proof of absence when partial or global coverage is unknown.",
        "fileurlHint":"fileurl is an internal Moodle reference. Use moodle_file and share links.download, never a Moodle token."}),
    )
}

fn clean_link(item: &mut Value, key: &str) {
    let cleaned = item[key]
        .as_str()
        .and_then(|text| url::Url::parse(text).ok())
        .and_then(|mut url| {
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            let _ = url.set_username("");
            let _ = url.set_password(None);
            let pairs = url
                .query_pairs()
                .filter(|(key, _)| {
                    !matches!(
                        key.to_ascii_lowercase().as_str(),
                        "token" | "wstoken" | "sesskey" | "password" | "accesskey"
                    )
                })
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            url.set_query(None);
            if !pairs.is_empty() {
                url.query_pairs_mut().extend_pairs(pairs);
            }
            Some(url.to_string())
        });
    if let Some(cleaned) = cleaned {
        item[key] = json!(cleaned);
    } else {
        item.as_object_mut().unwrap().remove(key);
    }
}

pub(super) fn overview(courseid: i64, raw: &Value, query: Option<&str>) -> Value {
    let mut result = compact_course_contents(courseid, raw, None);
    for section in result["sections"].as_array_mut().unwrap() {
        let source = raw
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == section["id"])
            .unwrap();
        let summary = source["summary"].as_str().unwrap_or("");
        section["summaryTruncated"] = json!(summary.chars().count() > 240);
        section["richContentOmitted"] = source["richContentOmitted"].clone();
        section["links"] = source["links"].clone();
        for module in section["modules"].as_array_mut().unwrap() {
            let source = source["modules"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == module["id"])
                .unwrap();
            let description = source["description"].as_str().unwrap_or("");
            module["description"] = json!(truncate(description, 500));
            module["descriptionTruncated"] = json!(description.chars().count() > 500);
            module["inlineContentAvailable"] =
                json!(!source["contentText"].as_str().unwrap_or("").is_empty());
            module["richContentOmitted"] = source["richContentOmitted"].clone();
            module["links"] = source["links"].clone();
        }
    }
    if let Some(query) = query {
        let tokens = leftover_search_tokens(query, &[]);
        for section in result["sections"].as_array_mut().unwrap() {
            let raw_section = raw
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == section["id"])
                .unwrap();
            let section_text = format!(
                "{} {}",
                raw_section["name"].as_str().unwrap_or(""),
                raw_section["summary"].as_str().unwrap_or("")
            );
            section["modules"].as_array_mut().unwrap().retain(|module| {
                let source = raw_section["modules"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["id"] == module["id"])
                    .unwrap();
                matches(
                    &format!(
                        "{} {} {} {} {}",
                        section_text,
                        source["name"].as_str().unwrap_or(""),
                        source["description"].as_str().unwrap_or(""),
                        source["contentText"].as_str().unwrap_or(""),
                        file_names(source)
                    ),
                    &tokens,
                )
            });
        }
        result["sections"]
            .as_array_mut()
            .unwrap()
            .retain(|section| {
                !section["modules"].as_array().unwrap().is_empty()
                    || raw
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| item["id"] == section["id"])
                        .is_some_and(|source| {
                            matches(source["summary"].as_str().unwrap_or(""), &tokens)
                        })
            });
        result["count"] = json!(result["sections"].as_array().unwrap().len());
    }
    result["coverage"] = json!({"mode":"overview", "filtered":query.is_some(), "fileContentsRead":false,
        "descriptionsComplete":false, "note":"Navigation previews only. For all instructions, request detail=full (optionally moduleid/sectionid), then follow nextPage until null. Files need the file tool."});
    result
}

fn file_names(module: &Value) -> String {
    module["contents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["filename"].as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn matches(text: &str, tokens: &[String]) -> bool {
    !tokens.is_empty() && matches_normalized(&normalize_tokens(text), tokens)
}

fn matches_normalized(hay: &[String], tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    // Keep TD5 → TD Chapitre 5, but do not let that special case ignore "corrigé".
    let td = tokens.iter().any(|token| token == "td")
        && tokens
            .iter()
            .any(|token| token.chars().all(|ch| ch.is_ascii_digit()));
    if td
        && !tokens
            .iter()
            .filter(|token| token.chars().all(|ch| ch.is_ascii_digit()))
            .all(|number| super::td_number_in(hay, number))
    {
        return false;
    }
    tokens
        .iter()
        .filter(|token| {
            !td || (token.as_str() != "td" && !token.chars().all(|ch| ch.is_ascii_digit()))
        })
        .all(|token| {
            hay.iter().any(|part| {
                part == token
                    || (token.len() >= 4
                        && part.trim_end_matches('s') == token.trim_end_matches('s'))
            })
        })
}

fn field_score(text: &str, tokens: &[String], weight: i64) -> i64 {
    let hay = normalize_tokens(text);
    if matches_normalized(&hay, tokens) {
        weight
    } else if hay.iter().any(|part| tokens.contains(part)) {
        weight / 4
    } else {
        0
    }
}

pub(super) fn search(course: &Value, raw: &Value, query: &str) -> Vec<Value> {
    let tokens = leftover_search_tokens(query, std::slice::from_ref(course));
    let mut hits = Vec::new();
    for section in raw.as_array().into_iter().flatten() {
        let section_name = section["name"].as_str().unwrap_or("");
        let summary = section["summary"].as_str().unwrap_or("");
        // Fields shared by every attachment are scored only when a hit needs them.
        let section_scores = std::cell::LazyCell::new(|| {
            [
                ("section", field_score(section_name, &tokens, 20)),
                ("summary", field_score(summary, &tokens, 10)),
            ]
        });
        if matches(&format!("{section_name} {summary}"), &tokens) {
            hits.push(json!({"area":"section", "title":section_name, "courseid":course["id"],
                "sectionid":section["id"], "score":10, "matchedIn":["section"],
                "snippet":excerpt(summary, &tokens), "contentTruncated":summary.chars().count() > 500}));
        }
        for module in section["modules"].as_array().into_iter().flatten() {
            let name = module["name"].as_str().unwrap_or("");
            let description = module["description"].as_str().unwrap_or("");
            let inline = module["contentText"].as_str().unwrap_or("");
            let context = format!("{name} {section_name} {summary} {description} {inline}");
            let shared = std::cell::LazyCell::new(|| {
                let fields = [
                    ("title", field_score(name, &tokens, 80)),
                    ("description", field_score(description, &tokens, 40)),
                    ("inlineContent", field_score(inline, &tokens, 30)),
                    section_scores[0],
                    section_scores[1],
                ];
                (fields, excerpt(&format!("{description} {inline}"), &tokens))
            });
            let files = module["contents"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let candidates = if files.is_empty() {
                std::slice::from_ref(&Value::Null)
            } else {
                files
            };
            for file in candidates {
                let filename = file["filename"].as_str().unwrap_or("");
                if !matches(&format!("{filename} {context}"), &tokens) {
                    continue;
                }
                let mut matched = Vec::new();
                let mut score = 1;
                for (field, weight) in
                    std::iter::once(("filename", field_score(filename, &tokens, 100)))
                        .chain(shared.0.iter().copied())
                {
                    if weight > 0 {
                        score += weight;
                        matched.push(field);
                    }
                }
                let mut hit = json!({"title":name, "course":course["fullname"], "courseid":course["id"],
                    "section":section_name, "sectionid":section["id"], "moduleid":module["id"],
                    "component":module["modname"], "url":module["url"], "score":score, "matchedIn":matched,
                    "area":if file.is_null() {"activity"} else {"file"},
                    "snippet":shared.1,
                    "contentTruncated":true, "fileContentsRead":false});
                if !file.is_null() {
                    hit["filename"] = file["filename"].clone();
                    hit["fileurl"] = file["fileurl"].clone();
                }
                hits.push(hit);
            }
        }
    }
    hits.sort_by_key(|hit| std::cmp::Reverse(hit["score"].as_i64().unwrap_or(0)));
    hits
}

fn excerpt(text: &str, tokens: &[String]) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>();
    let first = words
        .iter()
        .position(|word| {
            normalize_tokens(word)
                .iter()
                .any(|word| tokens.contains(word))
        })
        .unwrap_or(0);
    let start = first.saturating_sub(12);
    format!(
        "{}{}",
        if start > 0 { "… " } else { "" },
        truncate(&words[start..].join(" "), 500)
    )
}

pub(super) fn detail(
    courseid: i64,
    raw: &Value,
    moduleid: Option<i64>,
    sectionid: Option<i64>,
    page: usize,
) -> Result<Value, String> {
    // Count all blocks, but materialize only the four returned by this call.
    // Keep overflow validation after source selection to preserve error precedence.
    let mut window = DetailWindow {
        from: page.saturating_mul(4),
        total: 0,
        blocks: Vec::new(),
    };
    let mut found = false;
    for section in raw.as_array().into_iter().flatten() {
        if sectionid.is_some_and(|id| section["id"].as_i64() != Some(id)) {
            continue;
        }
        if moduleid.is_none() {
            found = true;
            window.append(
                "section",
                section,
                &section["id"],
                &[section["summary"].as_str().unwrap_or("")],
            );
        }
        for module in section["modules"].as_array().into_iter().flatten() {
            if moduleid.is_some_and(|id| module["id"].as_i64() != Some(id)) {
                continue;
            }
            found = true;
            window.append(
                "activity",
                module,
                &section["id"],
                &[
                    module["description"].as_str().unwrap_or(""),
                    "\n",
                    module["contentText"].as_str().unwrap_or(""),
                ],
            );
        }
    }
    if !found {
        return Err("No section/activity matches these IDs in this course.".into());
    }
    let from = page.checked_mul(4).ok_or("page is too large.")?;
    if from >= window.total && page > 0 {
        return Err("page is past the last detail block.".into());
    }
    let total = window.total;
    let selected = window.blocks;
    let end = from + selected.len();
    Ok(
        json!({"courseid":courseid,"moduleid":moduleid,"sectionid":sectionid,"page":page,
        "blocks":selected,"totalBlocks":total,"truncated":end < total,"nextPage":if end < total {Some(page + 1)} else {None},
        "coverage":{"mode":"detail", "firstBlock":from + 1,"lastBlock":end,"totalBlocks":total,
            "allSelectedMetadataReturned":from == 0 && end == total,"fileContentsRead":false},
        "note":"Full textual metadata, not attached/linked files. Read every block and follow nextPage. links lists recognized absolute HTTP references, not fetched content. richContentOmitted flags HTML images/media/math/tables/links not fully represented: consult the school page or source files, or disclose the limitation. Relative or unusual HTML references may require the official page. Delivered content is not proof of model comprehension."}),
    )
}

struct DetailWindow {
    from: usize,
    total: usize,
    blocks: Vec<Value>,
}

impl DetailWindow {
    fn append(&mut self, kind: &str, source: &Value, section: &Value, text: &[&str]) {
        let length: usize = text.iter().map(|part| part.chars().count()).sum();
        let parts = length.div_ceil(8_000).max(1);
        let start = self.total;
        self.total += parts;
        let first = self.from.saturating_sub(start).min(parts);
        let end = self.from.saturating_add(4).saturating_sub(start).min(parts);
        if first >= end {
            return;
        }
        let mut chars = text
            .iter()
            .flat_map(|part| part.chars())
            .skip(first * 8_000);
        for part in first..end {
            let text = chars.by_ref().take(8_000).collect::<String>();
            self.blocks.push(
                json!({"kind":kind,"id":source["id"],"title":source["name"],"sectionid":section,
            "part":part + 1,"parts":parts,"text":text,
            "files":source.get("contents").cloned().unwrap_or(json!([])),"url":source["url"],
            "richContentOmitted":source["richContentOmitted"],"links":source["links"]}),
            );
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod performance;
