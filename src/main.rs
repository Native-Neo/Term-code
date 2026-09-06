use std::{fs, io, path::PathBuf, sync::mpsc, time::{Duration, SystemTime, UNIX_EPOCH}};
use chrono::Local;
use crossterm::{event::{self, Event, KeyCode, KeyEvent, KeyModifiers}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use futures_util::StreamExt;
use ratatui::{prelude::*, widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap}};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CONFIG_DIR: &str = ".nativestuff";
const CONFIG_FILE: &str = "Config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    provider: String,
    api_key: String,
    #[serde(default)] model: Option<String>,
    #[serde(default)] user_name: Option<String>,
}

#[derive(Clone)]
struct ProviderInfo { id: &'static str, name: &'static str }

const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo { id: "openai", name: "OpenAI" },
    ProviderInfo { id: "anthropic", name: "Anthropic" },
    ProviderInfo { id: "google", name: "Google" },
    ProviderInfo { id: "deepseek", name: "DeepSeek" },
    ProviderInfo { id: "qwen", name: "Alibaba/Qwen" },
    ProviderInfo { id: "openrouter", name: "OpenRouter" },
    ProviderInfo { id: "zhipu", name: "Zhipu AI" },
];

#[derive(Clone, Serialize)]
struct Message { role: String, content: String }

struct App {
    config: Option<Config>,
    messages: Vec<Message>,
    input: String,
    status: String,
    error: Option<String>,
    busy: bool,
    first_run: bool,
    setup_step: u8,
    provider_idx: usize,
    model_idx: usize,
    models: Vec<String>,
    model_state: ListState,
    show_models: bool,
    show_providers: bool,
    provider_state: ListState,
    name_input: String,
    api_input: String,
    greeting: String,
    scroll: u16,
}

enum WorkerEvent { Chunk(String), Done, Error(String), Models(Vec<String>) }

impl App {
    fn new(config: Option<Config>) -> Self {
        let first_run = config.is_none();
        let provider_idx = config.as_ref().and_then(|c| PROVIDERS.iter().position(|p| p.id == c.provider)).unwrap_or(0);
        let greeting = greeting(config.as_ref().and_then(|c| c.user_name.as_deref()));
        Self {
            config, messages: Vec::new(), input: String::new(), status: if first_run { "First-run setup".into() } else { "Ready".into() },
            error: None, busy: false, first_run, setup_step: 0, provider_idx, model_idx: 0, models: Vec::new(),
            model_state: ListState::default(), show_models: false, show_providers: false, provider_state: ListState::default(),
            name_input: String::new(), api_input: String::new(), greeting, scroll: 0,
        }
    }
    fn provider(&self) -> &ProviderInfo { &PROVIDERS[self.provider_idx] }
    fn save_config(&self) -> Result<(), String> {
        let cfg = self.config.as_ref().ok_or("No configuration")?;
        let dir = home_dir().join(CONFIG_DIR);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(CONFIG_FILE);
        let data = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
        fs::write(path, data).map_err(|e| e.to_string())?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = fs::set_permissions(dir.join(CONFIG_FILE), fs::Permissions::from_mode(0o600)); }
        Ok(())
    }
    fn finish_setup(&mut self) {
        self.config = Some(Config { provider: self.provider().id.into(), api_key: self.api_input.trim().into(), model: None, user_name: Some(self.name_input.trim().into()).filter(|s| !s.is_empty()) });
        self.first_run = false; self.setup_step = 0; self.api_input.clear(); self.name_input.clear();
        if let Err(e) = self.save_config() { self.error = Some(format!("Could not save config: {e}")); }
        self.greeting = greeting(self.config.as_ref().and_then(|c| c.user_name.as_deref()));
        self.status = "Loading models...".into();
    }
}

fn home_dir() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")) }

