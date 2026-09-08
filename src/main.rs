mod theme;
mod tools;
mod ui;

use chrono::{Local, Timelike};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::{Stream, StreamExt};
use ratatui::{prelude::*, widgets::ListState};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DIR: &str = ".nativestuff";
const FILE: &str = "Config.json";
const CONV_SUBDIR: &str = "conversations";

#[derive(Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub model: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<Msg>,
}
#[derive(Clone)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub model: String,
    pub updated_at: u64,
}
fn conversations_dir() -> PathBuf {
    home().join(DIR).join(CONV_SUBDIR)
}
fn save_conversation(conv: &Conversation) -> Result<(), String> {
    let dir = conversations_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", conv.id));
    fs::write(
        &path,
        serde_json::to_string_pretty(conv).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn list_conversations() -> Vec<ConversationMeta> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(conversations_dir()) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                    out.push(ConversationMeta {
                        id: conv.id,
                        title: conv.title,
                        model: conv.model,
                        updated_at: conv.updated_at,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}
fn load_conversation(id: &str) -> Result<Conversation, String> {
    let path = conversations_dir().join(format!("{id}.json"));
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}
fn delete_conversation(id: &str) -> Result<(), String> {
    fs::remove_file(conversations_dir().join(format!("{id}.json"))).map_err(|e| e.to_string())
}
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
pub fn format_ts(secs: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|dt| {
            dt.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: String,
    pub api_key: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub user_name: Option<String>,
}
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
}
pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "openai",
        name: "OpenAI",
    },
    Provider {
        id: "anthropic",
        name: "Anthropic",
    },
    Provider {
        id: "google",
        name: "Google",
    },
    Provider {
        id: "deepseek",
        name: "DeepSeek",
    },
    Provider {
        id: "qwen",
        name: "Alibaba/Qwen",
    },
    Provider {
        id: "openrouter",
        name: "OpenRouter",
    },
    Provider {
        id: "zhipu",
        name: "Zhipu AI",
    },
];
#[derive(Clone, Serialize, Deserialize)]
pub struct Msg {
    pub role: String,
    pub content: String,
}
// Hard cap on subagents spawned per user turn. Subagents no longer have the spawn_subagent
// tool themselves (see tools::system_prompt), but this cap is defense-in-depth against runaway
// spawning if a model is asked to spawn "N subagents" and tries to do it faster than one at a
// time, or via any other path that reaches the spawn_subagent branch repeatedly.
const MAX_SUBAGENTS_PER_TURN: u32 = 5;
pub enum EventMsg {
    Chunk(u64, String),
    Done(u64),
    Error(u64, String),
    Models(u64, Vec<String>),
    Title(String, String),
}
#[derive(PartialEq, Eq)]
pub enum Mode {
    Chat,
    Setup(u8),
    Models,
    Providers,
    Conversations,
}

pub struct App {
    pub config: Config,
    pub messages: Vec<Msg>,
    pub input: String,
    pub input_cursor: usize,
    pub input_anchor: Option<usize>,
    pub status: String,
    pub error: Option<String>,
    pub busy: bool,
    pub mode: Mode,
    pub provider: usize,
    pub models: Vec<String>,
    pub model_state: ListState,
    pub provider_state: ListState,
    pub api_input: String,
    pub name_input: String,
    pub greeting: String,
    pub scroll: u16,
    pub request_id: u64,
    pub cwd: PathBuf,
    pub tool_calls: Vec<tools::ToolCall>,
    pub tools_expanded: bool,
    pub tool_click_y: Cell<u16>,
    pub subagent_count: u32,
    pub awaiting_subagent: bool,
    pub conversation_id: Option<String>,
    pub conversation_title: Option<String>,
    pub conversation_created_at: Option<u64>,
    pub conversations: Vec<ConversationMeta>,
    pub conversation_state: ListState,
}
impl App {
    fn new(config: Config, setup: bool, error: Option<String>) -> Self {
        let provider = PROVIDERS
            .iter()
            .position(|p| p.id == config.provider)
            .unwrap_or(0);
        let name = config.user_name.clone();
        let mode = if error.is_some() {
            Mode::Setup(3)
        } else if setup {
            Mode::Setup(0)
        } else {
            Mode::Chat
        };
        let status = if error.is_some() {
            "Fix Config.json and restart".into()
        } else if setup {
            "Choose a provider".into()
        } else {
            "Ready".into()
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            config,
            messages: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            input_anchor: None,
            status,
            error,
            busy: false,
            mode,
            provider,
            models: Vec::new(),
            model_state: ListState::default(),
            provider_state: ListState::default(),
            api_input: String::new(),
            name_input: name.clone().unwrap_or_default(),
            greeting: greeting(name.as_deref()),
            scroll: 0,
            request_id: 0,
            cwd,
            tool_calls: Vec::new(),
            tools_expanded: false,
            tool_click_y: Cell::new(0),
            subagent_count: 0,
            awaiting_subagent: false,
            conversation_id: None,
            conversation_title: None,
            conversation_created_at: None,
            conversations: Vec::new(),
            conversation_state: ListState::default(),
        }
    }
    fn save(&self) -> Result<(), String> {
        let dir = home().join(DIR);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(FILE);
        fs::write(
            &path,
            serde_json::to_string_pretty(&self.config).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ".".into())
}
fn load() -> Result<Option<Config>, String> {
    let p = home().join(DIR).join(FILE);
    if !p.exists() {
        return Ok(None);
    }
    let c: Config = serde_json::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Wrong Config.json formatting: {e}"))?;
    if c.provider.trim().is_empty() || c.api_key.trim().is_empty() {
        return Err("Wrong Config.json formatting: provider and api_key are required".into());
    }
    if !PROVIDERS.iter().any(|p| p.id == c.provider) {
        return Err(format!(
            "Wrong Config.json formatting: unsupported provider {}",
            c.provider
        ));
    }
    Ok(Some(c))
}
fn greeting(name: Option<&str>) -> String {
    let n = name.map(|x| format!(", {x}")).unwrap_or_default();
    let h = Local::now().hour();
    let v = if h < 12 {
        [
            format!("Good morning{n}."),
            format!("What should we work on{n}?"),
        ]
    } else if h < 18 {
        [
            format!("Good afternoon{n}."),
            format!("What should we do{n}?"),
        ]
    } else {
        [
            format!("Good evening{n}."),
            format!("Good to see you back{n}."),
        ]
    };
    v[(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as usize)
        % 2]
    .clone()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (config, setup, error) = match load() {
        Ok(Some(c)) => (c, false, None),
        Ok(None) => (
            Config {
                provider: PROVIDERS[0].id.into(),
                api_key: String::new(),
                model: None,
                user_name: None,
            },
            true,
            None,
        ),
        Err(e) => (
            Config {
                provider: PROVIDERS[0].id.into(),
                api_key: String::new(),
                model: None,
                user_name: None,
            },
            false,
            Some(e),
        ),
    };
    let mut app = App::new(config, setup, error);
    if app.error.is_none() && !setup {
        if let Err(e) = load_models(&mut app).await {
            app.error = Some(e);
            app.status = "Model discovery failed".into();
        }
    }
    let (tx, rx) = mpsc::channel();
    run(&mut app, tx, rx)?;
    Ok(())
}
fn run(app: &mut App, tx: mpsc::Sender<EventMsg>, rx: mpsc::Receiver<EventMsg>) -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;
    let result = loop_app(&mut term, app, tx, rx);
    disable_raw_mode()?;
    execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    term.show_cursor()?;
    result
}
fn loop_app<B: Backend>(
    term: &mut Terminal<B>,
    app: &mut App,
    tx: mpsc::Sender<EventMsg>,
    rx: mpsc::Receiver<EventMsg>,
) -> io::Result<()> {
    loop {
        while let Ok(e) = rx.try_recv() {
            match e {
                EventMsg::Chunk(id, s) if id == app.request_id => {
                    if let Some(m) = app.messages.last_mut() {
                        m.content.push_str(&s);
                        app.scroll = 0;
                    }
                }
                EventMsg::Done(id) if id == app.request_id => {
                    app.busy = false;
                    app.status = "Ready".into();
                    if let Some(last) = app.messages.last().cloned() {
                        if last.role == "assistant" {
                            if let Some(tc) = tools::parse_tool_call(&last.content) {
                                app.tool_calls.push(tc.clone());
                                if tc.tool == "spawn_subagent" {
                                    let task =
                                        tc.args["task"].as_str().unwrap_or("subtask").to_string();
                                    app.busy = true;
                                    app.status = format!("Subagent running: {task}");
                                    // Push a fresh message for the subagent's output so its
                                    // streamed chunks don't get appended onto the message that
                                    // still contains the original tool-call JSON. Without this,
                                    // parse_tool_call() would keep re-matching that same JSON on
                                    // every Done event and spawn the subagent again forever.
                                    app.messages.push(Msg {
                                        role: "assistant".into(),
                                        content: String::new(),
                                    });
                                    tools::spawn_subagent_task(
                                        app.config.clone(),
                                        task,
                                        app.cwd.clone(),
                                        tx.clone(),
                                        app.request_id,
                                    );
                                } else {
                                    app.status = format!("Executing tool {}...", tc.tool);
                                    let result = match tools::execute_tool(&app.cwd, &tc) {
                                        Ok(o) => format!("[TOOL RESULT] ({})\n{o}", tc.tool),
                                        Err(e) => format!("[TOOL ERROR] ({})\n{e}", tc.tool),
                                    };
                                    app.messages.push(Msg {
                                        role: "system".into(),
                                        content: result,
                                    });
                                    send_followup(app, &tx);
                                }
                            } else {
                                // No tool call in the final reply -- this turn is truly done.
                                // Persist the conversation now (and kick off async title
                                // generation the first time this conversation is saved).
                                persist_conversation(app, &tx);
                            }
                        }
                    }
                }
                EventMsg::Error(id, e) if id == app.request_id => {
                    app.busy = false;
                    app.awaiting_subagent = false;
                    app.error = Some(e);
                    app.status = "Error".into();
                    if app
                        .messages
                        .last()
                        .is_some_and(|m| m.role == "assistant" && m.content.trim().is_empty())
                    {
                        app.messages.pop();
                    }
                }
                EventMsg::Models(id, v) if id == app.request_id => {
                    app.models = v;
                    app.model_state
                        .select((!app.models.is_empty()).then_some(0));
                    app.busy = false;
                    app.status = "Ready".into();
                    choose_model(app);
                }
                EventMsg::Title(conv_id, title) => {
                    if app.conversation_id.as_deref() == Some(conv_id.as_str()) {
                        app.conversation_title = Some(title);
                        persist_conversation(app, &tx);
                    }
                }
                _ => {}
            }
        }
        term.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(40))? {
            match event::read()? {
                Event::Key(k) => {
                    if key(app, k, &tx) {
                        break;
                    }
                }
                Event::Mouse(m) => {
                    if m.kind == MouseEventKind::Down(MouseButton::Left)
                        && m.row == app.tool_click_y.get()
                    {
                        app.tools_expanded = !app.tools_expanded;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}
fn tab_complete(input: &mut String) {
    if !input.starts_with('/') {
        return;
    }
    let c = [
        "/model",
        "/provider",
        "/config",
        "/conversations",
        "/clear",
        "/new",
        "/quit",
        "/exit",
        "/help",
    ];
    let cur = input.trim().to_lowercase();
    if cur == "/" {
        *input = c[0].into();
        return;
    }
    if let Some(i) = c.iter().position(|x| *x == cur) {
        *input = c[(i + 1) % c.len()].into();
        return;
    }
    if let Some(x) = c.iter().find(|x| x.starts_with(&cur)) {
        *input = (*x).into()
    }
}
fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}
fn byte_pos(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|x| x.0).unwrap_or(s.len())
}
fn clear_selection(a: &mut App) {
    a.input_anchor = None
}
fn selected_range(a: &App) -> Option<(usize, usize)> {
    a.input_anchor
        .map(|x| (x.min(a.input_cursor), x.max(a.input_cursor)))
        .filter(|x| x.0 != x.1)
}
fn delete_selection(a: &mut App) -> bool {
    if let Some((x, y)) = selected_range(a) {
        let mut c = chars(&a.input);
        c.drain(x..y);
        a.input = c.into_iter().collect();
        a.input_cursor = x;
        clear_selection(a);
        true
    } else {
        false
    }
}
fn move_cursor(a: &mut App, target: usize, select: bool) {
    if select {
        if a.input_anchor.is_none() {
            a.input_anchor = Some(a.input_cursor)
        }
    } else {
        clear_selection(a)
    }
    a.input_cursor = target.min(a.input.chars().count())
}
fn word_left(s: &str, n: usize) -> usize {
    let c = chars(s);
    let mut i = n;
    while i > 0 && c[i - 1].is_whitespace() {
        i -= 1
    }
    while i > 0 && !c[i - 1].is_whitespace() {
        i -= 1
    }
    i
}
fn word_right(s: &str, n: usize) -> usize {
    let c = chars(s);
    let mut i = n;
    while i < c.len() && c[i].is_whitespace() {
        i += 1
    }
    while i < c.len() && !c[i].is_whitespace() {
        i += 1
    }
    i
}
fn key(a: &mut App, k: KeyEvent, tx: &mpsc::Sender<EventMsg>) -> bool {
    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    match a.mode {
        Mode::Setup(s) => return setup_key(a, k, s, tx),
        Mode::Models => {
            model_key(a, k);
            return false;
        }
        Mode::Providers => {
            provider_key(a, k);
            return false;
        }
        Mode::Conversations => {
            conversations_key(a, k);
            return false;
        }
        Mode::Chat => {}
    }
    if matches!(k.code, KeyCode::Tab | KeyCode::BackTab) {
        if a.input.starts_with('/') {
            tab_complete(&mut a.input);
            a.input_cursor = a.input.chars().count();
            clear_selection(a)
        }
        return false;
    }
    if k.code == KeyCode::Enter {
        if a.input.starts_with('/') {
            return command(a, tx);
        }
        if !a.busy && !a.input.trim().is_empty() {
            send(a, tx)
        }
        return false;
    }
    let select = k.modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Esc if a.busy => {
            a.request_id = a.request_id.wrapping_add(1);
            a.busy = false;
            a.status = "Cancelled".into()
        }
        KeyCode::Esc => {
            a.input.clear();
            a.input_cursor = 0;
            clear_selection(a)
        }
        KeyCode::Backspace if ctrl => {
            if !delete_selection(a) {
                let p = word_left(&a.input, a.input_cursor);
                let old = a.input_cursor;
                move_cursor(a, p, false);
                a.input_anchor = Some(p);
                a.input_cursor = old;
                let _ = delete_selection(a);
            }
        }
        KeyCode::Delete if ctrl => {
            if !delete_selection(a) {
                let p = word_right(&a.input, a.input_cursor);
                a.input_anchor = Some(a.input_cursor);
                a.input_cursor = p;
                let _ = delete_selection(a);
            }
        }
        KeyCode::Backspace => {
            if !delete_selection(a) && a.input_cursor > 0 {
                let mut c = chars(&a.input);
                c.remove(a.input_cursor - 1);
                a.input = c.into_iter().collect();
                a.input_cursor -= 1
            }
        }
        KeyCode::Delete => {
            if !delete_selection(a) && a.input_cursor < a.input.chars().count() {
                let mut c = chars(&a.input);
                c.remove(a.input_cursor);
                a.input = c.into_iter().collect()
            }
        }
        KeyCode::Left => move_cursor(
            a,
            if ctrl {
                word_left(&a.input, a.input_cursor)
            } else {
                a.input_cursor.saturating_sub(1)
            },
            select,
        ),
        KeyCode::Right => move_cursor(
            a,
            if ctrl {
                word_right(&a.input, a.input_cursor)
            } else {
                (a.input_cursor + 1).min(a.input.chars().count())
            },
            select,
        ),
        KeyCode::Home => move_cursor(a, 0, select),
        KeyCode::End => move_cursor(a, a.input.chars().count(), select),
        KeyCode::Up if a.input.is_empty() => a.scroll = a.scroll.saturating_add(1),
        KeyCode::Down if a.input.is_empty() => a.scroll = a.scroll.saturating_sub(1),
        KeyCode::PageUp => a.scroll = a.scroll.saturating_add(10),
        KeyCode::PageDown => a.scroll = a.scroll.saturating_sub(10),
        KeyCode::Char('a') if ctrl => move_cursor(a, a.input.chars().count(), true),
        KeyCode::Char(c) if !ctrl => {
            delete_selection(a);
            let p = byte_pos(&a.input, a.input_cursor);
            a.input.insert(p, c);
            a.input_cursor += 1
        }
        _ => {}
    }
    false
}
fn setup_key(a: &mut App, k: KeyEvent, s: u8, tx: &mpsc::Sender<EventMsg>) -> bool {
    match s {
        0 => match k.code {
            KeyCode::Up => a.provider = a.provider.saturating_sub(1),
            KeyCode::Down => a.provider = (a.provider + 1).min(PROVIDERS.len() - 1),
            KeyCode::Enter => {
                a.mode = Mode::Setup(1);
                a.status = "Paste your API key".into()
            }
            KeyCode::Esc => return true,
            _ => {}
        },
        1 => match k.code {
            KeyCode::Enter if !a.api_input.trim().is_empty() => {
                a.mode = Mode::Setup(2);
                a.status = "What should I call you?".into()
            }
            KeyCode::Esc => a.mode = Mode::Setup(0),
            KeyCode::Backspace => {
                a.api_input.pop();
            }
            KeyCode::Char(c) => a.api_input.push(c),
            _ => {}
        },
        2 => match k.code {
            KeyCode::Enter => {
                a.config.provider = PROVIDERS[a.provider].id.into();
                a.config.api_key = a.api_input.trim().into();
                a.config.user_name =
                    (!a.name_input.trim().is_empty()).then(|| a.name_input.trim().into());
                a.config.model = None;
                if let Err(e) = a.save() {
                    a.error = Some(e);
                    a.status = "Config save failed".into();
                    return false;
                }
                a.error = None;
                a.greeting = greeting(a.config.user_name.as_deref());
                a.models.clear();
                a.mode = Mode::Chat;
                a.status = "Loading models...".into();
                refresh_async(a, tx.clone())
            }
            KeyCode::Esc => a.mode = Mode::Setup(1),
            KeyCode::Backspace => {
                a.name_input.pop();
            }
            KeyCode::Char(c) => a.name_input.push(c),
            _ => {}
        },
        3 => {
            return matches!(
                k.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q')
            );
        }
        _ => {}
    }
    false
}
fn command(a: &mut App, tx: &mpsc::Sender<EventMsg>) -> bool {
    let c = a.input.trim().to_lowercase();
    a.input.clear();
    a.input_cursor = 0;
    clear_selection(a);
    match c.as_str() {
        "/help" => {
            a.status =
                "/model /provider /config /conversations /new /clear /quit".into()
        }
        "/model" => {
            a.mode = Mode::Models;
            if a.models.is_empty() {
                refresh_async(a, tx.clone())
            }
        }
        "/provider" => {
            a.provider_state.select(Some(a.provider));
            a.mode = Mode::Providers
        }
        "/config" => {
            a.status = format!(
                "Provider: {} | Model: {} | API key: configured",
                PROVIDERS[a.provider].name,
                a.config.model.as_deref().unwrap_or("auto")
            )
        }
        "/new" | "/clear" => {
            a.messages.clear();
            a.tool_calls.clear();
            a.scroll = 0;
            a.conversation_id = None;
            a.conversation_title = None;
            a.conversation_created_at = None;
        }
        "/conversations" | "/history" => {
            a.conversations = list_conversations();
            a.conversation_state
                .select((!a.conversations.is_empty()).then_some(0));
            a.mode = Mode::Conversations;
        }
        "/quit" | "/exit" => return true,
        _ => a.status = format!("Unknown command: {c}"),
    }
    false
}
fn model_key(a: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => a.mode = Mode::Chat,
        KeyCode::Up => a.model_state.select(Some(
            a.model_state.selected().unwrap_or(0).saturating_sub(1),
        )),
        KeyCode::Down => {
            let i = a.model_state.selected().unwrap_or(0);
            if i + 1 < a.models.len() {
                a.model_state.select(Some(i + 1))
            }
        }
        KeyCode::PageUp => {
            let i = a.model_state.selected().unwrap_or(0);
            a.model_state.select(Some(i.saturating_sub(5)))
        }
        KeyCode::PageDown => {
            let i = a.model_state.selected().unwrap_or(0);
            if !a.models.is_empty() {
                a.model_state.select(Some((i + 5).min(a.models.len() - 1)))
            }
        }
        KeyCode::Enter => {
            if let Some(i) = a.model_state.selected() {
                if let Some(m) = a.models.get(i).cloned() {
                    a.config.model = Some(m);
                    if let Err(e) = a.save() {
                        a.error = Some(e)
                    } else {
                        a.mode = Mode::Chat
                    }
                }
            }
        }
        _ => {}
    }
}
fn provider_key(a: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => a.mode = Mode::Chat,
        KeyCode::Up => a.provider_state.select(Some(
            a.provider_state.selected().unwrap_or(0).saturating_sub(1),
        )),
        KeyCode::Down => {
            let i = a.provider_state.selected().unwrap_or(0);
            if i + 1 < PROVIDERS.len() {
                a.provider_state.select(Some(i + 1))
            }
        }
        KeyCode::Enter => {
            if let Some(i) = a.provider_state.selected() {
                a.provider = i;
                a.api_input.clear();
                a.mode = Mode::Setup(1);
                a.status = "Paste your API key".into()
            }
        }
        _ => {}
    }
}
fn conversations_key(a: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => a.mode = Mode::Chat,
        KeyCode::Up => a.conversation_state.select(Some(
            a.conversation_state
                .selected()
                .unwrap_or(0)
                .saturating_sub(1),
        )),
        KeyCode::Down => {
            let i = a.conversation_state.selected().unwrap_or(0);
            if i + 1 < a.conversations.len() {
                a.conversation_state.select(Some(i + 1))
            }
        }
        KeyCode::PageUp => {
            let i = a.conversation_state.selected().unwrap_or(0);
            a.conversation_state.select(Some(i.saturating_sub(5)))
        }
        KeyCode::PageDown => {
            let i = a.conversation_state.selected().unwrap_or(0);
            if !a.conversations.is_empty() {
                a.conversation_state
                    .select(Some((i + 5).min(a.conversations.len() - 1)))
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(i) = a.conversation_state.selected() {
                if let Some(meta) = a.conversations.get(i).cloned() {
                    if let Err(e) = delete_conversation(&meta.id) {
                        a.status = format!("Delete failed: {e}");
                    } else {
                        a.conversations = list_conversations();
                        let n = a.conversations.len();
                        a.conversation_state
                            .select((n > 0).then_some(i.min(n.saturating_sub(1))));
                        if a.conversation_id.as_deref() == Some(meta.id.as_str()) {
                            a.conversation_id = None;
                            a.conversation_title = None;
                            a.conversation_created_at = None;
                        }
                    }
                }
            }
        }
        KeyCode::Enter => {
            if let Some(i) = a.conversation_state.selected() {
                if let Some(meta) = a.conversations.get(i).cloned() {
                    match load_conversation(&meta.id) {
                        Ok(conv) => {
                            a.messages = conv.messages;
                            a.tool_calls.clear();
                            a.scroll = 0;
                            a.conversation_id = Some(conv.id);
                            a.conversation_title = Some(conv.title);
                            a.conversation_created_at = Some(conv.created_at);
                            if let Some(pi) = PROVIDERS
                                .iter()
                                .position(|p| p.name == conv.provider.as_str())
                            {
                                a.provider = pi;
                            }
                            a.config.model = Some(conv.model);
                            a.mode = Mode::Chat;
                        }
                        Err(e) => a.status = format!("Load failed: {e}"),
                    }
                }
            }
        }
        _ => {}
    }
}
fn send(a: &mut App, tx: &mpsc::Sender<EventMsg>) {
    let text = a.input.trim().to_string();
    a.input.clear();
    a.input_cursor = 0;
    clear_selection(a);
    a.error = None;
    a.request_id = a.request_id.wrapping_add(1);
    let id = a.request_id;
    a.subagent_count = 0;
    a.messages.push(Msg {
        role: "user".into(),
        content: text,
    });
    a.messages.push(Msg {
        role: "assistant".into(),
        content: String::new(),
    });
    a.busy = true;
    a.status = "Thinking...".into();
    a.scroll = 0;
    let cfg = a.config.clone();
    let cwd = a.cwd.clone();
    let hist = a.messages.clone();
    let tx = tx.clone();
    let _ = tokio::spawn(async move {
        if let Err(e) = request(&cfg, &cwd, &hist, tx.clone(), id).await {
            let _ = tx.send(EventMsg::Error(id, e));
        }
    });
}
fn send_followup(a: &mut App, tx: &mpsc::Sender<EventMsg>) {
    a.error = None;
    a.request_id = a.request_id.wrapping_add(1);
    let id = a.request_id;
    a.messages.push(Msg {
        role: "assistant".into(),
        content: String::new(),
    });
    a.busy = true;
    a.status = "Processing tool result...".into();
    a.scroll = 0;
    let cfg = a.config.clone();
    let cwd = a.cwd.clone();
    let hist = a.messages.clone();
    let tx = tx.clone();
    let _ = tokio::spawn(async move {
        if let Err(e) = request(&cfg, &cwd, &hist, tx.clone(), id, true).await {
            let _ = tx.send(EventMsg::Error(id, e));
        }
    });
}
pub async fn request_direct(c: &Config, hist: &[Msg], cwd: &Path) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    request(c, cwd, hist, tx, 99999).await?;
    let mut r = String::new();
    while let Ok(m) = rx.try_recv() {
        if let EventMsg::Chunk(_, s) = m {
            r.push_str(&s)
        }
    }
    Ok(r)
}
fn refresh_async(a: &mut App, tx: mpsc::Sender<EventMsg>) {
    a.request_id = a.request_id.wrapping_add(1);
    let id = a.request_id;
    let cfg = a.config.clone();
    a.busy = true;
    let _ = std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(x) => x,
            Err(e) => {
                let _ = tx.send(EventMsg::Error(id, e.to_string()));
                return;
            }
        };
        let _ = tx.send(match rt.block_on(list_models(&cfg)) {
            Ok(v) => EventMsg::Models(id, v),
            Err(e) => EventMsg::Error(id, e),
        });
    });
}
async fn load_models(a: &mut App) -> Result<(), String> {
    let v = list_models(&a.config).await?;
    a.models = v;
    a.model_state.select((!a.models.is_empty()).then_some(0));
    choose_model(a);
    Ok(())
}
fn choose_model(a: &mut App) {
    if a.config
        .model
        .as_ref()
        .is_some_and(|m| a.models.contains(m))
    {
        return;
    }
    if let Some(m) = a.models.first().cloned() {
        a.config.model = Some(m);
        let _ = a.save();
    }
}
async fn http() -> Result<Client, String> {
    Client::builder()
        .user_agent("term-code/0.1")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}
