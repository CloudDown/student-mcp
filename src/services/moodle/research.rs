//! Searchable course metadata and bounded reads of its complete textual details.
use super::{compact_module_files, leftover_search_tokens, normalize_tokens, plain_text, truncate};
use serde_json::{json, Value};

/// Store only navigation/content fields, not arbitrary Web Service/session data.
/// Delegated `mod_subsection` sections are nested under their parent activity.
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
    let flat = sections
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
                    let rich_content = rich_content(module["description"].as_str().unwrap_or(""))
                        || module["contents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|part| rich_content(part["content"].as_str().unwrap_or("")));
                    let availability = module["availability"].as_str().unwrap_or("");
                    let (group_ids, grouping_ids) = availability_groups(availability);
                    let groupmode = module["groupmode"].as_i64().unwrap_or(0);
                    let grouping = module["groupingid"].as_i64().unwrap_or(0);
                    let mut item = json!({"id":module["id"], "modname":module["modname"],
                        "instance":module["instance"],
                        "name":plain_text(module["name"].as_str().unwrap_or("")),
                        "description":plain_text(module["description"].as_str().unwrap_or("")),
                        "contentText":inline,"url":module["url"],"richContentOmitted":rich_content,"links":links,
                        "contents":files,
                        "uservisible":module["uservisible"].as_bool().unwrap_or(true),
                        "groupRestricted":groupmode == 1 || grouping > 0 || !group_ids.is_empty() || !grouping_ids.is_empty(),
                        "groupIds":group_ids,"groupingIds":grouping_ids});
                    if module["subsections"].is_array() {
                        item["subsections"] = module["subsections"].clone();
                    }
                    clean_link(&mut item, "url");
                    item
                })
                .collect::<Vec<_>>();
            let (group_ids, grouping_ids) =
                availability_groups(section["availability"].as_str().unwrap_or(""));
            let item = json!({"normalized":true,"id":section["id"], "section":section["section"],
                "component":section["component"], "itemid":section["itemid"],
                "name":plain_text(section["name"].as_str().unwrap_or("")),
                "summary":plain_text(section["summary"].as_str().unwrap_or("")),
                "richContentOmitted":rich_content(section["summary"].as_str().unwrap_or("")),
                "links":html_links(section["summary"].as_str().unwrap_or("")), "modules":modules,
                "uservisible":section["uservisible"].as_bool().unwrap_or(true),
                "groupRestricted":!group_ids.is_empty() || !grouping_ids.is_empty(),
                "groupIds":group_ids,"groupingIds":grouping_ids});
            item
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(nest_subsections(flat)))
}

/// Parse standard Moodle availability JSON for group/grouping conditions only.
fn availability_groups(raw: &str) -> (Vec<i64>, Vec<i64>) {
    fn walk(value: &Value, groups: &mut Vec<i64>, groupings: &mut Vec<i64>) {
        match value {
            Value::Object(map) => {
                let kind = map.get("type").and_then(Value::as_str);
                let id = map.get("id").and_then(Value::as_i64).filter(|id| *id > 0);
                match (kind, id) {
                    (Some("group"), Some(id)) => groups.push(id),
                    (Some("grouping"), Some(id)) => groupings.push(id),
                    _ => {}
                }
                for child in map.values() {
                    walk(child, groups, groupings);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, groups, groupings);
                }
            }
            _ => {}
        }
    }
    let mut groups = Vec::new();
    let mut groupings = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        walk(&value, &mut groups, &mut groupings);
    }
    groups.sort_unstable();
    groups.dedup();
    groupings.sort_unstable();
    groupings.dedup();
    (groups, groupings)
}