fn load_config() -> Result<Option<Config>, String> {
    let path = home_dir().join(CONFIG_DIR).join(CONFIG_FILE);
    if !path.exists() { return Ok(None); }
    let text = fs::read_to_string(&path).map_err(|e| format!("Could not read Config.json: {e}"))?;
    serde_json::from_str(&text).map(Some).map_err(|e| format!("Invalid Config.json formatting: {e}"))
}

fn greeting(name: Option<&str>) -> String {
    let n = name.map(|s| format!(", {s}")).unwrap_or_default();
    let h = Local::now().hour();
    let pool = if h < 12 { vec![format!("Good morning{n}."), format!("What should we work on{n}?")] }
        else if h < 18 { vec![format!("Good afternoon{n}."), format!("What should we do{n}?")] }
        else { vec![format!("Good evening{n}."), format!("Good to see you back{n}.")] };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as usize;
    pool[secs % pool.len()].clone()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match load_config() { Ok(c) => c, Err(e) => { let mut a = App::new(None); a.error = Some(e); Some(a) }.and_then(|a| a.config) };
    let mut app = App::new(config);
    if app.config.is_some() { refresh_models(&mut app); }
    let (tx, rx) = mpsc::channel();
    terminal_loop(&mut app, tx.clone(), rx)?;
    Ok(())
}

fn terminal_loop(app: &mut App, tx: mpsc::Sender<WorkerEvent>, rx: mpsc::Receiver<WorkerEvent>) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?; let mut stdout = io::stdout(); execute!(stdout, EnterAlternateScreen)?; let backend = CrosstermBackend::new(stdout); let mut term = Terminal::new(backend)?;
    let result = run_app(&mut term, app, tx, rx);
    disable_raw_mode()?; execute!(term.backend_mut(), LeaveAlternateScreen)?; term.show_cursor()?; result.map_err(Into::into)
}

fn run_app<B: Backend>(term: &mut Terminal<B>, app: &mut App, tx: mpsc::Sender<WorkerEvent>, rx: mpsc::Receiver<WorkerEvent>) -> io::Result<()> {
    loop {
        while let Ok(ev) = rx.try_recv() { match ev {
            WorkerEvent::Chunk(s) => { if let Some(m) = app.messages.last_mut() { m.content.push_str(&s); } }
            WorkerEvent::Done => { app.busy = false; app.status = "Ready".into(); }
            WorkerEvent::Error(e) => { app.busy = false; app.status = "Error".into(); app.error = Some(e); if app.messages.last().map(|m| m.content.is_empty()).unwrap_or(false) { app.messages.pop(); } }
            WorkerEvent::Models(m) => { app.models = m; app.model_state.select(Some(0)); app.busy = false; app.status = if app.models.is_empty() { "No models found".into() } else { "Ready".into() }; apply_default_model(app); }
        }}
        term.draw(|f| ui(f, app))?;
        if event::poll(Duration::from_millis(50))? { if let Event::Key(k) = event::read()? { if handle_key(app, k, &tx) { break; } } }
    }
    Ok(())
}

fn handle_key(app: &mut App, k: KeyEvent, tx: &mpsc::Sender<WorkerEvent>) -> bool {
    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) { return true; }
    if app.first_run {
        match app.setup_step {
            0 => match k.code { KeyCode::Up => app.provider_idx = app.provider_idx.saturating_sub(1), KeyCode::Down => app.provider_idx = (app.provider_idx + 1).min(PROVIDERS.len()-1), KeyCode::Enter => app.setup_step = 1, KeyCode::Esc => return true, _ => {} },
            1 => text_key(&mut app.api_input, k, || { app.setup_step = 2 }),
            2 => text_key(&mut app.name_input, k, || { app.finish_setup(); refresh_models(app); }),
            _ => {}
        } return false;
    }
    if app.show_models { return model_keys(app, k); }
    if app.show_providers { return provider_keys(app, k); }
    match k.code {
        KeyCode::Esc => if app.busy { app.busy = false; app.status = "Cancelled".into(); } else { return true; },
        KeyCode::Enter if !app.busy && !app.input.trim().is_empty() => send_prompt(app, tx),
        KeyCode::Backspace => { app.input.pop(); },
        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => app.input.push(c),
        KeyCode::Up if app.input.is_empty() => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Down if app.input.is_empty() => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::Char('l') if k.modifiers.contains(KeyModifiers::CONTROL) => { app.messages.clear(); app.scroll = 0; },
        _ => {}
    }
    if app.input.starts_with('/') && k.code == KeyCode::Enter { command(app, tx); }
    false
}

