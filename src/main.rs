use chrono::{Local, Timelike};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::{Stream, StreamExt};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DIR: &str = ".nativestuff";
const FILE: &str = "Config.json";

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    provider: String,
    api_key: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    user_name: Option<String>,
}

struct Provider {
    id: &'static str,
    name: &'static str,
}
const PROVIDERS: &[Provider] = &[
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

#[derive(Clone)]
struct Msg {
    role: String,
    content: String,
}
enum EventMsg {
    Chunk(u64, String),
    Done(u64),
    Error(u64, String),
    Models(u64, Vec<String>),
}
enum Mode {
    Chat,
    Setup(u8),
    Models,
    Providers,
}
struct App {
    config: Config,
    messages: Vec<Msg>,
    input: String,
    status: String,
    error: Option<String>,
    busy: bool,
    mode: Mode,
    provider: usize,
    models: Vec<String>,
    model_state: ListState,
    provider_state: ListState,
    api_input: String,
    name_input: String,
    greeting: String,
    scroll: u16,
    request_id: u64,
}

impl App {
    fn new(config: Config, setup: bool, error: Option<String>) -> Self {
        let provider = PROVIDERS
            .iter()
            .position(|p| p.id == config.provider)
            .unwrap_or(0);
        let name = config.user_name.clone();
        let mode = if setup {
            Mode::Setup(0)
        } else {
            Mode::Setup(3)
        };
        Self {
            config,
            messages: Vec::new(),
            input: String::new(),
            status: if setup {
                "Choose a provider"
            } else {
                "Fix Config.json and restart"
            }
            .into(),
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
    execute!(out, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;
    let result = loop_app(&mut term, app, tx, rx);
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
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
                    }
                }
                EventMsg::Done(id) if id == app.request_id => {
                    app.busy = false;
                    app.status = "Ready".into();
                }
                EventMsg::Error(id, e) if id == app.request_id => {
                    app.busy = false;
                    app.error = Some(e);
                    app.status = "Error".into();
                    if app.messages.last().is_some_and(|m| m.content.is_empty()) {
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
                _ => {}
            }
        }
        term.draw(|f| draw(f, app))?;
        if event::poll(Duration::from_millis(40))? {
            if let Event::Key(k) = event::read()?
                && key(app, k, &tx)
            {
                break;
            }
        }
    }
    Ok(())
}

fn key(app: &mut App, k: KeyEvent, tx: &mpsc::Sender<EventMsg>) -> bool {
    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    match app.mode {
        Mode::Setup(step) => return setup_key(app, k, step, tx),
        Mode::Models => {
            model_key(app, k);
            return false;
        }
        Mode::Providers => {
            provider_key(app, k);
            return false;
        }
        Mode::Chat => {}
    }
    if k.code == KeyCode::Enter {
        if app.input.starts_with('/') {
            return command(app, tx);
        }
        if !app.busy && !app.input.trim().is_empty() {
            send(app, tx);
        }
        return false;
    }
    match k.code {
        KeyCode::Esc if app.busy => {
            app.request_id = app.request_id.wrapping_add(1);
            app.busy = false;
            app.status = "Cancelled".into();
        }
        KeyCode::Esc => return true,
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Up if app.input.is_empty() => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Down if app.input.is_empty() => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => app.input.push(c),
        _ => {}
    }
    false
}

fn setup_key(app: &mut App, k: KeyEvent, step: u8, tx: &mpsc::Sender<EventMsg>) -> bool {
    match step {
        0 => match k.code {
            KeyCode::Up => app.provider = app.provider.saturating_sub(1),
            KeyCode::Down => app.provider = (app.provider + 1).min(PROVIDERS.len() - 1),
            KeyCode::Enter => {
                app.mode = Mode::Setup(1);
                app.status = "Paste your API key".into();
            }
            KeyCode::Esc => return true,
            _ => {}
        },
        1 => match k.code {
            KeyCode::Enter if !app.api_input.trim().is_empty() => {
                app.mode = Mode::Setup(2);
                app.status = "What should I call you?".into();
            }
            KeyCode::Backspace => {
                app.api_input.pop();
            }
            KeyCode::Char(c) => app.api_input.push(c),
            _ => {}
        },
        2 => match k.code {
            KeyCode::Enter => {
                app.config.provider = PROVIDERS[app.provider].id.into();
                app.config.api_key = app.api_input.trim().into();
                app.config.user_name =
                    (!app.name_input.trim().is_empty()).then(|| app.name_input.trim().into());
                app.config.model = None;
                if let Err(e) = app.save() {
                    app.error = Some(e);
                    app.status = "Config save failed".into();
                    return false;
                }
                app.error = None;
                app.greeting = greeting(app.config.user_name.as_deref());
                app.models.clear();
                app.mode = Mode::Chat;
                app.status = "Loading models...".into();
                refresh_async(app, tx.clone());
            }
            KeyCode::Backspace => {
                app.name_input.pop();
            }
            KeyCode::Char(c) => app.name_input.push(c),
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

fn command(app: &mut App, tx: &mpsc::Sender<EventMsg>) -> bool {
    let c = app.input.trim().to_lowercase();
    app.input.clear();
    match c.as_str() {
        "/help" => app.status = "/model /provider /config /new /clear /quit".into(),
        "/model" => {
            app.mode = Mode::Models;
            if app.models.is_empty() {
                refresh_async(app, tx.clone());
            }
        }
        "/provider" => {
            app.provider_state.select(Some(app.provider));
            app.mode = Mode::Providers;
        }
        "/config" => {
            app.status = format!(
                "Provider: {} | Model: {} | API key: configured",
                PROVIDERS[app.provider].name,
                app.config.model.as_deref().unwrap_or("auto")
            )
        }
        "/new" | "/clear" => {
            app.messages.clear();
            app.scroll = 0;
        }
        "/quit" | "/exit" => return true,
        _ => app.status = format!("Unknown command: {c}"),
    }
    false
}
fn model_key(app: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => app.mode = Mode::Chat,
        KeyCode::Up => app.model_state.select(Some(
            app.model_state.selected().unwrap_or(0).saturating_sub(1),
        )),
        KeyCode::Down => {
            let i = app.model_state.selected().unwrap_or(0);
            if i + 1 < app.models.len() {
                app.model_state.select(Some(i + 1));
            }
        }
        KeyCode::Enter => {
            if let Some(i) = app.model_state.selected() {
                if let Some(m) = app.models.get(i).cloned() {
                    app.config.model = Some(m);
                    if let Err(e) = app.save() {
                        app.error = Some(e);
                    } else {
                        app.mode = Mode::Chat;
                    }
                }
            }
        }
        _ => {}
    }
}
fn provider_key(app: &mut App, k: KeyEvent) {
    match k.code {
        KeyCode::Esc => app.mode = Mode::Chat,
        KeyCode::Up => app.provider_state.select(Some(
            app.provider_state.selected().unwrap_or(0).saturating_sub(1),
        )),
        KeyCode::Down => {
            let i = app.provider_state.selected().unwrap_or(0);
            if i + 1 < PROVIDERS.len() {
                app.provider_state.select(Some(i + 1));
            }
        }
        KeyCode::Enter => {
            if let Some(i) = app.provider_state.selected() {
                app.provider = i;
                app.api_input.clear();
                app.mode = Mode::Setup(1);
                app.status = "Paste your API key".into();
            }
        }
        _ => {}
    }
}

fn send(app: &mut App, tx: &mpsc::Sender<EventMsg>) {
    let text = app.input.trim().to_string();
    app.input.clear();
    app.error = None;
    app.request_id = app.request_id.wrapping_add(1);
    let id = app.request_id;
    app.messages.push(Msg {
        role: "user".into(),
        content: text,
    });
    app.messages.push(Msg {
        role: "assistant".into(),
        content: String::new(),
    });
    app.busy = true;
    app.status = "Thinking...".into();
    let cfg = app.config.clone();
    let hist = app.messages.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = request(&cfg, &hist, tx.clone(), id).await {
            let _ = tx.send(EventMsg::Error(id, e));
        }
    });
}
fn refresh_async(app: &mut App, tx: mpsc::Sender<EventMsg>) {
    app.request_id = app.request_id.wrapping_add(1);
    let id = app.request_id;
    let cfg = app.config.clone();
    app.busy = true;
    std::thread::spawn(move || {
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
async fn load_models(app: &mut App) -> Result<(), String> {
    let v = list_models(&app.config).await?;
    app.models = v;
    app.model_state
        .select((!app.models.is_empty()).then_some(0));
    choose_model(app);
    Ok(())
}
fn choose_model(app: &mut App) {
    if app
        .config
        .model
        .as_ref()
        .is_some_and(|m| app.models.contains(m))
    {
        return;
    }
    if let Some(m) = app.models.first().cloned() {
        app.config.model = Some(m);
        let _ = app.save();
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
                    .collect::<Vec<_>>()
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
                            .collect::<Vec<_>>()
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
                    if let Some(token) = &page_token {
                        req = req.query(&[("pageToken", token)]);
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
                        );
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
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
) -> Result<(), String> {
    match c.provider.as_str() {
        "anthropic" => anthropic(c, hist, tx, id).await,
        "google" => google(c, hist, tx, id).await,
        _ => openai(c, hist, tx, id).await,
    }
}
async fn openai(
    c: &Config,
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
) -> Result<(), String> {
    let b = base(c).ok_or("Provider endpoint unavailable")?;
    let model = c.model.as_deref().ok_or("No model selected")?;
    let msgs: Vec<Value> = hist
        .iter()
        .map(|m| json!({"role":m.role,"content":m.content}))
        .collect::<Vec<_>>();
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
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
) -> Result<(), String> {
    let model = c.model.as_deref().ok_or("No model selected")?;
    let msgs: Vec<Value> = hist
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| json!({"role":m.role,"content":m.content}))
        .collect::<Vec<_>>();
    let h = http().await?;
    let r = h
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &c.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({"model":model,"max_tokens":4096,"messages":msgs,"stream":true}))
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
    hist: &[Msg],
    tx: mpsc::Sender<EventMsg>,
    id: u64,
) -> Result<(), String> {
    let model = c.model.as_deref().ok_or("No model selected")?;
    let contents: Vec<Value> = hist.iter().map(|m| json!({"role":if m.role == "assistant" { "model" } else { "user" },"parts":[{"text":m.content}]})).collect::<Vec<_>>();
    let h = http().await?;
    let r = h
        .post(format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent"
        ))
        .query(&[("alt", "sse")])
        .header("x-goog-api-key", &c.api_key)
        .json(&json!({"contents":contents}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let s = r.status();
    if !s.is_success() {
        return Err(api_error(&r.json::<Value>().await.unwrap_or_default(), s));
    }
    sse(
        r.bytes_stream(),
        tx,
        "/candidates/0/content/parts/0/text",
        id,
    )
    .await
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
    while let Some(x) = stream.next().await {
        let chunk = String::from_utf8_lossy(&x.map_err(|e| e.to_string())?).replace("\r\n", "\n");
        buf.push_str(&chunk);
        while let Some(p) = buf.find("\n\n") {
            let block = buf[..p].to_owned();
            buf.drain(..p + 2);
            for line in block.lines() {
                let data = line.strip_prefix("data:").map(str::trim).unwrap_or("");
                if data == "[DONE]" {
                    let _ = tx.send(EventMsg::Done(id));
                    return Ok(());
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(s) = v.pointer(path).and_then(Value::as_str) {
                        let _ = tx.send(EventMsg::Chunk(id, s.into()));
                    }
                }
            }
        }
    }
    if !buf.trim().is_empty() {
        for line in buf.lines() {
            let data = line.strip_prefix("data:").map(str::trim).unwrap_or("");
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(s) = v.pointer(path).and_then(Value::as_str) {
                    let _ = tx.send(EventMsg::Chunk(id, s.into()));
                }
            }
        }
    }
    let _ = tx.send(EventMsg::Done(id));
    Ok(())
}
fn api_error(v: &Value, s: StatusCode) -> String {
    v.pointer("/error/message")
        .and_then(Value::as_str)
        .or_else(|| v.get("message").and_then(Value::as_str))
        .map(|x| format!("API error ({s}): {x}"))
        .unwrap_or_else(|| format!("API error: {s}"))
}

fn draw(f: &mut Frame, a: &App) {
    let l = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(f.area());
    let head = if matches!(a.mode, Mode::Chat) {
        format!(
            "Term Code  •  {}  •  {}",
            PROVIDERS[a.provider].name,
            a.config.model.as_deref().unwrap_or("auto")
        )
    } else {
        "Term Code".into()
    };
    f.render_widget(Paragraph::new(head), l[0]);
    match a.mode {
        Mode::Chat => chat(f, a, l[1]),
        Mode::Setup(step) => setup(f, a, l[1], step),
        Mode::Models => list(f, &a.models, &a.model_state, l[1], "Models"),
        Mode::Providers => {
            let v = PROVIDERS
                .iter()
                .map(|p| p.name.to_owned())
                .collect::<Vec<_>>();
            list(f, &v, &a.provider_state, l[1], "Providers");
        }
    }
    let input = match a.mode {
        Mode::Setup(1) => format!("API key: {}", "•".repeat(a.api_input.chars().count())),
        Mode::Setup(2) => format!("Name: {}", a.name_input),
        Mode::Setup(3) => "Config.json is invalid. Press Esc or Q to exit.".into(),
        _ => format!("› {}", a.input),
    };
    f.render_widget(
        Paragraph::new(input).block(Block::default().borders(Borders::NONE)),
        l[2],
    );
    f.render_widget(
        Paragraph::new(a.error.as_deref().unwrap_or(&a.status)),
        l[3],
    );
}
fn chat(f: &mut Frame, a: &App, r: Rect) {
    let mut s = a
        .messages
        .iter()
        .map(|m| {
            format!(
                "{}:\n{}\n",
                if m.role == "user" {
                    "› You"
                } else {
                    "• Term Code"
                },
                m.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if s.is_empty() {
        s = a.greeting.clone();
    }
    f.render_widget(
        Paragraph::new(s)
            .scroll((a.scroll, 0))
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::NONE)),
        r,
    );
}
fn setup(f: &mut Frame, a: &App, r: Rect, step: u8) {
    let title = match step {
        0 => "Select provider",
        1 => "API key",
        2 => "Your name",
        3 => "Invalid configuration",
        _ => "Setup",
    };
    let text = match step {
        0 => PROVIDERS
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{} {}", if i == a.provider { ">" } else { " " }, p.name))
            .collect::<Vec<_>>()
            .join("\n"),
        1 => "Paste your API key, then press Enter.".into(),
        2 => "Enter the name Term Code should call you.".into(),
        3 => a
            .error
            .clone()
            .unwrap_or_else(|| "Invalid Config.json".into()),
        _ => String::new(),
    };
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::NONE)
                .title(Span::styled(title, Style::default().cyan().bold())),
        ),
        r,
    );
}
fn list(f: &mut Frame, v: &[String], st: &ListState, r: Rect, title: &str) {
    let mut s = st.clone();
    f.render_stateful_widget(
        List::new(
            v.iter()
                .map(|x| ListItem::new(x.clone()))
                .collect::<Vec<_>>(),
        )
        .highlight_symbol("› ")
        .block(
            Block::default()
                .borders(Borders::NONE)
                .title(Span::styled(title, Style::default().cyan().bold())),
        ),
        r,
        &mut s,
    );
}