fn base(c: &Config) -> Option<&'static str> {
    match c.provider.as_str() {
        "openai" => Some("https://api.openai.com/v1"),
        "deepseek" => Some("https://api.deepseek.com"),
        "qwen" => Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "zhipu" => Some("https://open.bigmodel.cn/api/paas/v4"),
        _ => None,
    }
}
async fn list_models(c: &Config) -> Result<Vec<String>, String> {
    let h = http().await?;
    let models = if let Some(b) = base(c) {
        let r = h
            .get(format!("{b}/models"))
            .bearer_auth(&c.api_key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let s = r.status();
        let v: Value = r.json().await.map_err(|e| e.to_string())?;
        if !s.is_success() {
            return Err(api_error(&v, s));
        }
        v["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        match c.provider.as_str() {
            "anthropic" => {
                let r = h
                    .get("https://api.anthropic.com/v1/models")
                    .header("x-api-key", &c.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let s = r.status();
                let v: Value = r.json().await.map_err(|e| e.to_string())?;
                if !s.is_success() {
                    return Err(api_error(&v, s));
                }
                v["data"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x["id"].as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            "google" => {
                let mut models = Vec::new();
                let mut page_token = None;
                loop {
                    let mut req = h
                        .get("https://generativelanguage.googleapis.com/v1beta/models")
                        .header("x-goog-api-key", &c.api_key)
                        .query(&[("pageSize", "1000")]);
                    if let Some(t) = &page_token {
                        req = req.query(&[("pageToken", t)])
                    }
                    let r = req.send().await.map_err(|e| e.to_string())?;
                    let status = r.status();
                    let v: Value = r.json().await.map_err(|e| e.to_string())?;
                    if !status.is_success() {
                        return Err(api_error(&v, status));
                    }
                    if let Some(items) = v["models"].as_array() {
                        models.extend(
                            items
                                .iter()
                                .filter(|x| {
                                    x["supportedGenerationMethods"].as_array().is_some_and(|m| {
                                        m.iter().any(|v| v.as_str() == Some("generateContent"))
                                    })
                                })
                                .filter_map(|x| {
                                    x["name"]
                                        .as_str()
                                        .map(|n| n.trim_start_matches("models/").to_owned())
                                }),
                        )
                    }
                    page_token = v["nextPageToken"].as_str().map(str::to_owned);
                    if page_token.is_none() {
                        break;
                    }
                }
                models
            }
            _ => return Err("Unsupported provider".into()),
        }
    };
    if models.is_empty() {
        return Err("Provider returned no usable models".into());
    }
    Ok(models)
}
async fn request(
    c: &Config,
    cwd: &Path,
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
    allow_subagents: bool,
) -> Result<(), String> {
    match c.provider.as_str() {
        "anthropic" => anthropic(c, cwd, hist, tx, id, allow_subagents).await,
        "google" => google(c, cwd, hist, tx, id, allow_subagents).await,
        _ => openai(c, cwd, hist, tx, id, allow_subagents).await,
    }
}
async fn openai(
    c: &Config,
    cwd: &Path,
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
    allow_subagents: bool,
) -> Result<(), String> {
    let b = base(c).ok_or("Provider endpoint unavailable")?;
    let model = c.model.as_deref().ok_or("No model selected")?;
    let mut msgs = vec![json!({"role":"system","content":tools::system_prompt(cwd, allow_subagents)})];
    for m in hist {
        let role = if m.role == "system" { "user" } else { &m.role };
        msgs.push(json!({"role":role,"content":m.content}))
    }
    let h = http().await?;
    let r = h
        .post(format!("{b}/chat/completions"))
        .bearer_auth(&c.api_key)
        .json(&json!({"model":model,"messages":msgs,"stream":true}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let s = r.status();
    if !s.is_success() {
        return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s));
    }
    sse(r.bytes_stream(), tx, "/choices/0/delta/content", id).await
}
async fn anthropic(
    c: &Config,
    cwd: &Path,
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
) -> Result<(), String> {
    let model = c.model.as_deref().ok_or("No model selected")?;
    let sys = tools::system_prompt(cwd);
    // Anthropic's Messages API has no "system" role for individual turns, but tool-call
    // results are stored internally with role "system" (see execute_tool/send_followup).
    // They must still be sent back as a "user" turn (like the openai()/google() paths do),
    // not dropped -- otherwise the model never sees tool output on follow-up requests and
    // the agentic tool-use loop breaks for the Anthropic provider.
    let msgs = hist
        .iter()
        .map(|m| {
            let role = if m.role == "system" { "user" } else { m.role.as_str() };
            json!({"role":role,"content":m.content})
        })
        .collect::<Vec<_>>();
    let h = http().await?;
    let r = h
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &c.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({"model":model,"max_tokens":4096,"system":sys,"messages":msgs,"stream":true}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let s = r.status();
    if !s.is_success() {
        return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s));
    }
    sse(r.bytes_stream(), tx, "/delta/text", id).await
}
async fn google(
    c: &Config,
    cwd: &Path,
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
    allow_subagents: bool,
) -> Result<(), String> {
    let model = c.model.as_deref().ok_or("No model selected")?;
    let sys = tools::system_prompt(cwd, allow_subagents);
    let contents = hist.iter().map(|m| json!({"role":if m.role == "assistant" { "model" } else { "user" },"parts":[{"text":m.content}]})).collect::<Vec<_>>();
    let h = http().await?;
    let r = h
        .post(format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent"
        ))
        .query(&[("alt", "sse")])
        .header("x-goog-api-key", &c.api_key)
        .json(&json!({"contents":contents,"system_instruction":{"parts":[{"text":sys}]}}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let s = r.status();
    if !s.is_success() {
        return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s));
    }
    sse(r.bytes_stream(), tx, "", id).await
}
async fn sse<S>(
    mut stream: S,
    tx: mpsc::Sender<EventMsg>,
    path: &str,
    id: u64,
) -> Result<(), String>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    let mut buf = String::new();
    let mut received = false;
    while let Some(x) = stream.next().await {
        let chunk = String::from_utf8_lossy(&x.map_err(|e| e.to_string())?).replace("\r\n", "\n");
        buf.push_str(&chunk);
        while let Some(p) = buf.find("\n\n") {
            let block = buf[..p].to_owned();
            buf.drain(..p + 2);
            for line in block.lines() {
                if process_sse_line(line, &tx, path, id, &mut received)? {
                    return Ok(());
                }
            }
        }
    }
    if !buf.trim().is_empty() {
        for line in buf.lines() {
            if process_sse_line(line, &tx, path, id, &mut received)? {
                return Ok(());
            }
        }
    }
    if !received {
        return Err("Model stream ended without a response".into());
    }
    let _ = tx.send(EventMsg::Done(id));
    Ok(())
}
fn process_sse_line(
    line: &str,
    tx: &mpsc::Sender<EventMsg>,
    path: &str,
    id: u64,
    received: &mut bool,
) -> Result<bool, String> {
    let data = line.strip_prefix("data:").map(str::trim).unwrap_or("");
    if data.is_empty() {
        return Ok(false);
    }
    if data == "[DONE]" {
        let _ = tx.send(EventMsg::Done(id));
        return Ok(true);
    }
    if let Ok(v) = serde_json::from_str::<Value>(data) {
        if v.pointer("/error/message").is_some() {
            return Err(api_error(&v, StatusCode::BAD_GATEWAY));
        }
        if path.is_empty() {
            if let Some(parts) = v
                .pointer("/candidates/0/content/parts")
                .and_then(Value::as_array)
            {
                for part in parts {
                    if part.get("thought").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    if let Some(s) = part.get("text").and_then(Value::as_str) {
                        *received = true;
                        let _ = tx.send(EventMsg::Chunk(id, s.into()));
                    }
                }
            }
        } else if let Some(s) = v.pointer(path).and_then(Value::as_str) {
            *received = true;
            let _ = tx.send(EventMsg::Chunk(id, s.into()));
        }
    }
    Ok(false)
}
fn api_error(v: &Value, s: StatusCode) -> String {
    v.pointer("/error/message")
        .and_then(Value::as_str)
        .or_else(|| v.get("message").and_then(Value::as_str))
        .map(|x| format!("API error ({s}): {x}"))
        .unwrap_or_else(|| format!("API error: {s}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_tab_completion() {
        let mut input = "/m".to_string();
        tab_complete(&mut input);
        assert_eq!(input, "/model");
        let mut input = "/p".to_string();
        tab_complete(&mut input);
        assert_eq!(input, "/provider");
        let mut input = "/c".to_string();
        tab_complete(&mut input);
        assert_eq!(input, "/config");
        tab_complete(&mut input);
        assert_eq!(input, "/clear");
        let mut input = "/".to_string();
        tab_complete(&mut input);
        assert_eq!(input, "/model")
    }
}