fn text_key(s: &mut String, k: KeyEvent, enter: impl FnOnce()) { match k.code { KeyCode::Enter => enter(), KeyCode::Backspace => { s.pop(); }, KeyCode::Char(c) => s.push(c), _ => {} } }

fn command(app: &mut App, tx: &mpsc::Sender<WorkerEvent>) {
    let cmd = app.input.trim().to_lowercase(); app.input.clear();
    match cmd.as_str() { "/help" => app.status = "/model /provider /config /new /clear /quit".into(), "/model" => { app.show_models = true; }, "/provider" => { app.show_providers = true; app.provider_state.select(Some(app.provider_idx)); }, "/config" => app.status = format!("Provider: {} | Model: {} | API key: configured", app.provider().name, app.config.as_ref().and_then(|c| c.model.as_deref()).unwrap_or("auto")), "/new" | "/clear" => app.messages.clear(), "/quit" | "/exit" => { app.status = "Use Ctrl+C or Esc to quit".into(); }, _ => if !cmd.is_empty() { app.status = format!("Unknown command: {cmd}"); } }
    if cmd == "/model" && app.models.is_empty() && !app.busy { refresh_models_async(app, tx.clone()); }
}

fn model_keys(app: &mut App, k: KeyEvent) -> bool { match k.code { KeyCode::Esc => app.show_models = false, KeyCode::Up => app.model_state.select(Some(app.model_state.selected().unwrap_or(0).saturating_sub(1))), KeyCode::Down => { let i = app.model_state.selected().unwrap_or(0); if i + 1 < app.models.len() { app.model_state.select(Some(i+1)); } }, KeyCode::Enter => { if let Some(i) = app.model_state.selected() { if let Some(m) = app.models.get(i).cloned() { if let Some(c) = app.config.as_mut() { c.model = Some(m); let _ = app.save_config(); } app.show_models = false; } } }, _ => {} } false }

fn provider_keys(app: &mut App, k: KeyEvent) -> bool { match k.code { KeyCode::Esc => app.show_providers = false, KeyCode::Up => app.provider_state.select(Some(app.provider_state.selected().unwrap_or(0).saturating_sub(1))), KeyCode::Down => { let i = app.provider_state.selected().unwrap_or(0); if i + 1 < PROVIDERS.len() { app.provider_state.select(Some(i+1)); } }, KeyCode::Enter => { if let Some(i) = app.provider_state.selected() { app.provider_idx = i; app.show_providers = false; app.setup_step = 1; app.first_run = true; app.api_input.clear(); } }, _ => {} } false }

fn send_prompt(app: &mut App, tx: &mpsc::Sender<WorkerEvent>) { let input = app.input.trim().to_string(); app.input.clear(); app.error = None; app.messages.push(Message { role: "user".into(), content: input }); app.messages.push(Message { role: "assistant".into(), content: String::new() }); app.busy = true; app.status = "Thinking...".into(); let cfg = app.config.clone().unwrap(); let history = app.messages.clone(); let tx = tx.clone(); tokio::spawn(async move { match request(&cfg, &history, tx.clone()).await { Ok(()) => {}, Err(e) => { let _ = tx.send(WorkerEvent::Error(e)); } } }); }

