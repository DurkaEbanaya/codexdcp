use std::path::{Path, PathBuf};
use globset::{GlobSet, GlobSetBuilder, Glob};

#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
    blocked_globs: GlobSet,
}

const DEFAULT_BLOCKED_GLOBS: &[&str] = &[
    "**/.git/**",
    "**/node_modules/**",
    "**/target/**",
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/*.p12",
    "**/*.pfx",
    "**/.codexdcp/**",
    "**/.ai-bridge/execution-log.jsonl",
];

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        let mut builder = GlobSetBuilder::new();
        for pattern in DEFAULT_BLOCKED_GLOBS {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
            }
        }
        let blocked_globs = builder.build().unwrap_or_else(|_| GlobSet::empty());
        Self { root, blocked_globs }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };

        let canonical = resolved.canonicalize().map_err(|e| format!("path not found: {} ({})", path, e))?;

        if !canonical.starts_with(&self.root) {
            return Err(format!("path outside workspace: {}", path));
        }

        if self.is_blocked(&canonical) {
            return Err(format!("access blocked: {}", path));
        }

        Ok(canonical)
    }

    pub fn resolve_for_write(&self, path: &str) -> Result<PathBuf, String> {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };

        if !resolved.starts_with(&self.root) {
            return Err(format!("path outside workspace: {}", path));
        }

        if self.is_blocked(&resolved) {
            return Err(format!("write blocked: {}", path));
        }

        Ok(resolved)
    }

    fn is_blocked(&self, path: &Path) -> bool {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        self.blocked_globs.is_match(relative) || self.blocked_globs.is_match(path)
    }

    pub fn codex_context(&self) -> String {
        let mut parts = Vec::new();

        for name in &["AGENTS.override.md", "AGENTS.md", "agents.md", ".agents.md"] {
            let path = self.root.join(name);
            if let Ok(content) = std::fs::read_to_string(&path) {
                parts.push(format!("--- {} ---\n{}", name, content));
            }
        }

        if let Ok(status) = std::process::Command::new("git")
            .arg("status").arg("--short")
            .current_dir(&self.root)
            .output()
            && status.status.success()
        {
            let s = String::from_utf8_lossy(&status.stdout);
            if !s.trim().is_empty() {
                parts.push(format!("--- git status ---\n{}", s));
            }
        }

        if parts.is_empty() {
            "No AGENTS.md or git context found in workspace.".to_string()
        } else {
            parts.join("\n\n")
        }
    }
}

impl Default for Workspace {
    fn default() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(root)
    }
}
