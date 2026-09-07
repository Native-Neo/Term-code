mod theme;
mod tools;
mod ui;

use chrono::{Local, Timelike};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::{Stream, StreamExt};
use ratatui::{prelude::*, widgets::ListState};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io, path::{Path, PathBuf}, sync::mpsc, time::{Duration, SystemTime, UNIX_EPOCH}};

const DIR: &str = ".nativestuff";
const FILE: &str = "Config.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: String,
    pub api_key: String,
    #[serde(default)] pub model: Option<String>,
    #[serde(default)] pub user_name: Option<String>,
}

pub struct Provider { pub id: &'static str, pub name: &'static str }

pub const PROVIDERS: &[Provider] = &[
    Provider { id: "openai", name: "OpenAI" },
    Provider { id: "anthropic", name: "Anthropic" },
    Provider { id: "google", name: "Google" },
    Provider { id: "deepseek", name: "DeepSeek" },
    Provider { id: "qwen", name: "Alibaba/Qwen" },
    Provider { id: "openrouter", name: "OpenRouter" },
    Provider { id: "zhipu", name: "Zhipu AI" },
];

#[derive(Clone)]
pub struct Msg { pub role: String, pub content: String }

pub enum EventMsg {
    Chunk(u64, String), Done(u64), Error(u64, String), Models(u64, Vec<String>),
}

#[derive(PartialEq, Eq)]
pub enum Mode { Chat, Setup(u8), Models, Providers }

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
}

impl App {
    fn new(config: Config, setup: bool, error: Option<String>) -> Self {
        let provider = PROVIDERS.iter().position(|p| p.id == config.provider).unwrap_or(0);
        let name = config.user_name.clone();
        let mode = if error.is_some() { Mode::Setup(3) } else if setup { Mode::Setup(0) } else { Mode::Chat };
        let status = if error.is_some() { "Fix Config.json and restart".into() } else if setup { "Choose a provider".into() } else { "Ready".into() };
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            config, messages: Vec::new(), input: String::new(), input_cursor: 0, input_anchor: None,
            status, error, busy: false, mode, provider, models: Vec::new(), model_state: ListState::default(),
            provider_state: ListState::default(), api_input: String::new(), name_input: name.clone().unwrap_or_default(),
            greeting: greeting(name.as_deref()), scroll: 0, request_id: 0, cwd, tool_calls: Vec::new(), tools_expanded: false,
        }
    }

    fn save(&self) -> Result<(), String> {
        let dir = home().join(DIR);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(FILE);
        fs::write(&path, serde_json::to_string_pretty(&self.config).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn home() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| ".".into()) }

fn load() -> Result<Option<Config>, String> {
    let p = home().join(DIR).join(FILE);
    if !p.exists() { return Ok(None); }
    let c: Config = serde_json::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?).map_err(|e| format!("Wrong Config.json formatting: {e}"))?;
    if c.provider.trim().is_empty() || c.api_key.trim().is_empty() { return Err("Wrong Config.json formatting: provider and api_key are required".into()); }
    if !PROVIDERS.iter().any(|p| p.id == c.provider) { return Err(format!("Wrong Config.json formatting: unsupported provider {}", c.provider)); }
    Ok(Some(c))
}

fn greeting(name: Option<&str>) -> String {
    let n = name.map(|x| format!(", {x}")).unwrap_or_default();
    let h = Local::now().hour();
    let v = if h < 12 { [format!("Good morning{n}."), format!("What should we work on{n}?")] }
    else if h < 18 { [format!("Good afternoon{n}."), format!("What should we do{n}?")] }
    else { [format!("Good evening{n}."), format!("Good to see you back{n}.")] };
    v[(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as usize) % 2].clone()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (config, setup, error) = match load() {
        Ok(Some(c)) => (c, false, None),
        Ok(None) => (Config { provider: PROVIDERS[0].id.into(), api_key: String::new(), model: None, user_name: None }, true, None),
        Err(e) => (Config { provider: PROVIDERS[0].id.into(), api_key: String::new(), model: None, user_name: None }, false, Some(e)),
    };
    let mut app = App::new(config, setup, error);
    if app.error.is_none() && !setup {
        if let Err(e) = load_models(&mut app).await { app.error = Some(e); app.status = "Model discovery failed".into(); }
    }
    let (tx, rx) = mpsc::channel();
    run(&mut app, tx, rx)?;
    Ok(())
}

