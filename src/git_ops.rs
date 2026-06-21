use crate::workspace::Workspace;
use std::process::Command;

fn git(ws: &Workspace, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(ws.root())
        .output()
        .map_err(|e| format!("git failed: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git error: {}", err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn git_status(ws: &Workspace) -> Result<String, String> {
    let s = git(ws, &["status", "--short"])?;
    if s.trim().is_empty() {
        Ok("Working tree clean.".to_string())
    } else {
        Ok(s)
    }
}

pub fn git_diff(ws: &Workspace, staged: bool) -> Result<String, String> {
    if staged {
        git(ws, &["diff", "--cached", "--stat"])
    } else {
        git(ws, &["diff", "--stat"])
    }
}

pub fn show_changes(ws: &Workspace) -> Result<String, String> {
    let status = git(ws, &["status", "--short"])?;
    let diff_stat = git(ws, &["diff", "--stat"])?;
    let staged_stat = git(ws, &["diff", "--cached", "--stat"])?;

    let mut out = String::new();

    if status.trim().is_empty() {
        out.push_str("Working tree clean. No changes.\n");
        return Ok(out);
    }

    let staged: Vec<&str> = status.lines().filter(|l| !l.is_empty() && !l.starts_with(" ??") && !l.starts_with("??")).collect();
    let unstaged: Vec<&str> = status.lines().filter(|l| !l.is_empty() && (l.starts_with(" M") || l.starts_with(" M") || l.starts_with("??"))).collect();
    let untracked: Vec<&str> = status.lines().filter(|l| l.starts_with("??")).collect();

    out.push_str(&format!("Staged ({}):\n", staged.len()));
    for line in &staged {
        out.push_str(&format!("  {}\n", line));
    }
    if !staged_stat.trim().is_empty() {
        out.push_str(&format!("\nStaged diff:\n{}\n", staged_stat));
    }

    out.push_str(&format!("\nUnstaged ({}):\n", unstaged.len()));
    for line in &unstaged {
        out.push_str(&format!("  {}\n", line));
    }
    if !diff_stat.trim().is_empty() {
        out.push_str(&format!("\nUnstaged diff:\n{}\n", diff_stat));
    }

    out.push_str(&format!("\nUntracked ({}):\n", untracked.len()));
    for line in &untracked {
        out.push_str(&format!("  {}\n", line));
    }

    Ok(out)
}
