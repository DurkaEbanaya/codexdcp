use crate::workspace::Workspace;
use std::path::PathBuf;

const HANDOFF_DIR: &str = ".ai-bridge";

const HANDOFF_FILES: &[(&str, &str)] = &[
    ("current-plan.md", "Current Plan"),
    ("agent-status.md", "Agent Status"),
    ("implementation-diff.patch", "Implementation Diff"),
    ("codex-status.md", "Codex Status"),
    ("decisions.md", "Decisions"),
    ("open-questions.md", "Open Questions"),
];

fn handoff_path(ws: &Workspace) -> PathBuf {
    ws.root().join(HANDOFF_DIR)
}

pub fn read_handoff(ws: &Workspace) -> Result<String, String> {
    let dir = handoff_path(ws);
    if !dir.exists() {
        return Ok("No .ai-bridge/ directory found in workspace.".to_string());
    }

    let mut out = String::new();
    for (filename, label) in HANDOFF_FILES {
        let path = dir.join(filename);
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read {}: {}", filename, e))?;
            out.push_str(&format!("=== {} ({}) ===\n{}\n\n", label, filename, content));
        }
    }

    let log_path = dir.join("execution-log.jsonl");
    if log_path.exists()
        && let Ok(content) = std::fs::read_to_string(&log_path)
    {
        let lines: Vec<&str> = content.lines().collect();
        let last = lines.len().min(10);
        out.push_str(&format!("=== Execution Log (last {} entries) ===\n", last));
        for line in &lines[lines.len().saturating_sub(last)..] {
            out.push_str(line);
            out.push('\n');
        }
    }

    if out.is_empty() {
        Ok(".ai-bridge/ exists but is empty.".to_string())
    } else {
        Ok(out)
    }
}

pub fn handoff_to_agent(ws: &Workspace, plan: &str, agent: Option<&str>, model: Option<&str>) -> Result<String, String> {
    let dir = handoff_path(ws);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create .ai-bridge/: {}", e))?;

    let plan_path = dir.join("current-plan.md");
    std::fs::write(&plan_path, plan)
        .map_err(|e| format!("failed to write plan: {}", e))?;

    let agent_name = agent.unwrap_or("codex");
    let model_name = model.unwrap_or("default");

    let status_content = format!(
        "# Agent Status\n\n\
         - Agent: {}\n\
         - Model: {}\n\
         - Plan written: {}\n\
         - Status: pending\n",
        agent_name, model_name, chrono_now()
    );

    let status_path = dir.join("agent-status.md");
    std::fs::write(&status_path, status_content)
        .map_err(|e| format!("failed to write status: {}", e))?;

    let log_entry = format!(
        r#"{{"timestamp":"{}","action":"plan_written","agent":"{}","model":"{}"}}"#,
        chrono_now(), agent_name, model_name
    );
    let log_path = dir.join("execution-log.jsonl");
    let mut log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
    log_content.push_str(&log_entry);
    log_content.push('\n');
    std::fs::write(&log_path, log_content)
        .map_err(|e| format!("failed to write log: {}", e))?;

    Ok(format!(
        "Plan written to .ai-bridge/current-plan.md\n\
         Agent: {} | Model: {}\n\
         Status: pending\n\
         \n\
         To execute, run the agent on the plan:\n\
         - Codex: codex --plan .ai-bridge/current-plan.md\n\
         - OpenCode: use the plan as a prompt\n\
         - Custom: configure with --agent custom --command \"...\"",
        agent_name, model_name
    ))
}

pub fn export_pro_context(ws: &Workspace) -> Result<String, String> {
    let dir = handoff_path(ws);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create .ai-bridge/: {}", e))?;

    let mut content = String::new();

    content.push_str("# Workspace Context\n\n");

    if let Ok(tree) = crate::fs_ops::tree(ws, None, Some(3)) {
        content.push_str("## File Tree\n```\n");
        content.push_str(&tree);
        content.push_str("```\n\n");
    }

    if let Ok(status) = crate::git_ops::git_status(ws)
        && !status.contains("clean")
    {
        content.push_str("## Git Status\n```\n");
        content.push_str(&status);
        content.push_str("```\n\n");
    }

    if let Ok(diff) = crate::git_ops::git_diff(ws, false)
        && !diff.trim().is_empty()
    {
        content.push_str("## Unstaged Diff\n```\n");
        content.push_str(&diff);
        content.push_str("```\n\n");
    }

    for name in &["AGENTS.md", "agents.md"] {
        let path = ws.root().join(name);
        if let Ok(c) = std::fs::read_to_string(&path) {
            content.push_str(&format!("## {}\n{}\n\n", name, c));
        }
    }

    let out_path = dir.join("pro-context.md");
    std::fs::write(&out_path, &content)
        .map_err(|e| format!("failed to write pro-context.md: {}", e))?;

    Ok(format!("Exported {} bytes to .ai-bridge/pro-context.md", content.len()))
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", now)
}