fn run(app: &mut App, tx: mpsc::Sender<EventMsg>, rx: mpsc::Receiver<EventMsg>) -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;
    let result = loop_app(&mut term, app, tx, rx);
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    term.show_cursor()?;
    result
}

fn loop_app<B: Backend>(term: &mut Terminal<B>, app: &mut App, tx: mpsc::Sender<EventMsg>, rx: mpsc::Receiver<EventMsg>) -> io::Result<()> {
    loop {
        while let Ok(e) = rx.try_recv() {
            match e {
                EventMsg::Chunk(id, s) if id == app.request_id => {
                    if let Some(m) = app.messages.last_mut() { m.content.push_str(&s); }
                }
                EventMsg::Done(id) if id == app.request_id => {
                    app.busy = false;
                    app.status = "Ready".into();
                    if let Some(last_msg) = app.messages.last().cloned() {
                        if last_msg.role == "assistant" {
                            if let Some(tc) = tools::parse_tool_call(&last_msg.content) {
                                app.tool_calls.push(tc.clone());
                                if tc.tool == "spawn_subagent" {
                                    let task_prompt = tc.args["task"].as_str().unwrap_or("subtask").to_string();
                                    app.busy = true;
                                    app.status = format!("Subagent running: {task_prompt}");
                                    tools::spawn_subagent_task(app.config.clone(), task_prompt, app.cwd.clone(), tx.clone(), app.request_id);
                                } else {
                                    app.status = format!("Executing tool {}...", tc.tool);
                                    let res = tools::execute_tool(&app.cwd, &tc);
                                    let result_str = match res {
                                        Ok(output) => format!("[TOOL RESULT] ({})\n{output}", tc.tool),
                                        Err(err) => format!("[TOOL ERROR] ({})\n{err}", tc.tool),
                                    };
                                    app.messages.push(Msg { role: "system".into(), content: result_str });
                                    send_followup(app, &tx);
                                }
                            }
                        }
                    }
                }
                EventMsg::Error(id, e) if id == app.request_id => {
                    app.busy = false;
                    app.error = Some(e);
                    app.status = "Error".into();
                    if app.messages.last().is_some_and(|m| m.role == "assistant" && m.content.trim().is_empty()) { app.messages.pop(); }
                }
                EventMsg::Models(id, v) if id == app.request_id => {
                    app.models = v;
                    app.model_state.select((!app.models.is_empty()).then_some(0));
                    app.busy = false;
                    app.status = "Ready".into();
                    choose_model(app);
                }
                _ => {}
            }
        }
        term.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(40))? {
            if let Event::Key(k) = event::read()? && key(app, k, &tx) { break; }
        }
    }
    Ok(())
}

fn tab_complete(input: &mut String) {
    if !input.starts_with('/') { return; }
    let commands = ["/model", "/provider", "/config", "/clear", "/new", "/quit", "/exit", "/help"];
    let current = input.trim().to_lowercase();
    if current == "/" { *input = commands[0].to_string(); return; }
    if let Some(pos) = commands.iter().position(|c| *c == current) { *input = commands[(pos + 1) % commands.len()].to_string(); return; }
    if let Some(first) = commands.iter().filter(|c| c.starts_with(&current)).next() { *input = first.to_string(); }
}

fn chars(input: &str) -> Vec<char> { input.chars().collect() }
fn byte_pos(input: &str, cursor: usize) -> usize { input.char_indices().nth(cursor).map(|(i, _)| i).unwrap_or(input.len()) }
fn clear_selection(app: &mut App) { app.input_anchor = None; }
fn selected_range(app: &App) -> Option<(usize, usize)> { app.input_anchor.map(|a| (a.min(app.input_cursor), a.max(app.input_cursor))).filter(|(a, b)| a != b) }
fn delete_selection(app: &mut App) -> bool {
    if let Some((a, b)) = selected_range(app) {
        let mut c = chars(&app.input);
        c.drain(a..b);
        app.input = c.into_iter().collect();
        app.input_cursor = a;
        clear_selection(app);
        true
    } else { false }
}
fn move_cursor(app: &mut App, target: usize, select: bool) {
    if select { if app.input_anchor.is_none() { app.input_anchor = Some(app.input_cursor); } }
    else { clear_selection(app); }
    app.input_cursor = target.min(app.input.chars().count());
}
fn word_left(input: &str, cursor: usize) -> usize {
    let c = chars(input); let mut i = cursor;
    while i > 0 && c[i - 1].is_whitespace() { i -= 1; }
    while i > 0 && !c[i - 1].is_whitespace() { i -= 1; }
    i
}
fn word_right(input: &str, cursor: usize) -> usize {
    let c = chars(input); let mut i = cursor;
    while i < c.len() && c[i].is_whitespace() { i += 1; }
    while i < c.len() && !c[i].is_whitespace() { i += 1; }
    i
}

