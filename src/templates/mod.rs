use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use toml_edit::de::from_str;

// Embedded template files (name, content)
pub const EMBEDDED_TEMPLATES: &[(&str, &str)] = &[
    ("pnpm", include_str!("./pnpm.plz.toml")),
    ("npm", include_str!("./npm.plz.toml")),
    ("uv", include_str!("./uv.plz.toml")),
    ("rust", include_str!("./rust.plz.toml")),
];

pub const EMBEDDED_ENVIRONMENTS: &str = include_str!("../environments.toml");

pub const EMBEDDED_SNIPPETS: &[(&str, &str)] = &[
    ("general", include_str!("../snippets/general.toml")),
    ("rust", include_str!("../snippets/rust.toml")),
    ("pnpm", include_str!("../snippets/pnpm.toml")),
    ("npm", include_str!("../snippets/npm.toml")),
    ("uv", include_str!("../snippets/uv.toml")),
];

#[derive(Debug, Clone, Deserialize)]
pub struct Environment {
    pub patterns: Vec<String>,
    #[serde(default)]
    pub alternative_to: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TemplateMeta {
    pub name: String,
    pub description: String,
    pub env: String,
    pub content: String,
    pub is_user: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct TemplateHeader {
    description: String,
    env: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TemplateFile {
    template: TemplateHeader,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Snippet {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SnippetFile {
    snippets: Vec<Snippet>,
}

pub fn load_environments() -> HashMap<String, Environment> {
    from_str(EMBEDDED_ENVIRONMENTS).unwrap_or_default()
}

pub fn detect_environments(cwd: &Path, environments: &HashMap<String, Environment>) -> Vec<String> {
    let mut detected = Vec::new();
    // Sort for deterministic order
    let mut env_names: Vec<&String> = environments.keys().collect();
    env_names.sort();
    for name in env_names {
        let env = &environments[name];
        if env.patterns.iter().any(|p| cwd.join(p).exists()) {
            detected.push(name.clone());
        }
    }
    detected
}

pub fn load_templates(config_dir: Option<&Path>) -> Vec<TemplateMeta> {
    let mut templates = Vec::new();

    // Load user template first so it can be prioritized for its env
    let mut user_template: Option<TemplateMeta> = None;
    if let Some(dir) = config_dir {
        let user_path = dir.join("user.plz.toml");
        if let Ok(content) = std::fs::read_to_string(&user_path)
            && let Some(meta) = parse_template_meta("user", &content, true)
        {
            user_template = Some(meta);
        }
    }

    for (name, content) in EMBEDDED_TEMPLATES {
        // If user template matches this env, insert user template before it
        if let Some(ref ut) = user_template
            && ut.env == *name
        {
            templates.push(ut.clone());
        }
        if let Some(meta) = parse_template_meta(name, content, false) {
            templates.push(meta);
        }
    }

    // If user template env doesn't match any embedded template, append it
    if let Some(ref ut) = user_template
        && !EMBEDDED_TEMPLATES.iter().any(|(name, _)| *name == ut.env)
    {
        templates.push(ut.clone());
    }

    templates
}

fn parse_template_meta(name: &str, content: &str, is_user: bool) -> Option<TemplateMeta> {
    let header: TemplateFile = from_str(content).ok()?;
    Some(TemplateMeta {
        name: name.to_string(),
        description: header.template.description,
        env: header.template.env,
        content: content.to_string(),
        is_user,
    })
}

pub fn strip_template_section(content: &str) -> String {
    let mut result = String::new();
    let mut in_template_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[template]" {
            in_template_section = true;
            continue;
        }
        if in_template_section {
            // We're past [template], skip key = value lines until next section or blank
            if trimmed.starts_with('[') {
                in_template_section = false;
                // fall through to add this line
            } else if trimmed.is_empty() {
                in_template_section = false;
                continue;
            } else {
                continue;
            }
        }
        result.push_str(line);
        result.push('\n');
    }

    // Trim leading whitespace
    let trimmed = result.trim_start();
    trimmed.to_string()
}

pub fn load_snippets() -> Vec<(String, Vec<Snippet>)> {
    let mut all_snippets: Vec<(String, Vec<Snippet>)> = Vec::new();

    for (name, content) in EMBEDDED_SNIPPETS {
        if let Ok(file) = from_str::<SnippetFile>(content) {
            all_snippets.push((name.to_string(), file.snippets));
        }
    }

    all_snippets
}
