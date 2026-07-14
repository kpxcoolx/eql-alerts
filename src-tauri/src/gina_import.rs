use crate::engine::{Trigger, TriggerGroup, TriggerLibrary};
use crate::eql_compat::strip_permanent_buff_timers;
use crate::starter::ensure_eql_mez_timers;
use regex::Regex;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Import a GINA `.gtp` (zip containing ShareData.xml) or a raw ShareData.xml.
pub fn import_gina_package(path: &Path) -> Result<TriggerLibrary, String> {
    let xml = load_share_xml(path)?;
    let mut library = parse_share_data(&xml)?;
    strip_permanent_buff_timers(&mut library);
    let _ = ensure_eql_mez_timers(&mut library);
    Ok(library)
}

fn load_share_xml(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "xml" {
        return std::fs::read_to_string(path).map_err(|e| format!("read xml: {e}"));
    }

    let file = File::open(path).map_err(|e| format!("open gtp: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("zip: {e}"))?;

    let mut target_idx = None;
    for i in 0..archive.len() {
        let name = archive
            .by_index(i)
            .map(|f| f.name().to_string())
            .unwrap_or_default();
        if name.ends_with("ShareData.xml") {
            target_idx = Some(i);
            break;
        }
        if target_idx.is_none() && name.to_ascii_lowercase().ends_with(".xml") {
            target_idx = Some(i);
        }
    }
    let idx = target_idx.ok_or_else(|| "No XML found inside .gtp".to_string())?;
    let mut entry = archive.by_index(idx).map_err(|e| format!("zip entry: {e}"))?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|e| format!("read ShareData.xml: {e}"))?;
    Ok(xml)
}

fn parse_share_data(xml: &str) -> Result<TriggerLibrary, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("xml parse: {e}"))?;
    let root = doc.root_element();
    let mut groups = Vec::new();

    for child in root.children() {
        if child.tag_name().name() != "TriggerGroups" {
            continue;
        }
        for group in child.children() {
            if group.tag_name().name() == "TriggerGroup" {
                walk_group(group, &[], &mut groups);
            }
        }
    }

    Ok(TriggerLibrary { groups })
}

fn walk_group(
    node: roxmltree::Node<'_, '_>,
    path: &[String],
    out: &mut Vec<TriggerGroup>,
) {
    let name = child_text(node, "Name").unwrap_or_else(|| "Unnamed".to_string());
    let mut full = path.to_vec();
    full.push(name);

    if let Some(triggers_node) = child_elem(node, "Triggers") {
        let mut converted = Vec::new();
        let path_label = full.join(" / ");
        let path_id = format!("{:x}", md5::compute(path_label.as_bytes()));
        let path_id = &path_id[..12];

        let mut idx = 0usize;
        for t in triggers_node.children() {
            if t.tag_name().name() != "Trigger" {
                continue;
            }
            if let Some(trigger) = convert_trigger(t, path_id, idx) {
                converted.push(trigger);
                idx += 1;
            } else {
                idx += 1;
            }
        }

        if !converted.is_empty() {
            out.push(TriggerGroup {
                id: path_id.to_string(),
                name: path_label,
                enabled: false,
                triggers: converted,
            });
        }
    }

    if let Some(nested) = child_elem(node, "TriggerGroups") {
        for child in nested.children() {
            if child.tag_name().name() == "TriggerGroup" {
                walk_group(child, &full, out);
            }
        }
    }
}

fn convert_trigger(
    node: roxmltree::Node<'_, '_>,
    path_id: &str,
    idx: usize,
) -> Option<Trigger> {
    let name = child_text(node, "Name").unwrap_or_else(|| format!("Trigger {idx}"));
    let search_raw = child_text(node, "TriggerText").unwrap_or_default();
    let enable_regex = child_text(node, "EnableRegex").as_deref() == Some("True");
    let comments = child_text(node, "Comments").filter(|s| !s.is_empty());

    let (search, use_regex) = prepare_pattern(&search_raw, enable_regex)?;
    // Drop patterns Rust regex cannot compile (lookaround / backrefs).
    if use_regex && Regex::new(&search).is_err() {
        return None;
    }

    let mut display = None;
    if child_text(node, "UseText").as_deref() == Some("True") {
        display = child_text(node, "DisplayText")
            .filter(|s| !s.is_empty())
            .or_else(|| Some(name.clone()));
    }

    let speak = if child_text(node, "UseTextToVoice").as_deref() == Some("True") {
        child_text(node, "TextToVoiceText").filter(|s| !s.is_empty())
    } else {
        None
    };

    // TTS-only GINA triggers: keep a toast so you see what would have been spoken.
    if display.is_none() {
        if let Some(ref text) = speak {
            display = Some(text.clone());
        }
    }

    let ttype = child_text(node, "TimerType").unwrap_or_else(|| "NoTimer".to_string());
    let ms = child_text(node, "TimerMillisecondDuration")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let mut timer_seconds = None;
    let mut timer_name = None;
    if (ttype == "Timer" || ttype == "RepeatingTimer") && ms > 0 {
        timer_seconds = Some((ms / 1000).max(1));
        timer_name = child_text(node, "TimerName")
            .filter(|s| !s.is_empty())
            .or_else(|| Some(name.clone()));
    }

    let mut early_end = Vec::new();
    if let Some(early_el) = child_elem(node, "TimerEarlyEnders") {
        for ee in early_el.children() {
            if ee.tag_name().name() != "EarlyEnder" {
                continue;
            }
            let et_raw = child_text(ee, "EarlyEndText").unwrap_or_default();
            if et_raw.is_empty() {
                continue;
            }
            let ee_regex = child_text(ee, "EnableRegex").as_deref() == Some("True");
            if let Some((et, et_regex)) = prepare_pattern(&et_raw, ee_regex || use_regex) {
                if et_regex && Regex::new(&et).is_err() {
                    continue;
                }
                early_end.push(et);
            }
        }
    }

    if display.is_none() && speak.is_none() && timer_seconds.is_none() {
        return None;
    }

    Some(Trigger {
        id: format!("{path_id}-{idx}"),
        name,
        enabled: true,
        search,
        use_regex,
        display_text: display,
        timer_seconds,
        timer_name,
        early_end,
        sound: None,
        speak: speak.clone(),
        tts_enabled: true,
        comments,
    })
}