fn key(app: &mut App, k: KeyEvent, tx: &mpsc::Sender<EventMsg>) -> bool {
    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) { return true; }
    match app.mode {
        Mode::Setup(step) => return setup_key(app, k, step, tx),
        Mode::Models => { model_key(app, k); return false; }
        Mode::Providers => { provider_key(app, k); return false; }
        Mode::Chat => {}
    }
    if k.code == KeyCode::Tab || k.code == KeyCode::BackTab {
        if app.input.starts_with('/') { tab_complete(&mut app.input); app.input_cursor = app.input.chars().count(); clear_selection(app); }
        return false;
    }
    if k.code == KeyCode::Enter {
        if app.input.starts_with('/') { return command(app, tx); }
        if !app.busy && !app.input.trim().is_empty() { send(app, tx); }
        return false;
    }
    let select = k.modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Esc if app.busy => { app.request_id = app.request_id.wrapping_add(1); app.busy = false; app.status = "Cancelled".into(); }
        KeyCode::Esc => { app.input.clear(); app.input_cursor = 0; clear_selection(app); }
        KeyCode::Backspace => {
            if !delete_selection(app) && app.input_cursor > 0 {
                let mut c = chars(&app.input); c.remove(app.input_cursor - 1); app.input = c.into_iter().collect(); app.input_cursor -= 1;
            }
        }
        KeyCode::Delete => {
            if !delete_selection(app) && app.input_cursor < app.input.chars().count() {
                let mut c = chars(&app.input); c.remove(app.input_cursor); app.input = c.into_iter().collect();
            }
        }
        KeyCode::Left => {
            let target = if ctrl { word_left(&app.input, app.input_cursor) } else { app.input_cursor.saturating_sub(1) };
            move_cursor(app, target, select);
        }
        KeyCode::Right => {
            let target = if ctrl { word_right(&app.input, app.input_cursor) } else { (app.input_cursor + 1).min(app.input.chars().count()) };
            move_cursor(app, target, select);
        }
        KeyCode::Home => move_cursor(app, 0, select),
        KeyCode::End => move_cursor(app, app.input.chars().count(), select),
        KeyCode::Up if app.input.is_empty() => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Down if app.input.is_empty() => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::PageUp => app.scroll = app.scroll.saturating_add(10),
        KeyCode::PageDown => app.scroll = app.scroll.saturating_sub(10),
        KeyCode::Char(c) if !ctrl => {
            delete_selection(app);
            let pos = byte_pos(&app.input, app.input_cursor);
            app.input.insert(pos, c);
            app.input_cursor += 1;
        }
        _ => {}
    }
    false
}

