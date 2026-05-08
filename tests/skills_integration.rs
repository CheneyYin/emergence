use emergence::skills::{SkillRegistry, SkillSource};
use std::fs;
use tempfile::TempDir;

/// Verifies that SkillRegistry::load scans a directory with .md skill files.
#[test]
fn test_load_skill_from_directory() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("rust.md"), "---\nname: rust\ndescription: Rust expert\nallowed-tools: [read, write]\n---\n\nYou are a Rust expert.\n").unwrap();

    let registry = SkillRegistry::load(Some(dir.path().to_path_buf()), None).unwrap();
    let metas = registry.list();
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].name, "rust");
    assert_eq!(metas[0].source, SkillSource::User);
}

/// Verifies that SkillRegistry::load with both user and project dirs allows project to override.
#[test]
fn test_project_overrides_user_skill() {
    let user_dir = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();

    fs::write(user_dir.path().join("style.md"), "---\nname: style\ndescription: user style\n---\nuser").unwrap();
    fs::write(project_dir.path().join("style.md"), "---\nname: style\ndescription: project style\n---\nproject").unwrap();

    let registry = SkillRegistry::load(Some(user_dir.path().to_path_buf()), Some(project_dir.path().to_path_buf())).unwrap();
    let metas = registry.list();
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].source, SkillSource::Project);
    assert_eq!(metas[0].description, "project style");
}

/// Verifies that SkillRegistry::load handles a non-existent directory gracefully.
#[test]
fn test_load_nonexistent_dir() {
    let registry = SkillRegistry::load(None, None).unwrap();
    assert!(registry.list().is_empty());
}

/// Verifies that format_available_for_prompt produces expected output.
#[test]
fn test_format_for_prompt() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("ts.md"), "---\nname: typescript\ndescription: TS expert\n---\nbody").unwrap();

    let registry = SkillRegistry::load(Some(dir.path().to_path_buf()), None).unwrap();
    let text = registry.format_available_for_prompt();
    assert!(text.contains("<available_skills>"));
    assert!(text.contains("typescript"));
    assert!(text.contains("TS expert"));
}

/// Verifies that load_full_content loads body text stripped of frontmatter.
#[test]
fn test_load_full_content_through_public_api() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("my-skill.md"), "---\nname: my-skill\ndescription: desc\n---\n\nActual skill body here.\n").unwrap();

    let registry = SkillRegistry::load(Some(dir.path().to_path_buf()), None).unwrap();
    let content = registry.load_full_content("my-skill").unwrap();
    assert_eq!(content, "Actual skill body here.");
}

/// Verifies that fuzzy_match finds by exact, prefix, and contains.
#[test]
fn test_fuzzy_match_public_api() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("rust-analyzer.md"), "---\nname: rust-analyzer\ndescription: RA\n---\nbody").unwrap();

    let registry = SkillRegistry::load(Some(dir.path().to_path_buf()), None).unwrap();
    assert!(registry.fuzzy_match("rust-analyzer").is_some()); // exact
    assert!(registry.fuzzy_match("rust").is_some()); // prefix
    assert!(registry.fuzzy_match("analyzer").is_some()); // contains
    assert!(registry.fuzzy_match("python").is_none()); // no match
}
