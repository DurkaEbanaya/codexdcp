use crate::workspace::Workspace;
use std::path::PathBuf;

const SKILL_DIRS: &[&str] = &[
    ".opencode/skills",
    ".codex/skills",
    "skills",
    ".skills",
];

const USER_SKILL_DIRS: &[&str] = &[
    ".config/opencode/skills",
    ".codex/skills",
];

pub fn list_skills(ws: &Workspace) -> Vec<SkillInfo> {
    let mut skills = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    let workspace_dirs: Vec<PathBuf> = SKILL_DIRS.iter()
        .map(|d| ws.root().join(d))
        .collect();

    let user_dirs: Vec<PathBuf> = USER_SKILL_DIRS.iter()
        .map(|d| PathBuf::from(&home).join(d))
        .collect();

    for dir in workspace_dirs.iter().chain(user_dirs.iter()) {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let source = if path.starts_with(ws.root()) {
                        "workspace"
                    } else {
                        "user"
                    };
                    let description = std::fs::read_to_string(&skill_md)
                        .ok()
                        .and_then(|c| {
                            c.lines()
                                .find(|l| l.to_lowercase().starts_with("description:"))
                                .map(|l| l.trim_start_matches("description:").trim().to_string())
                        })
                        .unwrap_or_default();
                    skills.push(SkillInfo {
                        name,
                        description,
                        source: source.to_string(),
                        path: skill_md,
                    });
                }
            }
        }
    }

    skills
}

pub fn load_skill(ws: &Workspace, name: &str) -> Result<String, String> {
    let home = std::env::var("HOME").unwrap_or_default();

    let mut search_dirs: Vec<PathBuf> = SKILL_DIRS.iter()
        .map(|d| ws.root().join(d).join(name))
        .collect();

    for ud in USER_SKILL_DIRS.iter() {
        search_dirs.push(PathBuf::from(&home).join(ud).join(name));
    }

    for dir in &search_dirs {
        let skill_md = dir.join("SKILL.md");
        if skill_md.exists() {
            let content = std::fs::read_to_string(&skill_md)
                .map_err(|e| format!("failed to read skill '{}': {}", name, e))?;
            let content = if content.len() > 16000 {
                let mut truncated = content[..16000].to_string();
                truncated.push_str("\n... (truncated)");
                truncated
            } else {
                content
            };
            return Ok(format!("--- Skill: {} ---\n{}", name, content));
        }
    }

    let available = list_skills(ws);
    if available.is_empty() {
        Err(format!("skill '{}' not found. No skills discovered in workspace.", name))
    } else {
        let names: Vec<String> = available.iter()
            .map(|s| format!("  - {} ({}): {}", s.name, s.source, s.description))
            .collect();
        Err(format!("skill '{}' not found. Available skills:\n{}", name, names.join("\n")))
    }
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: PathBuf,
}