fn setup_key(app: &mut App, k: KeyEvent, step: u8, tx: &mpsc::Sender<EventMsg>) -> bool {
    match step {
        0 => match k.code {
            KeyCode::Up => app.provider = app.provider.saturating_sub(1),
            KeyCode::Down => app.provider = (app.provider + 1).min(PROVIDERS.len() - 1),
            KeyCode::Enter => { app.mode = Mode::Setup(1); app.status = "Paste your API key".into(); }
            KeyCode::Esc => return true,
            _ => {}
        },
        1 => match k.code {
            KeyCode::Enter if !app.api_input.trim().is_empty() => { app.mode = Mode::Setup(2); app.status = "What should I call you?".into(); }
            KeyCode::Esc => app.mode = Mode::Setup(0),
            KeyCode::Backspace => { app.api_input.pop(); }
            KeyCode::Char(c) => app.api_input.push(c),
            _ => {}
        },
        2 => match k.code {
            KeyCode::Enter => {
                app.config.provider = PROVIDERS[app.provider].id.into();
                app.config.api_key = app.api_input.trim().into();
                app.config.user_name = (!app.name_input.trim().is_empty()).then(|| app.name_input.trim().into());
                app.config.model = None;
                if let Err(e) = app.save() { app.error = Some(e); app.status = "Config save failed".into(); return false; }
                app.error = None;
                app.greeting = greeting(app.config.user_name.as_deref());
                app.models.clear(); app.mode = Mode::Chat; app.status = "Loading models...".into(); refresh_async(app, tx.clone());
            }
            KeyCode::Esc => app.mode = Mode::Setup(1),
            KeyCode::Backspace => { app.name_input.pop(); }
            KeyCode::Char(c) => app.name_input.push(c),
            _ => {}
        },
        3 => return matches!(k.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q')),
        _ => {}
    }
    false
}

fn command(app: &mut App, tx: &mpsc::Sender<EventMsg>) -> bool {
    let c = app.input.trim().to_lowercase();
    app.input.clear(); app.input_cursor = 0; clear_selection(app);
    match c.as_str() {
        "/help" => app.status = "/model /provider /config /new /clear /quit".into(),
        "/model" => { app.mode = Mode::Models; if app.models.is_empty() { refresh_async(app, tx.clone()); } }
        "/provider" => { app.provider_state.select(Some(app.provider)); app.mode = Mode::Providers; }
        "/config" => app.status = format!("Provider: {} | Model: {} | API key: configured", PROVIDERS[app.provider].name, app.config.model.as_deref().unwrap_or("auto")),
        "/new" | "/clear" => { app.messages.clear(); app.tool_calls.clear(); app.scroll = 0; }
        "/quit" | "/exit" => return true,
        _ => app.status = format!("Unknown command: {c}"),
    }
    false
}

fn model_key(app: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => app.mode = Mode::Chat,
        KeyCode::Up => app.model_state.select(Some(app.model_state.selected().unwrap_or(0).saturating_sub(1))),
        KeyCode::Down => { let i = app.model_state.selected().unwrap_or(0); if i + 1 < app.models.len() { app.model_state.select(Some(i + 1)); } }
        KeyCode::PageUp => { let i = app.model_state.selected().unwrap_or(0); app.model_state.select(Some(i.saturating_sub(5))); }
        KeyCode::PageDown => { let i = app.model_state.selected().unwrap_or(0); if !app.models.is_empty() { app.model_state.select(Some((i + 5).min(app.models.len() - 1))); } }
        KeyCode::Enter => if let Some(i) = app.model_state.selected() { if let Some(m) = app.models.get(i).cloned() { app.config.model = Some(m); if let Err(e) = app.save() { app.error = Some(e); } else { app.mode = Mode::Chat; } } },
        _ => {}
    }
}

fn provider_key(app: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => app.mode = Mode::Chat,
        KeyCode::Up => app.provider_state.select(Some(app.provider_state.selected().unwrap_or(0).saturating_sub(1))),
        KeyCode::Down => { let i = app.provider_state.selected().unwrap_or(0); if i + 1 < PROVIDERS.len() { app.provider_state.select(Some(i + 1)); } }
        KeyCode::Enter => if let Some(i) = app.provider_state.selected() { app.provider = i; app.api_input.clear(); app.mode = Mode::Setup(1); app.status = "Paste your API key".into(); },
        _ => {}
    }
}

fn send(app: &mut App, tx: &mpsc::Sender<EventMsg>) {
    let text = app.input.trim().to_string();
    app.input.clear(); app.input_cursor = 0; clear_selection(app); app.error = None;
    app.request_id = app.request_id.wrapping_add(1);
    let id = app.request_id;
    app.messages.push(Msg { role: "user".into(), content: text });
    app.messages.push(Msg { role: "assistant".into(), content: String::new() });
    app.busy = true; app.status = "Thinking...".into(); app.scroll = 0;
    let cfg = app.config.clone(); let cwd = app.cwd.clone(); let hist = app.messages.clone(); let tx = tx.clone();
    tokio::spawn(async move { if let Err(e) = request(&cfg, &cwd, &hist, tx.clone(), id).await { let _ = tx.send(EventMsg::Error(id, e)); } });
}

fn send_followup(app: &mut App, tx: &mpsc::Sender<EventMsg>) {
    app.error = None; app.request_id = app.request_id.wrapping_add(1); let id = app.request_id;
    app.messages.push(Msg { role: "assistant".into(), content: String::new() });
    app.busy = true; app.status = "Processing tool result...".into(); app.scroll = 0;
    let cfg = app.config.clone(); let cwd = app.cwd.clone(); let hist = app.messages.clone(); let tx = tx.clone();
    tokio::spawn(async move { if let Err(e) = request(&cfg, &cwd, &hist, tx.clone(), id).await { let _ = tx.send(EventMsg::Error(id, e)); } });
}

