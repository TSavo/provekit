use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn library_specific_body_templates(root: &Path) -> Vec<(String, String, PathBuf)> {
    let menagerie = root.join("menagerie");
    let mut templates = Vec::new();
    for language_dir in fs::read_dir(&menagerie).expect("read menagerie") {
        let language_dir = language_dir.expect("read language dir");
        let dirname = language_dir.file_name().to_string_lossy().into_owned();
        let Some(language) = dirname.strip_suffix("-language-signature") else {
            continue;
        };
        let template_dir = language_dir.path().join("specs").join("body-templates");
        if !template_dir.exists() {
            continue;
        }
        for entry in fs::read_dir(template_dir).expect("read body-template dir") {
            let entry = entry.expect("read body-template");
            let filename = entry.file_name().to_string_lossy().into_owned();
            let Some(library_tag) = filename
                .strip_prefix(&format!("{language}-canonical-bodies-"))
                .and_then(|rest| rest.strip_suffix(".json"))
            else {
                continue;
            };
            templates.push((language.to_string(), library_tag.to_string(), entry.path()));
        }
    }
    templates.sort_by(|a, b| a.2.cmp(&b.2));
    templates
}

#[test]
fn library_specific_body_template_entries_declare_target_tuple() {
    let root = repo_root();
    let templates = library_specific_body_templates(&root);
    assert!(
        !templates.is_empty(),
        "expected library-specific body templates under menagerie"
    );

    let mut violations = Vec::new();
    for (language, library_tag, path) in templates {
        let raw = fs::read_to_string(&path).expect("read body-template json");
        let json: Value = serde_json::from_str(&raw).expect("parse body-template json");
        let content = json
            .get("header")
            .and_then(|header| header.get("content"))
            .expect("body-template header.content exists");
        let target_language = content
            .get("target_language")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if target_language != language {
            violations.push(format!(
                "{} header.content.target_language={target_language:?}, expected {language:?}",
                path.display()
            ));
        }
        let entries = content
            .get("entries")
            .and_then(Value::as_array)
            .expect("body-template entries array exists");
        for (index, entry) in entries.iter().enumerate() {
            let target_library_tag = entry
                .get("target_library_tag")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if target_library_tag != library_tag {
                let concept = entry
                    .get("concept_name")
                    .and_then(Value::as_str)
                    .unwrap_or("<missing concept_name>");
                violations.push(format!(
                    "{} entry[{index}] {concept} target_library_tag={target_library_tag:?}, expected {library_tag:?}",
                    path.display()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "library-specific body-template entries must explicitly declare their (target_language, target_library_tag, concept_name) tuple:\n{}",
        violations.join("\n")
    );
}