/// Attach delegated sections to the `mod_subsection` module that owns them.
fn nest_subsections(flat: Vec<Value>) -> Vec<Value> {
    let mut parent: Vec<Option<(usize, usize)>> = vec![None; flat.len()];
    for (section_index, section) in flat.iter().enumerate() {
        let component = section["component"].as_str().unwrap_or("");
        let itemid = section["itemid"].as_i64().unwrap_or(0);
        if component != "mod_subsection" || itemid <= 0 {
            continue;
        }
        'search: for (candidate_index, candidate) in flat.iter().enumerate() {
            for (module_index, module) in candidate["modules"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                if module["modname"] == "subsection"
                    && (module["instance"].as_i64() == Some(itemid)
                        || module["customdata"]["sectionid"].as_i64()
                            == flat[section_index]["id"].as_i64())
                {
                    parent[section_index] = Some((candidate_index, module_index));
                    break 'search;
                }
            }
        }
    }
    fn build(
        index: usize,
        depth: usize,
        flat: &[Value],
        parent: &[Option<(usize, usize)>],
    ) -> Value {
        let mut section = flat[index].clone();
        if depth > 6 {
            return section;
        }
        let mut modules = section["modules"].as_array().cloned().unwrap_or_default();
        for (module_index, module) in modules.iter_mut().enumerate() {
            let children = parent
                .iter()
                .enumerate()
                .filter(|(_, slot)| **slot == Some((index, module_index)))
                .map(|(child, _)| build(child, depth + 1, flat, parent))
                .collect::<Vec<_>>();
            if !children.is_empty() {
                module["subsections"] = Value::Array(children);
            }
        }
        section["modules"] = Value::Array(modules);
        section
    }
    let nested = flat
        .iter()
        .enumerate()
        .filter(|(index, _)| parent[*index].is_none())
        .map(|(index, _)| build(index, 0, &flat, &parent))
        .collect::<Vec<_>>();
    // Orphaned delegated sections must not disappear if the parent is malformed.
    if nested.is_empty() {
        flat
    } else {
        nested
    }
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
    let mut cached_courses = 0usize;
    let mut fresh_courses = 0usize;
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
                            if index["status"] == "cached" {
                                cached_courses += 1;
                            } else {
                                fresh_courses += 1;
                            }
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
        "coverage":{"mode":"metadata-search","coursesExamined":selected.len(),
            "cachedCourses":cached_courses,"freshCourses":fresh_courses,
            "courseListCache":courses["index"]["status"],
            "candidateCourses":targets.len(),"candidateSelection":if explicit {"course-name/code match"} else {"all visible courses"},
            "firstCandidate":start,"endCandidate":end,"moreCourses":more_courses,
            "fileContentsSearched":false,"pdfTextSearched":false,
            "pdfTextNote":"PDF contents are not searched and no PDF text cache is consulted. Read a PDF with moodle_file format=text before searching inside it.",
            "globalSearch":global},
        "note":"Matches are navigation leads, not proof that all instructions/files were read. For exhaustive instructions use moodle_course_content detail=full and follow nextPage; read files separately. Follow continuation to finish metadata search. No results is not proof of absence when partial or global coverage is unknown.",
        "fileurlHint":"fileurl is an internal Moodle reference. Use moodle_file, never a Moodle token. Share its temporary link only if the user asks for a download."}),
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