pub async fn request_direct(c: &Config, hist: &[Msg], cwd: &Path) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    let id = 99999;
    request(c, cwd, hist, tx, id).await?;
    let mut result = String::new();
    while let Ok(msg) = rx.try_recv() { if let EventMsg::Chunk(_, chunk) = msg { result.push_str(&chunk); } }
    Ok(result)
}

fn refresh_async(app: &mut App, tx: mpsc::Sender<EventMsg>) {
    app.request_id = app.request_id.wrapping_add(1); let id = app.request_id; let cfg = app.config.clone(); app.busy = true;
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() { Ok(x) => x, Err(e) => { let _ = tx.send(EventMsg::Error(id, e.to_string())); return; } };
        let _ = tx.send(match rt.block_on(list_models(&cfg)) { Ok(v) => EventMsg::Models(id, v), Err(e) => EventMsg::Error(id, e) });
    });
}

async fn load_models(app: &mut App) -> Result<(), String> {
    let v = list_models(&app.config).await?; app.models = v; app.model_state.select((!app.models.is_empty()).then_some(0)); choose_model(app); Ok(())
}

fn choose_model(app: &mut App) {
    if app.config.model.as_ref().is_some_and(|m| app.models.contains(m)) { return; }
    if let Some(m) = app.models.first().cloned() { app.config.model = Some(m); let _ = app.save(); }
}

async fn http() -> Result<Client, String> {
    Client::builder().user_agent("term-code/0.1").timeout(Duration::from_secs(120)).build().map_err(|e| e.to_string())
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
        let r = h.get(format!("{b}/models")).bearer_auth(&c.api_key).send().await.map_err(|e| e.to_string())?;
        let s = r.status(); let v: Value = r.json().await.map_err(|e| e.to_string())?;
        if !s.is_success() { return Err(api_error(&v, s)); }
        v["data"].as_array().map(|a| a.iter().filter_map(|x| x["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
    } else {
        match c.provider.as_str() {
            "anthropic" => {
                let r = h.get("https://api.anthropic.com/v1/models").header("x-api-key", &c.api_key).header("anthropic-version", "2023-06-01").send().await.map_err(|e| e.to_string())?;
                let s = r.status(); let v: Value = r.json().await.map_err(|e| e.to_string())?;
                if !s.is_success() { return Err(api_error(&v, s)); }
                v["data"].as_array().map(|a| a.iter().filter_map(|x| x["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
            }
            "google" => {
                let mut models = Vec::new(); let mut page_token = None;
                loop {
                    let mut req = h.get("https://generativelanguage.googleapis.com/v1beta/models").header("x-goog-api-key", &c.api_key).query(&[("pageSize", "1000")]);
                    if let Some(token) = &page_token { req = req.query(&[("pageToken", token)]); }
                    let r = req.send().await.map_err(|e| e.to_string())?; let status = r.status(); let v: Value = r.json().await.map_err(|e| e.to_string())?;
                    if !status.is_success() { return Err(api_error(&v, status)); }
                    if let Some(items) = v["models"].as_array() {
                        models.extend(items.iter().filter(|x| x["supportedGenerationMethods"].as_array().is_some_and(|m| m.iter().any(|v| v.as_str() == Some("generateContent")))).filter_map(|x| x["name"].as_str().map(|n| n.trim_start_matches("models/").to_owned())));
                    }
                    page_token = v["nextPageToken"].as_str().map(str::to_owned); if page_token.is_none() { break; }
                }
                models
            }
            _ => return Err("Unsupported provider".into()),
        }
    };
    if models.is_empty() { return Err("Provider returned no usable models".into()); }
    Ok(models)
}

async fn request(c: &Config, cwd: &Path, hist: &[Msg], tx: mpsc::Sender<EventMsg>, id: u64) -> Result<(), String> {
    match c.provider.as_str() { "anthropic" => anthropic(c, cwd, hist, tx, id).await, "google" => google(c, cwd, hist, tx, id).await, _ => openai(c, cwd, hist, tx, id).await }
}

async fn openai(c: &Config, cwd: &Path, hist: &[Msg], tx: mpsc::Sender<EventMsg>, id: u64) -> Result<(), String> {
    let b = base(c).ok_or("Provider endpoint unavailable")?; let model = c.model.as_deref().ok_or("No model selected")?;
    let mut msgs = vec![json!({"role":"system","content":tools::system_prompt(cwd)})];
    for m in hist { let role = if m.role == "system" { "user" } else { &m.role }; msgs.push(json!({"role":role,"content":m.content})); }
    let h = http().await?; let r = h.post(format!("{b}/chat/completions")).bearer_auth(&c.api_key).json(&json!({"model":model,"messages":msgs,"stream":true})).send().await.map_err(|e| e.to_string())?;
    let s = r.status(); if !s.is_success() { return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s)); }
    sse(r.bytes_stream(), tx, "/choices/0/delta/content", id).await
}

async fn anthropic(c: &Config, cwd: &Path, hist: &[Msg], tx: mpsc::Sender<EventMsg>, id: u64) -> Result<(), String> {
    let model = c.model.as_deref().ok_or("No model selected")?; let sys = tools::system_prompt(cwd);
    let msgs = hist.iter().filter(|m| m.role != "system").map(|m| json!({"role":m.role,"content":m.content})).collect::<Vec<_>>();
    let h = http().await?; let r = h.post("https://api.anthropic.com/v1/messages").header("x-api-key", &c.api_key).header("anthropic-version", "2023-06-01").json(&json!({"model":model,"max_tokens":4096,"system":sys,"messages":msgs,"stream":true})).send().await.map_err(|e| e.to_string())?;
    let s = r.status(); if !s.is_success() { return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s)); }
    sse(r.bytes_stream(), tx, "/delta/text", id).await
}

async fn google(c: &Config, cwd: &Path, hist: &[Msg], tx: mpsc::Sender<EventMsg>, id: u64) -> Result<(), String> {
    let model = c.model.as_deref().ok_or("No model selected")?; let sys = tools::system_prompt(cwd);
    let contents = hist.iter().map(|m| json!({"role":if m.role == "assistant" { "model" } else { "user" },"parts":[{"text":m.content}]})).collect::<Vec<_>>();
    let h = http().await?; let r = h.post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent")).query(&[("alt","sse")]).header("x-goog-api-key", &c.api_key).json(&json!({"contents":contents,"system_instruction":{"parts":[{"text":sys}]}})).send().await.map_err(|e| e.to_string())?;
    let s = r.status(); if !s.is_success() { return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s)); }
    sse(r.bytes_stream(), tx, "/candidates/0/content/parts/0/text", id).await
}

