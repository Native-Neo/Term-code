use crate::{Config, EventMsg, Msg};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

pub fn system_prompt(cwd: &Path) -> String {
    format!(
        r#"You are Term-Code Agent, an advanced AI developer CLI operating inside directory: `{}`.
You have FULL ACCESS to this directory and all subfolders/files.

You can inspect code, read/write files, execute shell commands, search the codebase, and spawn subagents for multi-tasking.

When you need to interact with the workspace or execute commands, output a JSON block with your tool call.
Use EXACTLY this JSON format in a code block:

```json
{{
  "tool": "<tool_name>",
  "args": {{ ... }}
}}
```

AVAILABLE TOOLS:

1. `list_dir` - List contents of a directory.
   args: {{ "path": "." }}

2. `read_file` - Read file contents.
   args: {{ "path": "src/main.rs" }}

3. `write_file` - Create or overwrite a file.
   args: {{ "path": "path/to/file", "content": "file content string" }}

4. `run_cmd` - Execute a shell command inside the workspace.
   args: {{ "cmd": "cargo test" }}

5. `search` - Search for text patterns across files in the workspace.
   args: {{ "query": "struct App" }}

6. `spawn_subagent` - Spin up a subagent to work on a subtask in parallel.
   args: {{ "task": "description of task for subagent" }}

GUIDELINES:
- Perform workspace actions step-by-step using tools.
- When writing or editing code, use `write_file` or `run_cmd`.
- For heavy or multi-part tasks, use `spawn_subagent` to break down work.
- Output clear explanations alongside tool calls."#,
        cwd.display()
    )
}

pub fn parse_tool_call(content: &str) -> Option<ToolCall> {
    if !content.contains("```json") && !content.contains("```") && !content.contains("\"tool\"") {
        return None;
    }

    // Try parsing from markdown json block
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(tc) = serde_json::from_str::<ToolCall>(json_str) {
                return Some(tc);
            }
        }
    }

    // Try parsing generic code block
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        // skip language tag if any
        let code_body = if let Some(newline) = after.find('\n') {
            &after[newline + 1..]
        } else {
            after
        };
        if let Some(end) = code_body.find("```") {
            let json_str = code_body[..end].trim();
            if let Ok(tc) = serde_json::from_str::<ToolCall>(json_str) {
                return Some(tc);
            }
        }
    }

    // Try raw json object containing "tool"
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if end > start {
                let json_str = &content[start..=end];
                if let Ok(tc) = serde_json::from_str::<ToolCall>(json_str) {
                    return Some(tc);
                }
            }
        }
    }

    None
}

pub fn execute_tool(cwd: &Path, tc: &ToolCall) -> Result<String, String> {
    match tc.tool.as_str() {
        "list_dir" => {
            let rel_path = tc.args["path"].as_str().unwrap_or(".");
            let target = resolve_path(cwd, rel_path);
            let entries =
                fs::read_dir(&target).map_err(|e| format!("Failed to read dir {rel_path}: {e}"))?;

            let mut files = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if is_dir {
                    files.push(format!("[DIR]  {name}/"));
                } else {
                    files.push(format!("[FILE] {name} ({size} bytes)"));
                }
            }
            files.sort();
            Ok(format!("Contents of `{rel_path}`:\n{}", files.join("\n")))
        }
        "read_file" => {
            let rel_path = tc.args["path"].as_str().ok_or("Missing 'path' argument")?;
            let target = resolve_path(cwd, rel_path);
            let content = fs::read_to_string(&target)
                .map_err(|e| format!("Failed to read file {rel_path}: {e}"))?;

            let lines: Vec<String> = content
                .lines()
                .enumerate()
                .map(|(idx, l)| format!("{:4} | {l}", idx + 1))
                .collect();

            Ok(format!(
                "File `{rel_path}` ({} lines):\n{}",
                lines.len(),
                lines.join("\n")
            ))
        }
        "write_file" => {
            let rel_path = tc.args["path"].as_str().ok_or("Missing 'path' argument")?;
            let content = tc.args["content"]
                .as_str()
                .ok_or("Missing 'content' argument")?;
            let target = resolve_path(cwd, rel_path);

            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }

            fs::write(&target, content)
                .map_err(|e| format!("Failed to write file {rel_path}: {e}"))?;
            Ok(format!(
                "Successfully wrote {} bytes to `{rel_path}`",
                content.len()
            ))
        }
        "run_cmd" => {
            let cmd_str = tc.args["cmd"].as_str().ok_or("Missing 'cmd' argument")?;
            let output = if cfg!(target_os = "windows") {
                Command::new("cmd")
                    .args(["/C", cmd_str])
                    .current_dir(cwd)
                    .output()
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(cmd_str)
                    .current_dir(cwd)
                    .output()
            }
            .map_err(|e| format!("Failed to run command '{cmd_str}': {e}"))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let status = output.status.code().unwrap_or(-1);

            Ok(format!(
                "Command: `{cmd_str}` (Exit status: {status})\n--- STDOUT ---\n{stdout}\n--- STDERR ---\n{stderr}"
            ))
        }
        "search" => {
            let query = tc.args["query"]
                .as_str()
                .ok_or("Missing 'query' argument")?;
            let mut matches = Vec::new();

            search_dir_recursive(cwd, cwd, query, &mut matches, 0);

            if matches.is_empty() {
                Ok(format!("No matches found for query: '{query}'"))
            } else {
                if matches.len() > 50 {
                    matches.truncate(50);
                    matches.push("... (results truncated to 50 matches)".to_string());
                }
                Ok(format!(
                    "Search results for '{query}':\n{}",
                    matches.join("\n")
                ))
            }
        }
        "spawn_subagent" => {
            let task = tc.args["task"].as_str().ok_or("Missing 'task' argument")?;
            Ok(format!("SUBAGENT_SPAWN: {task}"))
        }
        unknown => Err(format!("Unknown tool: '{unknown}'")),
    }
}

