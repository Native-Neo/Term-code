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

pub fn system_prompt(cwd: &Path, allow_subagents: bool) -> String {
    let subagent_tool = if allow_subagents {
        "\n6. `spawn_subagent` - Spin up a subagent to work on a subtask in parallel.\n   args: { \"task\": \"description of task for subagent\" }\n"
    } else {
        ""
    };
    let subagent_guideline = if allow_subagents {
        "\n- For heavy or multi-part tasks, use `spawn_subagent` to break down work."
    } else {
        "\n- You are a subagent handling one focused subtask; you do NOT have the spawn_subagent tool, so complete this task directly with the other tools instead of trying to delegate further."
    };
    format!(
        r#"You are Term-Code Agent, an advanced AI developer CLI operating inside directory: `{}`.
You have FULL ACCESS to this directory and all subfolders/files.

You can inspect code, read/write files, execute shell commands, and search the codebase{}.

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
{subagent_tool}
GUIDELINES:
- Perform workspace actions step-by-step using tools.
- When writing or editing code, use `write_file` or `run_cmd`.{subagent_guideline}
- Output clear explanations alongside tool calls."#,
        cwd.display(),
        if allow_subagents { ", and spawn subagents for multi-tasking" } else { "" },
    )
}

pub fn parse_tool_call(content: &str) -> Option<ToolCall> {
    if !content.contains("\"tool\"") {
        return None;
    }

    // Try every markdown json block in order.
    let mut rest = content;
    while let Some(start) = rest.find("```json") {
        let after = &rest[start + 7..];
        let (json_str, tail) = match after.find("```") {
            Some(end) => (&after[..end], &after[end + 3..]),
            None => break,
        };
        if let Ok(tc) = serde_json::from_str::<ToolCall>(json_str.trim()) {
            return Some(tc);
        }
        rest = tail;
    }

    // Try every generic code block, skipping the language tag.
    rest = content;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let (code_body, tail) = match after.find("```") {
            Some(end) => {
                let body = match after[..end].find('\n') {
                    Some(nl) => &after[nl + 1..end],
                    None => &after[..end],
                };
                (body, &after[end + 3..])
            }
            None => break,
        };
        if let Ok(tc) = serde_json::from_str::<ToolCall>(code_body.trim()) {
            return Some(tc);
        }
        rest = tail;
    }

    // Finally, scan each raw JSON object one at a time. This handles
    // multiple objects in one message without concatenating them, and
    // skips braces that appear inside JSON string values.
    let mut i = 0;
    while let Some(rel) = content[i..].find('{') {
        let start = i + rel;
        match closing_brace(content, start) {
            Some(end) => {
                if let Ok(tc) = serde_json::from_str::<ToolCall>(&content[start..=end]) {
                    return Some(tc);
                }
                i = end + 1;
            }
            None => break,
        }
    }

    None
}

fn closing_brace(content: &str, open: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
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
                if content.contains('\0') {
                    continue;
                }
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

pub async fn generate_title(config: &Config, hist: &[Msg], cwd: &Path) -> Result<String, String> {
    let snippet = hist
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .take(6)
        .map(|m| {
            let body: String = m.content.chars().take(300).collect();
            format!("{}: {}", m.role, body)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = vec![Msg {
        role: "user".to_string(),
        content: format!(
            "Generate a short, plain-text title (3-6 words, no quotes, no trailing \
             punctuation, no markdown) that summarizes the following conversation. \
             Respond with ONLY the title text and nothing else.\n\n{snippet}"
        ),
    }];
    let raw = crate::request_direct(config, &prompt, cwd).await?;
    // Defensive: if the model ignored the instructions and emitted a tool call instead of a
    // plain title, don't use it -- fall back to the caller's default title.
    if parse_tool_call(&raw).is_some() {
        return Err("model returned a tool call instead of a title".into());
    }
    let title = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .trim_matches(['"', '\'', '.'])
        .to_string();
    if title.is_empty() {
        Err("model returned an empty title".into())
    } else {
        Ok(title)
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
                // Sent as a Chunk+Done, not a top-level Error: a failed subagent shouldn't halt
                // the whole turn. This lets the top-level model see the failure via the normal
                // follow-up path and decide how to proceed (retry, try another approach, or
                // just report it), the same way a failed regular tool call already does.
                let _ = tx.send(EventMsg::Chunk(request_id, err_msg));
                let _ = tx.send(EventMsg::Done(request_id));
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
    fn test_parse_tool_call_multiple_blocks() {
        let content = r#"Found it. First inspect the tree:
```json
{ "tool": "list_dir", "args": { "path": "." } }
```
No wait, we need the manifest:
```json
{ "tool": "read_file", "args": { "path": "Cargo.toml" } }
```"#;
        let tc = parse_tool_call(content).expect("first tool call should be parsed");
        assert_eq!(tc.tool, "list_dir");
    }

    #[test]
    fn test_parse_tool_call_string_braces() {
        let content = "Write it: {\"tool\":\"write_file\",\"args\":{\"path\":\"a.txt\",\"content\":\"{hello} world\"}} and done.";
        let tc = parse_tool_call(content).expect("should parse with braces inside a string");
        assert_eq!(tc.tool, "write_file");
        assert_eq!(tc.args["content"], "{hello} world");
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