#[allow(clippy::too_many_arguments)]
pub(super) fn overview(
    courseid: i64,
    raw: &Value,
    query: Option<&str>,
    groups: &Value,
    group_only: bool,
    page: usize,
    base_url: &str,
    sectionid: Option<i64>,
) -> Result<Value, String> {
    const PAGE_SECTIONS: usize = 6;
    let mut sections = raw
        .as_array()
        .into_iter()
        .flatten()
        .filter(|section| !(group_only && group_hidden(section)))
        .map(|section| overview_section(courseid, section, group_only, base_url))
        .filter(|section| section["allModulesFiltered"] != true)
        .collect::<Vec<_>>();
    if let Some(query) = query {
        let tokens = leftover_search_tokens(query, &[]);
        sections.retain_mut(|section| prune_section(section, &tokens));
    }
    if let Some(sectionid) = sectionid {
        let found = find_section(&sections, sectionid);
        sections = found.into_iter().collect();
    }
    let total = sections.len();
    let from = page
        .checked_mul(PAGE_SECTIONS)
        .ok_or("page is too large.")?;
    if from >= total && page > 0 {
        return Err("page is past the last section page. Omit page or start at 0.".into());
    }
    let selected = sections
        .into_iter()
        .skip(from)
        .take(PAGE_SECTIONS)
        .collect::<Vec<_>>();
    let end = from + selected.len();
    Ok(json!({
        "courseid": courseid,
        "count": selected.len(),
        "page": page,
        "totalSections": total,
        "truncated": end < total,
        "nextPage": if end < total { Some(page + 1) } else { None },
        "sections": selected,
        "groups": groups,
        "groupOnly": group_only,
        "coverage": {
            "mode": "overview",
            "filtered": query.is_some(),
            "sectionid": sectionid,
            "fileContentsRead": false,
            "descriptionsComplete": false,
            "firstSection": from + 1,
            "lastSection": end,
            "totalSections": total,
            "note": "Navigation previews only, with shortened descriptions and bounded module/file lists. Subsections are nested under their parent activity. Follow nextPage for other sections; moduleCount/fileCount show omissions. For all instructions, request detail=full (optionally moduleid/sectionid), then follow nextPage until null. Files need the file tool. Section urls open the Moodle page directly. group_only removes only sources Moodle marks invisible through group/grouping restrictions; no group is inferred from names."
        },
        "fileurlHint": "fileurl is an internal Moodle reference. Use moodle_file, never a Moodle token."
    }))
}

fn group_hidden(item: &Value) -> bool {
    item["groupRestricted"].as_bool() == Some(true) && item["uservisible"].as_bool() == Some(false)
}

fn overview_section(courseid: i64, section: &Value, group_only: bool, base_url: &str) -> Value {
    const MAX_MODULES: usize = 8;
    const MAX_FILES: usize = 6;
    let summary = section["summary"].as_str().unwrap_or("");
    let mut modules = Vec::new();
    let mut latest = 0;
    for module in section["modules"].as_array().into_iter().flatten() {
        if group_only && group_hidden(module) {
            continue;
        }
        let description = module["description"].as_str().unwrap_or("");
        let files = module["contents"].as_array().cloned().unwrap_or_default();
        if let Some(modified) = files
            .iter()
            .filter_map(|file| file["timemodified"].as_i64())
            .max()
        {
            latest = latest.max(modified);
        }
        let mut item = json!({
            "id": module["id"],
            "modname": module["modname"],
            "name": module["name"],
            "description": truncate(description, 160),
            "descriptionTruncated": description.chars().count() > 160,
            "inlineContentAvailable": !module["contentText"].as_str().unwrap_or("").is_empty(),
            "url": module["url"],
            "richContentOmitted": module["richContentOmitted"],
            "links": module["links"],
            "uservisible": module["uservisible"],
            "groupRestricted": module["groupRestricted"],
            "groupIds": module["groupIds"],
            "groupingIds": module["groupingIds"],
            "fileCount": files.len(),
            "files": files.iter().take(MAX_FILES).cloned().collect::<Vec<_>>(),
        });
        if files.len() > MAX_FILES {
            item["filesTruncated"] = json!(true);
        }
        let children = module["subsections"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|child| !(group_only && group_hidden(child)))
            .map(|child| overview_section(courseid, child, group_only, base_url))
            .filter(|child| child["allModulesFiltered"] != true)
            .collect::<Vec<_>>();
        if !children.is_empty() {
            item["subsections"] = Value::Array(children);
        }
        modules.push(item);
    }
    let module_count = modules.len();
    if modules.len() > MAX_MODULES {
        modules.truncate(MAX_MODULES);
    }
    let mut item = json!({
        "id": section["id"],
        "section": section["section"],
        "name": section["name"],
        "summary": truncate(summary, 240),
        "summaryTruncated": summary.chars().count() > 240,
        "richContentOmitted": section["richContentOmitted"],
        "links": section["links"],
        "url": section_link(base_url, courseid, section["section"].as_i64().unwrap_or(0)),
        "uservisible": section["uservisible"],
        "groupRestricted": section["groupRestricted"],
        "groupIds": section["groupIds"],
        "groupingIds": section["groupingIds"],
        "moduleCount": module_count,
        "modules": modules,
    });
    if module_count > MAX_MODULES {
        item["modulesTruncated"] = json!(true);
    }
    if group_only
        && module_count == 0
        && !section["modules"].as_array().is_none_or(|m| m.is_empty())
    {
        item["allModulesFiltered"] = json!(true);
    }
    if latest > 0 {
        item["lastModified"] = json!(latest);
    }
    item
}