async fn sse<S>(mut stream: S, tx: mpsc::Sender<EventMsg>, path: &str, id: u64) -> Result<(), String>
where S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin {
    let mut buf = String::new(); let mut received = false;
    while let Some(x) = stream.next().await {
        let chunk = String::from_utf8_lossy(&x.map_err(|e| e.to_string())?).replace("\r\n", "\n"); buf.push_str(&chunk);
        while let Some(p) = buf.find("\n\n") {
            let block = buf[..p].to_owned(); buf.drain(..p + 2);
            for line in block.lines() {
                let data = line.strip_prefix("data:").map(str::trim).unwrap_or("");
                if data == "[DONE]" { let _ = tx.send(EventMsg::Done(id)); return Ok(()); }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(s) = v.pointer(path).and_then(Value::as_str) { received = true; let _ = tx.send(EventMsg::Chunk(id, s.into())); }
                    if v.pointer("/error/message").is_some() { return Err(api_error(&v, StatusCode::BAD_GATEWAY)); }
                }
            }
        }
    }
    if !buf.trim().is_empty() {
        for line in buf.lines() {
            let data = line.strip_prefix("data:").map(str::trim).unwrap_or("");
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(s) = v.pointer(path).and_then(Value::as_str) { received = true; let _ = tx.send(EventMsg::Chunk(id, s.into())); }
                if v.pointer("/error/message").is_some() { return Err(api_error(&v, StatusCode::BAD_GATEWAY)); }
            }
        }
    }
    if !received { return Err("Model stream ended without a response".into()); }
    let _ = tx.send(EventMsg::Done(id));
    Ok(())
}

fn api_error(v: &Value, s: StatusCode) -> String {
    v.pointer("/error/message").and_then(Value::as_str).or_else(|| v.get("message").and_then(Value::as_str)).map(|x| format!("API error ({s}): {x}")).unwrap_or_else(|| format!("API error: {s}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_tab_completion() {
        let mut input = "/m".to_string(); tab_complete(&mut input); assert_eq!(input, "/model");
        let mut input = "/p".to_string(); tab_complete(&mut input); assert_eq!(input, "/provider");
        let mut input = "/c".to_string(); tab_complete(&mut input); assert_eq!(input, "/config"); tab_complete(&mut input); assert_eq!(input, "/clear");
        let mut input = "/".to_string(); tab_complete(&mut input); assert_eq!(input, "/model");
    }
}