fn refresh_models(app: &mut App) { if let Some(cfg) = app.config.clone() { let rt = tokio::runtime::Runtime::new().unwrap(); match rt.block_on(list_models(&cfg)) { Ok(m) => { app.models = m; app.model_state.select(Some(0)); apply_default_model(app); }, Err(e) => app.error = Some(e) } } }
fn refresh_models_async(app: &mut App, tx: mpsc::Sender<WorkerEvent>) { if let Some(cfg) = app.config.clone() { app.busy = true; std::thread::spawn(move || { let rt = tokio::runtime::Runtime::new().unwrap(); let _ = tx.send(match rt.block_on(list_models(&cfg)) { Ok(m) => WorkerEvent::Models(m), Err(e) => WorkerEvent::Error(e) }); }); } }
fn apply_default_model(app: &mut App) { if let Some(c) = app.config.as_mut() { if c.model.as_ref().map(|m| !app.models.contains(m)).unwrap_or(true) { c.model = app.models.first().cloned(); let _ = app.save_config(); } } }

async fn client() -> Result<Client, String> { Client::builder().user_agent("term-code/0.1").timeout(Duration::from_secs(120)).build().map_err(|e| e.to_string()) }

fn base(cfg: &Config) -> Option<&'static str> { match cfg.provider.as_str() { "openai" => Some("https://api.openai.com/v1"), "deepseek" => Some("https://api.deepseek.com"), "qwen" => Some("https://dashscope.aliyuncs.com/compatible-mode/v1"), "openrouter" => Some("https://openrouter.ai/api/v1"), "zhipu" => Some("https://open.bigmodel.cn/api/paas/v4"), _ => None } }

async fn list_models(cfg: &Config) -> Result<Vec<String>, String> {
    let c = client().await?;
    if let Some(b) = base(cfg) { let r = c.get(format!("{b}/models")).bearer_auth(&cfg.api_key).send().await.map_err(|e| e.to_string())?; let status=r.status(); let v:Value=r.json().await.map_err(|e| e.to_string())?; if !status.is_success(){return Err(api_error(&v,status));} return Ok(v.get("data").and_then(|x|x.as_array()).map(|a|a.iter().filter_map(|m|m.get("id").and_then(Value::as_str).map(str::to_owned)).collect()).unwrap_or_default()); }
    match cfg.provider.as_str() {
        "anthropic" => { let r=c.get("https://api.anthropic.com/v1/models").header("x-api-key",&cfg.api_key).header("anthropic-version","2023-06-01").send().await.map_err(|e|e.to_string())?; let status=r.status(); let v:Value=r.json().await.map_err(|e|e.to_string())?; if !status.is_success(){return Err(api_error(&v,status));} Ok(v.get("data").and_then(|x|x.as_array()).map(|a|a.iter().filter_map(|m|m.get("id").and_then(Value::as_str).map(str::to_owned)).collect()).unwrap_or_default()) }
        "google" => { let r=c.get(format!("https://generativelanguage.googleapis.com/v1beta/models?key={}",cfg.api_key)).send().await.map_err(|e|e.to_string())?; let status=r.status(); let v:Value=r.json().await.map_err(|e|e.to_string())?; if !status.is_success(){return Err(api_error(&v,status));} Ok(v.get("models").and_then(|x|x.as_array()).map(|a|a.iter().filter_map(|m|m.get("name").and_then(Value::as_str).map(|s|s.trim_start_matches("models/").to_owned())).filter(|m|m.contains("generateContent")).collect()).unwrap_or_default()) }
        _ => Err("Unsupported provider".into())
    }
}

async fn request(cfg: &Config, history: &[Message], tx: mpsc::Sender<WorkerEvent>) -> Result<(), String> {
    match cfg.provider.as_str() { "anthropic" => anthropic(cfg,history,tx).await, "google" => google(cfg,history,tx).await, _ => openai_compat(cfg,history,tx).await }
}