/// Remove non-matching activities/subsections; keep a section with a matching summary.
fn prune_section(section: &mut Value, tokens: &[String]) -> bool {
    let section_text = format!(
        "{} {}",
        section["name"].as_str().unwrap_or(""),
        section["summary"].as_str().unwrap_or("")
    );
    let mut keep = matches(&section_text, tokens);
    let modules = section["modules"].as_array_mut().unwrap();
    modules.retain_mut(|module| module_matches(module, tokens));
    keep |= !modules.is_empty();
    keep
}

fn module_matches(module: &Value, tokens: &[String]) -> bool {
    let mut text = format!(
        "{} {} {} {}",
        module["name"].as_str().unwrap_or(""),
        module["description"].as_str().unwrap_or(""),
        module["contentText"].as_str().unwrap_or(""),
        file_names(module)
    );
    let mut children = module["subsections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    children.retain_mut(|child| prune_section(child, tokens));
    if !children.is_empty() {
        text.push(' ');
        text.push_str(&serde_json::to_string(&children).unwrap_or_default());
    }
    matches(&text, tokens)
}

fn find_section(sections: &[Value], id: i64) -> Option<Value> {
    for section in sections {
        if section["id"].as_i64() == Some(id) {
            return Some(section.clone());
        }
        for module in section["modules"].as_array().into_iter().flatten() {
            if let Some(children) = module["subsections"].as_array() {
                if let Some(found) = find_section(children, id) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn section_link(base_url: &str, courseid: i64, section: i64) -> String {
    format!(
        "{}/course/view.php?id={courseid}&section={section}",
        base_url.trim_end_matches('/')
    )
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
    search_sections(
        course,
        raw.as_array().map(Vec::as_slice).unwrap_or(&[]),
        &tokens,
        &mut hits,
        None,
    );
    hits.sort_by_key(|hit| std::cmp::Reverse(hit["score"].as_i64().unwrap_or(0)));
    hits
}

fn search_sections(
    course: &Value,
    sections: &[Value],
    tokens: &[String],
    hits: &mut Vec<Value>,
    parent: Option<&str>,
) {
    for section in sections {
        let section_name = section["name"].as_str().unwrap_or("");
        let summary = section["summary"].as_str().unwrap_or("");
        // Fields shared by every attachment are scored only when a hit needs them.
        let section_scores = std::cell::LazyCell::new(|| {
            [
                ("section", field_score(section_name, tokens, 20)),
                ("summary", field_score(summary, tokens, 10)),
            ]
        });
        if matches(&format!("{section_name} {summary}"), tokens) {
            let mut hit = json!({"area":"section", "title":section_name, "courseid":course["id"],
                "sectionid":section["id"], "score":10, "matchedIn":["section"],
                "snippet":excerpt(summary, tokens), "contentTruncated":summary.chars().count() > 500});
            if let Some(parent) = parent {
                hit["subsection"] = json!(parent);
            }
            hits.push(hit);
        }
        for module in section["modules"].as_array().into_iter().flatten() {
            let name = module["name"].as_str().unwrap_or("");
            let description = module["description"].as_str().unwrap_or("");
            let inline = module["contentText"].as_str().unwrap_or("");
            let context = format!("{name} {section_name} {summary} {description} {inline}");
            let shared = std::cell::LazyCell::new(|| {
                let fields = [
                    ("title", field_score(name, tokens, 80)),
                    ("description", field_score(description, tokens, 40)),
                    ("inlineContent", field_score(inline, tokens, 30)),
                    section_scores[0],
                    section_scores[1],
                ];
                (fields, excerpt(&format!("{description} {inline}"), tokens))
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
                if !matches(&format!("{filename} {context}"), tokens) {
                    continue;
                }
                let mut matched = Vec::new();
                let mut score = 1;
                for (field, weight) in
                    std::iter::once(("filename", field_score(filename, tokens, 100)))
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
                if let Some(parent) = parent {
                    hit["subsection"] = json!(parent);
                }
                hits.push(hit);
            }
            if let Some(children) = module["subsections"].as_array() {
                search_sections(course, children, tokens, hits, Some(name));
            }
        }
    }
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
    groups: &Value,
    base_url: &str,
) -> Result<Value, String> {
    // Count all blocks, but materialize only the four returned by this call.
    // Keep overflow validation after source selection to preserve error precedence.
    let mut window = DetailWindow {
        from: page.saturating_mul(4),
        total: 0,
        blocks: Vec::new(),
    };
    let mut found = false;
    collect_detail(
        raw.as_array().map(Vec::as_slice).unwrap_or(&[]),
        moduleid,
        sectionid,
        &mut window,
        courseid,
        base_url,
        0,
        &mut found,
    );
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
        "groups":groups,
        "coverage":{"mode":"detail", "firstBlock":from + 1,"lastBlock":end,"totalBlocks":total,
            "allSelectedMetadataReturned":from == 0 && end == total,"fileContentsRead":false},
        "note":"Full textual metadata, not attached/linked files. Read every block and follow nextPage. links lists recognized absolute HTTP references, not fetched content. richContentOmitted flags HTML images/media/math/tables/links not fully represented: consult the school page or source files, or disclose the limitation. Relative or unusual HTML references may require the official page. Delivered content is not proof of model comprehension."}),
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_detail(
    sections: &[Value],
    moduleid: Option<i64>,
    sectionid: Option<i64>,
    window: &mut DetailWindow,
    courseid: i64,
    base_url: &str,
    depth: usize,
    found: &mut bool,
) {
    if depth > 8 {
        return;
    }
    for section in sections {
        let section_selected = sectionid.is_none_or(|id| section["id"].as_i64() == Some(id));
        if section_selected && moduleid.is_none() {
            *found = true;
            window.append(
                "section",
                section,
                &section["id"],
                &[section["summary"].as_str().unwrap_or("")],
                Some(&section_link(
                    base_url,
                    courseid,
                    section["section"].as_i64().unwrap_or(0),
                )),
            );
        }
        for module in section["modules"].as_array().into_iter().flatten() {
            let module_selected = moduleid.is_none_or(|id| module["id"].as_i64() == Some(id));
            let children = module["subsections"].as_array().map(Vec::as_slice);
            if section_selected && module_selected {
                *found = true;
                match children {
                    // A subsection module has no text of its own: read its activities.
                    Some(children) => collect_detail(
                        children,
                        None,
                        None,
                        window,
                        courseid,
                        base_url,
                        depth + 1,
                        found,
                    ),
                    None => window.append(
                        "activity",
                        module,
                        &section["id"],
                        &[
                            module["description"].as_str().unwrap_or(""),
                            "\n",
                            module["contentText"].as_str().unwrap_or(""),
                        ],
                        None,
                    ),
                }
            } else if let Some(children) = children {
                collect_detail(
                    children,
                    moduleid,
                    sectionid,
                    window,
                    courseid,
                    base_url,
                    depth + 1,
                    found,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn detail_matches(
    courseid: i64,
    raw: &Value,
    module_ids: &[i64],
    section_ids: &[i64],
    page: usize,
    groups: &Value,
    base_url: &str,
) -> Result<Value, String> {
    let modules = module_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let sections = section_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut window = DetailWindow {
        from: page.saturating_mul(4),
        total: 0,
        blocks: Vec::new(),
    };
    let mut found = false;
    collect_matches(
        raw.as_array().map(Vec::as_slice).unwrap_or(&[]),
        &modules,
        &sections,
        &mut window,
        courseid,
        base_url,
        0,
        &mut found,
    );
    let from = page.checked_mul(4).ok_or("page is too large.")?;
    if from >= window.total && page > 0 {
        return Err("page is past the last detail block.".into());
    }
    let total = window.total;
    let selected = window.blocks;
    let end = from + selected.len();
    Ok(json!({
        "courseid": courseid, "page": page, "blocks": selected,
        "totalBlocks": total, "truncated": end < total,
        "nextPage": if end < total { Some(page + 1) } else { None },
        "groups": groups,
        "coverage": {"mode": "detail-query", "firstBlock": from + 1, "lastBlock": end,
            "totalBlocks": total, "matchedModules": module_ids.len(),
            "matchedSections": section_ids.len(), "fileContentsRead": false},
        "note": if found {
            "Complete textual metadata for the elements matching the query; follow nextPage for all blocks. Attached files still require the file tool."
        } else {
            "No course element matched this query. Try the overview or a broader term; absence here is not proof that the item does not exist."
        }
    }))
}

#[allow(clippy::too_many_arguments)]
fn collect_matches(
    sections: &[Value],
    modules: &std::collections::HashSet<i64>,
    section_ids: &std::collections::HashSet<i64>,
    window: &mut DetailWindow,
    courseid: i64,
    base_url: &str,
    depth: usize,
    found: &mut bool,
) {
    if depth > 8 {
        return;
    }
    for section in sections {
        if section_ids.contains(&section["id"].as_i64().unwrap_or(0)) {
            *found = true;
            window.append(
                "section",
                section,
                &section["id"],
                &[section["summary"].as_str().unwrap_or("")],
                Some(&section_link(
                    base_url,
                    courseid,
                    section["section"].as_i64().unwrap_or(0),
                )),
            );
        }
        for module in section["modules"].as_array().into_iter().flatten() {
            if modules.contains(&module["id"].as_i64().unwrap_or(0)) {
                *found = true;
                match module["subsections"].as_array().map(Vec::as_slice) {
                    Some(children) => collect_detail(
                        children,
                        None,
                        None,
                        window,
                        courseid,
                        base_url,
                        depth + 1,
                        found,
                    ),
                    None => window.append(
                        "activity",
                        module,
                        &section["id"],
                        &[
                            module["description"].as_str().unwrap_or(""),
                            "\n",
                            module["contentText"].as_str().unwrap_or(""),
                        ],
                        None,
                    ),
                }
            } else if let Some(children) = module["subsections"].as_array() {
                collect_matches(
                    children,
                    modules,
                    section_ids,
                    window,
                    courseid,
                    base_url,
                    depth + 1,
                    found,
                );
            }
        }
    }
}

struct DetailWindow {
    from: usize,
    total: usize,
    blocks: Vec<Value>,
}

impl DetailWindow {
    fn append(
        &mut self,
        kind: &str,
        source: &Value,
        section: &Value,
        text: &[&str],
        fallback_url: Option<&str>,
    ) {
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
            let url = source
                .get("url")
                .filter(|value| !value.is_null())
                .cloned()
                .or_else(|| fallback_url.map(|url| json!(url)))
                .unwrap_or(Value::Null);
            self.blocks.push(
                json!({"kind":kind,"id":source["id"],"title":source["name"],"sectionid":section,
            "part":part + 1,"parts":parts,"text":text,
            "files":source.get("contents").cloned().unwrap_or(json!([])),"url":url,
            "richContentOmitted":source["richContentOmitted"],"links":source["links"]}),
            );
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod performance;
