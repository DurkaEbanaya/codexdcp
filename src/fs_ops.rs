use crate::workspace::Workspace;
use walkdir::WalkDir;

const MAX_READ_LINES: usize = 2000;
const MAX_TREE_DEPTH: usize = 10;
const MAX_SEARCH_RESULTS: usize = 200;

pub fn read_file(ws: &Workspace, path: &str, offset: Option<usize>, limit: Option<usize>) -> Result<String, String> {
    let resolved = ws.resolve(path)?;
    let content = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("failed to read {}: {}", path, e))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = offset.unwrap_or(0).min(lines.len());
    let end = limit
        .map(|l| (start + l).min(lines.len()))
        .unwrap_or(lines.len().min(start + MAX_READ_LINES));

    let mut out = String::new();
    for (i, line) in lines[start..end].iter().enumerate() {
        out.push_str(&format!("{}: {}\n", start + i + 1, line));
    }
    if end < lines.len() {
        out.push_str(&format!("\n... ({} more lines, use offset={})", lines.len() - end, end));
    }
    Ok(out)
}

pub fn write_file(ws: &Workspace, path: &str, content: &str) -> Result<String, String> {
    let resolved = ws.resolve_for_write(path)?;

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create dirs: {}", e))?;
    }

    let existed = resolved.exists();
    let old_content = if existed {
        std::fs::read_to_string(&resolved).unwrap_or_default()
    } else {
        String::new()
    };

    std::fs::write(&resolved, content)
        .map_err(|e| format!("failed to write {}: {}", path, e))?;

    let new_lines = content.lines().count();
    let old_lines = old_content.lines().count();

    if existed {
        let changed = old_content.lines()
            .zip(content.lines())
            .filter(|(a, b)| a != b)
            .count();
        Ok(format!("Modified: {} ({} lines, ~{} changed from {})", path, new_lines, changed, old_lines))
    } else {
        Ok(format!("Created: {} ({} lines)", path, new_lines))
    }
}

pub fn edit_file(ws: &Workspace, path: &str, old_string: &str, new_string: &str) -> Result<String, String> {
    let resolved = ws.resolve(path)?;
    let content = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("failed to read {}: {}", path, e))?;

    let count = content.matches(old_string).count();
    if count == 0 {
        return Err(format!("old_string not found in {}", path));
    }
    if count > 1 {
        return Err(format!("old_string found {} times in {}, need unique match", count, path));
    }

    let new_content = content.replacen(old_string, new_string, 1);
    std::fs::write(&resolved, &new_content)
        .map_err(|e| format!("failed to write {}: {}", path, e))?;

    let old_lines: Vec<&str> = old_string.lines().collect();
    let new_lines: Vec<&str> = new_string.lines().collect();
    Ok(format!("Edited: {} (replaced {} lines with {} lines)", path, old_lines.len(), new_lines.len()))
}

pub fn search_files(ws: &Workspace, pattern: &str, path: Option<&str>, include: Option<&str>) -> Result<String, String> {
    let search_root = if let Some(p) = path {
        ws.resolve(p)?
    } else {
        ws.root().to_path_buf()
    };

    let rg_available = std::process::Command::new("rg")
        .arg("--version")
        .output()
        .is_ok();

    if rg_available {
        let mut cmd = std::process::Command::new("rg");
        cmd.arg("--line-number").arg("--no-heading").arg("--color").arg("never");
        cmd.arg("--max-count").arg("50");
        if let Some(inc) = include {
            cmd.arg("--glob").arg(inc);
        }
        cmd.arg(pattern).arg(&search_root);
        let output = cmd.output().map_err(|e| format!("rg failed: {}", e))?;
        let result = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = result.lines().take(MAX_SEARCH_RESULTS).collect();
        if lines.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            Ok(lines.join("\n"))
        }
    } else {
        let mut cmd = std::process::Command::new("grep");
        cmd.arg("-rn").arg("--include=*");
        if let Some(inc) = include {
            cmd.arg(format!("--include={}", inc));
        }
        cmd.arg(pattern).arg(&search_root);
        let output = cmd.output().map_err(|e| format!("grep failed: {}", e))?;
        let result = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = result.lines().take(MAX_SEARCH_RESULTS).collect();
        if lines.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            Ok(lines.join("\n"))
        }
    }
}

pub fn tree(ws: &Workspace, path: Option<&str>, max_depth: Option<usize>) -> Result<String, String> {
    let root = if let Some(p) = path {
        ws.resolve(p)?
    } else {
        ws.root().to_path_buf()
    };
    let depth = max_depth.unwrap_or(MAX_TREE_DEPTH).min(20);

    let mut out = String::new();
    let display_root = root.strip_prefix(ws.root()).unwrap_or(&root);
    out.push_str(&format!("{}/\n", display_root.display()));

    let entries: Vec<walkdir::DirEntry> = WalkDir::new(&root)
        .max_depth(depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(name.starts_with('.') && name != ".github") &&
            name != "node_modules" &&
            name != "target" &&
            name != "__pycache__"
        })
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries.iter().skip(1) {
        let depth_rel = entry.depth();
        let prefix = "  ".repeat(depth_rel);
        let name = entry.file_name();
        if entry.file_type().is_dir() {
            out.push_str(&format!("{}{}/\n", prefix, name.display()));
        } else {
            out.push_str(&format!("{}{}\n", prefix, name.display()));
        }
    }

    if out.lines().count() > 500 {
        out = out.lines().take(500).collect::<Vec<_>>().join("\n");
        out.push_str("\n... (truncated)");
    }

    Ok(out)
}