async fn openai_compat(cfg:&Config, history:&[Message], tx:mpsc::Sender<WorkerEvent>) -> Result<(),String> {
    let b=base(cfg).ok_or("Provider endpoint unavailable")?; let model=cfg.model.as_deref().ok_or("No model selected")?; let c=client().await?;
    let msgs:Vec<Value>=history.iter().map(|m|json!({"role":m.role,"content":m.content})).collect();
    let r=c.post(format!("{b}/chat/completions")).bearer_auth(&cfg.api_key).json(&json!({"model":model,"messages":msgs,"stream":true})).send().await.map_err(|e|e.to_string())?;
    let status=r.status(); if !status.is_success(){let v=r.json::<Value>().await.unwrap_or_default();return Err(api_error(&v,status));}
    let mut stream=r.bytes_stream(); let mut buf=String::new();
    while let Some(x)=stream.next().await { let bytes=x.map_err(|e|e.to_string())?; buf.push_str(&String::from_utf8_lossy(&bytes)); while let Some(pos)=buf.find("\n\n") { let block=buf[..pos].to_string(); buf.drain(..pos+2); for line in block.lines() { let d=line.strip_prefix("data: ").unwrap_or(""); if d=="[DONE]" { let _=tx.send(WorkerEvent::Done); return Ok(()); } if !d.is_empty() { if let Ok(v)=serde_json::from_str::<Value>(d) { if let Some(s)=v.pointer("/choices/0/delta/content").and_then(Value::as_str) { let _=tx.send(WorkerEvent::Chunk(s.into())); } } } } } }
    let _=tx.send(WorkerEvent::Done); Ok(())
}

async fn anthropic(cfg:&Config, history:&[Message], tx:mpsc::Sender<WorkerEvent>)->Result<(),String>{
    let model=cfg.model.as_deref().ok_or("No model selected")?; let c=client().await?; let msgs:Vec<Value>=history.iter().filter(|m|m.role!="system").map(|m|json!({"role":m.role,"content":m.content})).collect();
    let r=c.post("https://api.anthropic.com/v1/messages").header("x-api-key",&cfg.api_key).header("anthropic-version","2023-06-01").header("anthropic-dangerous-direct-browser-access","true").json(&json!({"model":model,"max_tokens":4096,"messages":msgs,"stream":true})).send().await.map_err(|e|e.to_string())?;
    let status=r.status(); if !status.is_success(){let v=r.json::<Value>().await.unwrap_or_default();return Err(api_error(&v,status));} let mut stream=r.bytes_stream(); let mut buf=String::new();
    while let Some(x)=stream.next().await { buf.push_str(&String::from_utf8_lossy(&x.map_err(|e|e.to_string())?)); while let Some(pos)=buf.find("\n\n"){let block=buf[..pos].to_string();buf.drain(..pos+2);for line in block.lines(){if let Some(d)=line.strip_prefix("data: "){if let Ok(v)=serde_json::from_str::<Value>(d){if let Some(s)=v.pointer("/delta/text").and_then(Value::as_str){let _=tx.send(WorkerEvent::Chunk(s.into()));}}}}} }
    let _=tx.send(WorkerEvent::Done);Ok(())
}

async fn google(cfg:&Config, history:&[Message], tx:mpsc::Sender<WorkerEvent>)->Result<(),String>{
    let model=cfg.model.as_deref().ok_or("No model selected")?; let c=client().await?; let contents:Vec<Value>=history.iter().filter(|m|m.role!="system").map(|m|json!({"role":if m.role=="assistant"{"model"}else{"user"},"parts":[{"text":m.content}]})).collect();
    let url=format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",model,cfg.api_key); let r=c.post(url).json(&json!({"contents":contents})).send().await.map_err(|e|e.to_string())?; let status=r.status();let v:Value=r.json().await.map_err(|e|e.to_string())?;if !status.is_success(){return Err(api_error(&v,status));} if let Some(s)=v.pointer("/candidates/0/content/parts/0/text").and_then(Value::as_str){let _=tx.send(WorkerEvent::Chunk(s.into()));}let _=tx.send(WorkerEvent::Done);Ok(())
}