fn prepare_pattern(text: &str, enable_regex: bool) -> Option<(String, bool)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let has_tokens = Regex::new(r"\{[CSNL]|\{COUNTER\}")
        .ok()
        .map(|re| re.is_match(text))
        .unwrap_or(false);

    if enable_regex {
        let mut s = expand_gina_tokens(text);
        s = fix_atomic_groups(&s);
        return Some((s, true));
    }
    if has_tokens {
        return Some((gina_plain_to_regex(text), true));
    }
    Some((text.to_string(), false))
}

fn expand_gina_tokens(text: &str) -> String {
    let mut s = text.to_string();
    s = Regex::new(r"\{C\}")
        .unwrap()
        .replace_all(&s, ".+?")
        .to_string();
    s = Regex::new(r"\{S\d*\}")
        .unwrap()
        .replace_all(&s, ".+?")
        .to_string();
    s = Regex::new(r"\{N(?:[><=]+\d+)?\}")
        .unwrap()
        .replace_all(&s, r"\d+")
        .to_string();
    s = Regex::new(r"\{L\}")
        .unwrap()
        .replace_all(&s, ".+")
        .to_string();
    s = Regex::new(r"\{COUNTER\}")
        .unwrap()
        .replace_all(&s, r"\d+")
        .to_string();
    s
}

fn gina_plain_to_regex(text: &str) -> String {
    // Replace tokens with placeholders, regex-escape the rest, restore.
    let token_re = Regex::new(r"\{C\}|\{S\d*\}|\{N(?:[><=]+\d+)?\}|\{L\}|\{COUNTER\}").unwrap();
    let mut placeholders = Vec::new();
    let marked = token_re.replace_all(text, |caps: &regex::Captures| {
        let key = format!("\u{E000}{}\u{E001}", placeholders.len());
        let repl = match &caps[0] {
            "{L}" => ".+",
            "{COUNTER}" => r"\d+",
            t if t.starts_with("{N") => r"\d+",
            _ => ".+?",
        };
        placeholders.push(repl.to_string());
        key
    });
    let mut escaped = regex::escape(&marked);
    for (i, repl) in placeholders.iter().enumerate() {
        let key = regex::escape(&format!("\u{E000}{i}\u{E001}"));
        escaped = escaped.replace(&key, repl);
    }
    format!("^{escaped}$")
}

fn fix_atomic_groups(s: &str) -> String {
    if !s.contains("(?>") {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i] == '(' && chars[i + 1] == '?' && chars[i + 2] == '>' {
            let mut depth = 0;
            let mut j = i + 3;
            let start = j;
            let mut found = false;
            while j < chars.len() {
                if chars[j] == '\\' && j + 1 < chars.len() {
                    j += 2;
                    continue;
                }
                if chars[j] == '(' {
                    depth += 1;
                } else if chars[j] == ')' {
                    if depth == 0 {
                        let inner: String = chars[start..j].iter().collect();
                        out.push_str("(?:");
                        out.push_str(&fix_atomic_groups(&inner));
                        out.push(')');
                        i = j + 1;
                        found = true;
                        break;
                    }
                    depth -= 1;
                }
                j += 1;
            }
            if !found {
                out.push(chars[i]);
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn child_elem<'a, 'input: 'a>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|c| c.tag_name().name() == name)
}

fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    child_elem(node, name).and_then(|n| n.text().map(|t| t.to_string()))
}

/// Merge imported groups into an existing library (by group id).
pub fn merge_libraries(base: &TriggerLibrary, imported: TriggerLibrary) -> TriggerLibrary {
    let mut groups = base.groups.clone();
    for incoming in imported.groups {
        if let Some(existing) = groups.iter_mut().find(|g| g.id == incoming.id) {
            *existing = incoming;
        } else {
            groups.push(incoming);
        }
    }
    TriggerLibrary { groups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn imports_gina_pack() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../GINA/gina_pack.gtp");
        if !path.exists() {
            return;
        }
        let lib = import_gina_package(&path).expect("import");
        assert!(lib.groups.len() > 50);
        let total: usize = lib.groups.iter().map(|g| g.triggers.len()).sum();
        assert!(total > 500, "expected hundreds of triggers, got {total}");
        // All enabled flags on groups should be false (opt-in like GINA).
        assert!(lib.groups.iter().all(|g| !g.enabled));
    }

    #[test]
    fn fixes_atomic_groups() {
        let s = r"^You have been slain by (?>[^!]+)\!$";
        assert_eq!(
            fix_atomic_groups(s),
            r"^You have been slain by (?:[^!]+)\!$"
        );
    }
}