fn resolve_path(cwd: &Path, rel: &str) -> PathBuf {
    let path = PathBuf::from(rel);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn search_dir_recursive(
    root: &Path,
    current: &Path,
    query: &str,
    matches: &mut Vec<String>,
    depth: usize,
) {
    if depth > 8 || matches.len() >= 50 {
        return;
    }
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }

        if path.is_dir() {
            search_dir_recursive(root, &path, query, matches, depth + 1);
        } else if path.is_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(query) {
                        matches.push(format!("{rel}:{} | {}", line_num + 1, line.trim()));
                        if matches.len() >= 50 {
                            return;
                        }
                    }
                }
            }
        }
    }
}

pub fn spawn_subagent_task(
    config: Config,
    task_prompt: String,
    cwd: PathBuf,
    tx: mpsc::Sender<EventMsg>,
    request_id: u64,
) {
    tokio::spawn(async move {
        let _ = tx.send(EventMsg::Chunk(
            request_id,
            format!("\n[SUBAGENT] Started subagent task: \"{task_prompt}\"\n"),
        ));

        let sub_messages = vec![Msg {
            role: "user".to_string(),
            content: format!(
                "Execute this subtask in `{}`: {}",
                cwd.display(),
                task_prompt
            ),
        }];

        // Execute subagent prompt via HTTP request
        match crate::request_direct(&config, &sub_messages, &cwd).await {
            Ok(result) => {
                let summary =
                    format!("\n[SUBAGENT RESULT] Completed task \"{task_prompt}\":\n{result}\n");
                let _ = tx.send(EventMsg::Chunk(request_id, summary));
                let _ = tx.send(EventMsg::Done(request_id));
            }
            Err(e) => {
                let err_msg = format!("\n[SUBAGENT ERROR] Task \"{task_prompt}\" failed: {e}\n");
                let _ = tx.send(EventMsg::Error(request_id, err_msg));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_call() {
        let content = r#"I will list the directory contents.
```json
{
  "tool": "list_dir",
  "args": { "path": "." }
}
```"#;
        let tc = parse_tool_call(content).expect("Tool call should be parsed");
        assert_eq!(tc.tool, "list_dir");
        assert_eq!(tc.args["path"], ".");
    }

    #[test]
    fn test_execute_list_dir() {
        let cwd = std::env::current_dir().unwrap();
        let tc = ToolCall {
            tool: "list_dir".to_string(),
            args: serde_json::json!({ "path": "." }),
        };
        let res = execute_tool(&cwd, &tc).expect("list_dir should execute");
        assert!(res.contains("Cargo.toml"));
    }

    #[test]
    fn test_execute_read_file() {
        let cwd = std::env::current_dir().unwrap();
        let tc = ToolCall {
            tool: "read_file".to_string(),
            args: serde_json::json!({ "path": "Cargo.toml" }),
        };
        let res = execute_tool(&cwd, &tc).expect("read_file should execute");
        assert!(res.contains("term-code"));
    }
}