fn api_error(v:&Value,status:StatusCode)->String { v.pointer("/error/message").and_then(Value::as_str).or_else(||v.get("message").and_then(Value::as_str)).map(|m|format!("API error ({}): {m}",status)).unwrap_or_else(||format!("API error: {status}")) }

fn ui(f:&mut Frame, app:&App){
    let area=f.area(); let chunks=Layout::vertical([Constraint::Length(2),Constraint::Min(3),Constraint::Length(3),Constraint::Length(1)]).split(area);
    let title=if app.first_run { format!("Term Code — Setup — {}",app.provider().name) } else { format!("Term Code  •  {}  •  {}",app.provider().name,app.config.as_ref().and_then(|c|c.model.as_deref()).unwrap_or("no model")) };
    f.render_widget(Paragraph::new(title).bold().alignment(Alignment::Center),chunks[0]);
    if app.first_run { setup_ui(f,app,chunks[1]); } else { chat_ui(f,app,chunks[1]); }
    f.render_widget(Paragraph::new(if app.first_run { setup_hint(app) } else { format!("> {}",app.input) }).block(Block::default().borders(Borders::ALL).title(if app.first_run {"Setup"} else {"Input"})),chunks[2]);
    f.render_widget(Paragraph::new(app.error.as_deref().unwrap_or(&app.status)).wrap(Wrap{trim:true}),chunks[3]);
    if app.show_models { popup_models(f,app); } if app.show_providers { popup_providers(f,app); }
}
fn setup_hint(app:&App)->String { match app.setup_step {0=>"↑/↓ select provider • Enter continue".into(),1=>format!("API key: {}",mask(&app.api_input)),2=>format!("Name: {}",app.name_input),_=>"".into()} }
fn mask(s:&str)->String { "•".repeat(s.chars().count()) }
fn setup_ui(f:&mut Frame,app:&App,a:Rect){let items:Vec<ListItem>=PROVIDERS.iter().map(|p|ListItem::new(p.name)).collect();let mut st=ListState::default();st.select(Some(app.provider_idx));f.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).title("Provider")),a); if app.setup_step>0 { let txt=if app.setup_step==1{"Paste your API key and press Enter."}else{"What should I call you?"}; f.render_widget(Paragraph::new(txt).block(Block::default().borders(Borders::ALL).title("Next")),a); }}
fn chat_ui(f:&mut Frame,app:&App,a:Rect){let mut text=app.messages.iter().map(|m|format!("{}:\n{}\n",if m.role=="user"{"You"}else{"Term Code"},m.content)).collect::<Vec<_>>().join("\n");if text.is_empty(){text=app.greeting.clone();}f.render_widget(Paragraph::new(text).scroll((app.scroll,0)).wrap(Wrap{trim:false}).block(Block::default().borders(Borders::ALL)),a);}
fn popup_models(f:&mut Frame,app:&App){let a=centered(f.area(),60,70);let items=app.models.iter().map(|m|ListItem::new(m.as_str())).collect::<Vec<_>>();f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).title("Models — Esc close")),a,&mut app.model_state.clone());}
fn popup_providers(f:&mut Frame,app:&App){let a=centered(f.area(),50,60);let items=PROVIDERS.iter().map(|p|ListItem::new(p.name)).collect::<Vec<_>>();f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).title("Providers — Esc close")),a,&mut app.provider_state.clone());}
fn centered(r:Rect,w:u16,h:u16)->Rect{let v=Layout::vertical([Constraint::Percentage((100-h)/2),Constraint::Percentage(h),Constraint::Percentage((100-h)/2)]).split(r);Layout::horizontal([Constraint::Percentage((100-w)/2),Constraint::Percentage(w),Constraint::Percentage((100-w)/2)]).split(v[1])[1]}
trait Hour {fn hour(&self)->u32;} impl Hour for chrono::DateTime<Local>{fn hour(&self)->u32{use chrono::Timelike;self.time().hour()}}
