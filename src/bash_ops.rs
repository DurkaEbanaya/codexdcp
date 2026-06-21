use crate::workspace::Workspace;
use std::process::Command;

const BLOCKED_COMMANDS: &[&str] = &[
    "rm -rf", "rm -r ", "rmdir",
    "git push", "git reset --hard", "git clean -f", "git clean -d",
    "curl ", "wget ",
    "ssh ", "scp ",
    "docker ", "kubectl ",
    "find . -exec", "find . -delete", "find / -",
    "mkfs", "dd if=", "shutdown", "reboot",
    "kill -9", "killall",
    "> /dev/sd", "chmod 777",
    "eval ", "exec ",
    "source ", ". ",
    "cat ", "head ", "tail ",
    "sudo ",
];

const ALLOWED_PREFIXES: &[&str] = &[
    "cargo ", "cargo", "rustc ",
    "npm ", "npx ", "yarn ", "pnpm ", "node ", "deno ", "bun ",
    "python ", "python3 ", "pip ", "pip3 ", "poetry ", "uv ",
    "go ", "rustup ",
    "make ", "cmake ",
    "gcc ", "clang ", "g++ ",
    "tsc ", "eslint ", "prettier ", "ruff ", "black ", "mypy ",
    "pytest ", "jest ", "vitest ", "cargo test", "cargo clippy",
    "git status", "git diff", "git log", "git branch", "git show",
    "git add", "git stash", "git merge-base",
    "rg ", "grep ", "fd ", "find ", "ls ", "echo ", "pwd",
    "wc ", "sort ", "uniq ", "diff ",
    "mkdir ", "touch ", "cp ", "mv ",
    "cd ",
    "which ", "file ",
    "du ", "df ",
    "env ", "printenv",
];

#[derive(Clone, Debug, PartialEq)]
pub enum BashMode {
    Safe,
    Off,
    Full,
}

impl BashMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "safe" => Some(Self::Safe),
            "off" => Some(Self::Off),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

pub fn run_bash(ws: &Workspace, command: &str, mode: &BashMode) -> Result<String, String> {
    if *mode == BashMode::Off {
        return Err("bash execution is disabled (--bash-mode off)".to_string());
    }

    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("empty command".to_string());
    }

    if *mode == BashMode::Safe {
        let lower = trimmed.to_lowercase();
        for blocked in BLOCKED_COMMANDS {
            if lower.contains(blocked) {
                return Err(format!("blocked command in safe mode: matches '{}'", blocked));
            }
        }

        let first_token = trimmed.split_whitespace().next().unwrap_or("");
        let is_allowed = ALLOWED_PREFIXES.iter().any(|prefix| {
            trimmed.starts_with(prefix) || trimmed.starts_with(prefix.trim_end())
        });

        if !is_allowed {
            return Err(format!(
                "command '{}' not in allowlist for safe mode. Use --bash-mode full to allow any command.",
                first_token
            ));
        }
    }

    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .arg("/C").arg(trimmed)
            .current_dir(ws.root())
            .output()
    } else {
        Command::new("sh")
            .arg("-c").arg(trimmed)
            .current_dir(ws.root())
            .output()
    };

    let output = output.map_err(|e| format!("failed to execute: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr]\n");
        result.push_str(&stderr);
    }
    if result.is_empty() {
        result = "(no output)".to_string();
    }

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        return Err(format!("exit code {}\n{}", code, result));
    }

    if result.len() > 16000 {
        result.truncate(16000);
        result.push_str("\n... (output truncated)");
    }

    Ok(result)
}
