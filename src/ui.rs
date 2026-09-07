use crate::{App, Mode, PROVIDERS};
use crate::theme::Theme;
use ratatui::{prelude::*, widgets::*};

pub fn draw(f: &mut Frame, app: &App) {
    let theme = Theme::default();
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(6), Constraint::Length(3), Constraint::Length(1)]).split(f.area());
    draw_header(f, app, chunks[0], &theme);
    draw_chat(f, app, chunks[1], &theme);
    match app.mode {
        Mode::Models => draw_models_modal(f, app, chunks[1], &theme),
        Mode::Providers => draw_providers_modal(f, app, chunks[1], &theme),
        Mode::Setup(step) => draw_setup_modal(f, app, chunks[1], step, &theme),
        Mode::Chat => {}
    }
    draw_input(f, app, chunks[2], &theme);
    draw_footer(f, app, chunks[3], &theme);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let provider_name = PROVIDERS.get(app.provider).map(|p| p.name).unwrap_or("Unknown");
    let model_name = app.config.model.as_deref().unwrap_or("auto");
    let (status_text, status_style) = if app.busy { (" [BUSY] Thinking... ", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)) } else if app.error.is_some() { (" [ERR] Error ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)) } else { (" [OK] Ready ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)) };
    let chunks = Layout::horizontal([Constraint::Length(18), Constraint::Min(20), Constraint::Length(22)]).split(area);
    let brand = Paragraph::new(Line::from(vec![Span::styled(" # ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled("TERM CODE", Style::default().fg(theme.text).add_modifier(Modifier::BOLD))])).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.primary)));
    f.render_widget(brand, chunks[0]);
    let folder_name = app.cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| app.cwd.display().to_string());
    let info = Paragraph::new(Line::from(vec![Span::styled(" Provider: ", Style::default().fg(theme.text_muted)), Span::styled(provider_name, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" | Model: ", Style::default().fg(theme.text_muted)), Span::styled(model_name, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)), Span::styled(" | Dir: ", Style::default().fg(theme.text_muted)), Span::styled(folder_name, Style::default().fg(theme.success).add_modifier(Modifier::BOLD))])).alignment(Alignment::Center).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_inactive)));
    f.render_widget(info, chunks[1]);
    let status = Paragraph::new(Line::from(vec![Span::styled(status_text, status_style)])).alignment(Alignment::Right).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(if app.busy { Style::default().fg(theme.warning) } else if app.error.is_some() { Style::default().fg(theme.error) } else { Style::default().fg(theme.border_inactive) }));
    f.render_widget(status, chunks[2]);
}

fn draw_chat(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tool_height = if app.tool_calls.is_empty() { 0 } else if app.tools_expanded { (app.tool_calls.len().min(4) as u16) + 1 } else { 1 };
    let (chat_area, tool_area) = if tool_height > 0 { let x = Layout::vertical([Constraint::Min(1), Constraint::Length(tool_height)]).split(area); (x[0], Some(x[1])) } else { (area, None) };
    let scroll_info = if app.scroll > 0 { format!(" (Scroll: +{} lines up) ", app.scroll) } else { String::new() };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_inactive)).title(Span::styled(format!(" # Conversation{} ", scroll_info), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)));
    if app.messages.is_empty() {
        let provider_name = PROVIDERS.get(app.provider).map(|p| p.name).unwrap_or("Unknown");
        let model_name = app.config.model.as_deref().unwrap_or("auto");
        let lines = vec![Line::from(""), Line::from(vec![Span::styled("   # ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled("WELCOME TO TERM CODE (AGENTIC EDITION)", Style::default().fg(theme.text).add_modifier(Modifier::BOLD))]), Line::from(vec![Span::styled("     Agentic Cloud AI Terminal Interface", Style::default().fg(theme.text_muted))]), Line::from(""), Line::from(vec![Span::styled("   Workspace Dir: ", Style::default().fg(theme.text_muted)), Span::styled(app.cwd.display().to_string(), Style::default().fg(theme.success).add_modifier(Modifier::BOLD))]), Line::from(vec![Span::styled("   Connected to:  ", Style::default().fg(theme.text_muted)), Span::styled(provider_name, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled("  |  Model: ", Style::default().fg(theme.text_muted)), Span::styled(model_name, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD))]), Line::from(vec![Span::styled(format!("   Greeting:      {}", app.greeting), Style::default().fg(theme.text_muted))]), Line::from(""), Line::from(vec![Span::styled("   Agentic Capabilities Enabled:", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))]), Line::from(vec![Span::styled("     Tools:     ", Style::default().fg(theme.primary)), Span::styled("list_dir, read_file, write_file, run_cmd, search, spawn_subagent", Style::default().fg(theme.text_muted))]), Line::from(""), Line::from(vec![Span::styled("   Quick Commands:", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))]), Line::from(vec![Span::styled("     /model     ", Style::default().fg(theme.primary)), Span::styled("Select AI model", Style::default().fg(theme.text_muted))]), Line::from(vec![Span::styled("     /provider  ", Style::default().fg(theme.primary)), Span::styled("Switch provider (OpenAI, Anthropic, Google...)", Style::default().fg(theme.text_muted))]), Line::from(vec![Span::styled("     /config    ", Style::default().fg(theme.primary)), Span::styled("View current active configuration", Style::default().fg(theme.text_muted))]), Line::from(vec![Span::styled("     /new       ", Style::default().fg(theme.primary)), Span::styled("Clear chat history", Style::default().fg(theme.text_muted))]), Line::from(vec![Span::styled("     /quit      ", Style::default().fg(theme.primary)), Span::styled("Exit application", Style::default().fg(theme.text_muted))]), Line::from(""), Line::from(vec![Span::styled("   Type your message or command below to get started...", Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC))])];
        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), chat_area);
    } else {
        let mut lines = Vec::new();
        let model_name = app.config.model.as_deref().unwrap_or("auto");
        for (idx, msg) in app.messages.iter().enumerate() {
            if idx > 0 { lines.push(Line::from("")); }
            if msg.role == "user" {
                lines.push(Line::from(vec![Span::styled(" > ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" YOU ", Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD))]));
                lines.extend(parse_markdown_to_lines(&msg.content, true, theme));
            } else if msg.role == "system" {
                lines.push(Line::from(vec![Span::styled(" > ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)), Span::styled(" TOOL RESULT ", Style::default().bg(theme.success).fg(Color::Black).add_modifier(Modifier::BOLD))]));
                lines.extend(parse_markdown_to_lines(&msg.content, false, theme));
            } else {
                lines.push(Line::from(vec![Span::styled(" > ", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)), Span::styled(" AI ", Style::default().bg(theme.secondary).fg(Color::Black).add_modifier(Modifier::BOLD)), Span::styled(format!(" [{}]", model_name), Style::default().fg(theme.text_dim))]));
                lines.extend(parse_markdown_to_lines(&msg.content, false, theme));
            }
        }
        f.render_widget(Paragraph::new(lines).block(block).scroll((app.scroll, 0)).wrap(Wrap { trim: false }), chat_area);
    }
    if let Some(tool_area) = tool_area {
        let title = format!(" {} Tool Call{} {} ", if app.tools_expanded { "▼" } else { "▶" }, if app.tool_calls.len() == 1 { "" } else { "s" }, app.tool_calls.len());
        let mut lines = vec![Line::from(Span::styled(title, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)))];
        if app.tools_expanded {
            for tc in app.tool_calls.iter().rev().take(4) {
                let args = tc.args.to_string();
                lines.push(Line::from(vec![Span::styled(format!("  {} ", tc.tool), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)), Span::styled(args, Style::default().fg(theme.text_muted))]));
            }
        }
        f.render_widget(Paragraph::new(lines).style(Style::default().fg(theme.text)).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(theme.border_inactive))), tool_area);
    }
}

fn draw_models_modal(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(65, 70, area); f.render_widget(Clear, popup_area);
    let items: Vec<ListItem> = app.models.iter().map(|m| { let active = app.config.model.as_deref() == Some(m.as_str()); let line = if active { Line::from(vec![Span::styled(format!("  {} ", m), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)), Span::styled("[OK] Active", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))]) } else { Line::from(vec![Span::styled(format!("  {}", m), Style::default().fg(theme.text))]) }; ListItem::new(line) }).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_active)).title(Span::styled(format!(" [ Select Model ({}) ] ", app.models.len()), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)))).highlight_symbol(" > ").highlight_style(Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD));
    let mut state = app.model_state.clone(); f.render_stateful_widget(list, popup_area, &mut state);
}

fn draw_providers_modal(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(55, 60, area); f.render_widget(Clear, popup_area);
    let items: Vec<ListItem> = PROVIDERS.iter().enumerate().map(|(idx, p)| { let active = idx == app.provider; let line = if active { Line::from(vec![Span::styled(format!("  {} ", p.name), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)), Span::styled("[OK] Active", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))]) } else { Line::from(vec![Span::styled(format!("  {}", p.name), Style::default().fg(theme.text))]) }; ListItem::new(line) }).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_active)).title(Span::styled(" [ Select AI Provider ] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)))).highlight_symbol(" > ").highlight_style(Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD));
    let mut state = app.provider_state.clone(); f.render_stateful_widget(list, popup_area, &mut state);
}

fn draw_setup_modal(f: &mut Frame, app: &App, area: Rect, step: u8, theme: &Theme) {
    let popup_area = centered_rect(65, 75, area); f.render_widget(Clear, popup_area);
    let step_title = match step { 0 => "Step 1 of 3: Select Provider", 1 => "Step 2 of 3: Enter API Key", 2 => "Step 3 of 3: User Profile", 3 => "Configuration Error", _ => "Setup Wizard" };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_active)).title(Span::styled(format!(" [ Term Code Setup -- {} ] ", step_title), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)));
    let inner = Layout::vertical([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)]).margin(1).split(popup_area); f.render_widget(block, popup_area);
    let steps = Line::from(vec![Span::styled(" [1] Provider ", if step == 0 { Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_muted) }), Span::styled(" --> ", Style::default().fg(theme.text_dim)), Span::styled(" [2] API Key ", if step == 1 { Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_muted) }), Span::styled(" --> ", Style::default().fg(theme.text_dim)), Span::styled(" [3] Profile ", if step == 2 { Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_muted) })]);
    f.render_widget(Paragraph::new(steps).alignment(Alignment::Center), inner[0]);
    match step {
        0 => { let items = PROVIDERS.iter().enumerate().map(|(i,p)| ListItem::new(Span::styled(format!("{}{}", if i == app.provider { " > " } else { "   " }, p.name), if i == app.provider { Style::default().fg(theme.primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text) }))).collect::<Vec<_>>(); f.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_inactive)).title(" Select Cloud Provider ")), inner[1]); }
        1 => { let masked = "*".repeat(app.api_input.chars().count()); let lines = vec![Line::from(""), Line::from(Span::styled("Please paste or type your API key for the selected provider:", Style::default().fg(theme.text))), Line::from(""), Line::from(vec![Span::styled(" Key: ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(if masked.is_empty() { "<paste key here>".to_string() } else { masked }, Style::default().fg(theme.warning)), Span::styled("_", Style::default().fg(theme.primary))])]; f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_inactive)).title(" API Key Input ")), inner[1]); }
        2 => { let lines = vec![Line::from(""), Line::from(Span::styled("What should Term Code call you in greetings?", Style::default().fg(theme.text))), Line::from(""), Line::from(vec![Span::styled(" Name: ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(if app.name_input.is_empty() { "Developer (default)".to_string() } else { app.name_input.clone() }, Style::default().fg(theme.text)), Span::styled("_", Style::default().fg(theme.primary))])]; f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.border_inactive)).title(" Profile Setup ")), inner[1]); }
        3 => { let err = app.error.as_deref().unwrap_or("Invalid Config.json file found."); let lines = vec![Line::from(""), Line::from(Span::styled("[ERR] Configuration Load Error:", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))), Line::from(""), Line::from(Span::styled(err, Style::default().fg(theme.text))), Line::from(""), Line::from(Span::styled("Please fix or remove ~/.nativestuff/Config.json and restart.", Style::default().fg(theme.text_muted)))]; f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.error)).title(" Config Error ")), inner[1]); }
        _ => {}
    }
    let help = match step { 0 => " [Up/Down] Navigate  |  [Enter] Confirm  |  [Ctrl+C] Quit ", 1 => " [Type Key]  |  [Enter] Next Step  |  [Backspace] Delete ", 2 => " [Type Name]  |  [Enter] Finish & Save  |  [Backspace] Delete ", 3 => " [Ctrl+C] Quit Application ", _ => "" };
    f.render_widget(Paragraph::new(Span::styled(help, Style::default().fg(theme.text_muted))).alignment(Alignment::Center), inner[2]);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let (border, title) = if app.busy { (Style::default().fg(theme.warning), Span::styled(" > Thinking... (Esc to stop model) ", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))) } else if app.error.is_some() { (Style::default().fg(theme.error), Span::styled(" > Input (Error) ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))) } else { (Style::default().fg(theme.border_active), Span::styled(" > Input (/ for commands, Tab to complete) ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))) };
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border).title(title);
    match app.mode {
        Mode::Setup(1) => { let content = Line::from(vec![Span::styled("API Key: ", Style::default().fg(theme.primary)), Span::styled("*".repeat(app.api_input.chars().count()), Style::default().fg(theme.warning)), Span::styled("_", Style::default().fg(theme.primary))]); f.render_widget(Paragraph::new(content).block(block), area); return; }
        Mode::Setup(2) => { let content = Line::from(vec![Span::styled("Name: ", Style::default().fg(theme.primary)), Span::styled(&app.name_input, Style::default().fg(theme.text)), Span::styled("_", Style::default().fg(theme.primary))]); f.render_widget(Paragraph::new(content).block(block), area); return; }
        Mode::Setup(3) => { f.render_widget(Paragraph::new(Line::from(Span::styled("Config error. Press Ctrl+C to exit.", Style::default().fg(theme.error)))).block(block), area); return; }
        _ => {}
    }
    let width = area.width.saturating_sub(4) as usize;
    let input_chars: Vec<char> = app.input.chars().collect();
    let cursor = app.input_cursor.min(input_chars.len());
    let start = cursor.saturating_sub(width.saturating_sub(1));
    let end = (start + width).min(input_chars.len());
    let selected = app.input_anchor.map(|a| (a.min(cursor), a.max(cursor)));
    let mut spans = vec![Span::styled("> ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))];
    for i in start..end {
        let style = if selected.is_some_and(|(a,b)| i >= a && i < b) { Style::default().bg(theme.primary).fg(Color::Black) } else { Style::default().fg(theme.text) };
        spans.push(Span::styled(input_chars[i].to_string(), style));
    }
    spans.push(Span::styled("▌", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)));
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    if let Some(err) = app.error.as_deref() { f.render_widget(Paragraph::new(Line::from(vec![Span::styled(" [ERR] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)), Span::styled(err, Style::default().fg(theme.error))])), area); return; }
    let spans = match app.mode {
        Mode::Setup(_) | Mode::Models | Mode::Providers => vec![Span::styled(" [Enter]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Select  | ", Style::default().fg(theme.text_muted)), Span::styled("[Up/Down]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Navigate  | ", Style::default().fg(theme.text_muted)), Span::styled("[Esc]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Back  | ", Style::default().fg(theme.text_muted)), Span::styled("[Ctrl+C]", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)), Span::styled(" Quit", Style::default().fg(theme.text_muted))],
        Mode::Chat => vec![Span::styled(" [Enter]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Send  | ", Style::default().fg(theme.text_muted)), Span::styled("[/]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Commands  | ", Style::default().fg(theme.text_muted)), Span::styled("[Tab]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Complete  | ", Style::default().fg(theme.text_muted)), Span::styled("[Esc]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(if app.busy { " Stop AI  | " } else { " Clear Input  | " }, Style::default().fg(theme.text_muted)), Span::styled("[Arrows/Ctrl/Shift]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Edit  | ", Style::default().fg(theme.text_muted)), Span::styled("[Up/Down/PgUp/PgDn]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" Scroll  | ", Style::default().fg(theme.text_muted)), Span::styled("[Ctrl+C]", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)), Span::styled(" Quit", Style::default().fg(theme.text_muted))],
    };
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn parse_markdown_to_lines<'a>(content: &'a str, is_user: bool, theme: &Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::new(); let mut in_code_block = false;
    for line in content.lines() {
        if is_user { lines.push(Line::from(vec![Span::raw("   "), Span::styled(line.to_owned(), Style::default().fg(theme.text))])); continue; }
        let trimmed = line.trim();
        if trimmed.starts_with("```") { if in_code_block { in_code_block = false; lines.push(Line::from(vec![Span::styled("   +--", Style::default().fg(theme.border_inactive)), Span::styled("--------------------------------------------------------", Style::default().fg(theme.border_inactive))])); } else { in_code_block = true; let lang = trimmed.trim_start_matches("```").trim(); lines.push(Line::from(vec![Span::styled("   +-- ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(if lang.is_empty() { "code" } else { lang }.to_owned(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)), Span::styled(" --------------------------------------------------", Style::default().fg(theme.border_inactive))])); } continue; }
        if in_code_block { lines.push(Line::from(vec![Span::styled("   | ", Style::default().fg(theme.primary)), Span::styled(line.to_owned(), Style::default().fg(Color::Rgb(190,215,230)).bg(theme.code_bg))])); continue; }
        if trimmed.starts_with("# ") { lines.push(Line::from(vec![Span::raw("   "), Span::styled(trimmed[2..].trim().to_owned(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))])); }
        else if trimmed.starts_with("## ") { lines.push(Line::from(vec![Span::raw("   "), Span::styled(trimmed[3..].trim().to_owned(), Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD))])); }
        else if trimmed.starts_with("### ") { lines.push(Line::from(vec![Span::raw("   "), Span::styled(trimmed[4..].trim().to_owned(), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))])); }
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") { let mut s = vec![Span::raw("   "), Span::styled("* ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))]; s.extend(parse_inline_spans(trimmed[2..].trim(), theme)); lines.push(Line::from(s)); }
        else if trimmed.starts_with("> ") { lines.push(Line::from(vec![Span::styled("   | ", Style::default().fg(theme.warning)), Span::styled(trimmed[2..].trim().to_owned(), Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC))])); }
        else { let mut s = vec![Span::raw("   ")]; s.extend(parse_inline_spans(line, theme)); lines.push(Line::from(s)); }
    }
    if lines.is_empty() { lines.push(Line::from("")); }
    lines
}

fn parse_inline_spans<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    for (i, part) in text.split('`').enumerate() {
        if part.is_empty() { continue; }
        if i % 2 == 1 { spans.push(Span::styled(format!(" {} ", part), Style::default().fg(theme.primary).bg(theme.code_bg))); }
        else { for (j, p) in part.split("**").enumerate() { if p.is_empty() { continue; } spans.push(if j % 2 == 1 { Span::styled(p.to_string(), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)) } else { Span::styled(p.to_string(), Style::default().fg(theme.text)) }); } }
    }
    if spans.is_empty() { spans.push(Span::styled(text.to_string(), Style::default().fg(theme.text))); }
    spans
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v = Layout::vertical([Constraint::Percentage((100-percent_y)/2), Constraint::Percentage(percent_y), Constraint::Percentage((100-percent_y)/2)]).split(r);
    Layout::horizontal([Constraint::Percentage((100-percent_x)/2), Constraint::Percentage(percent_x), Constraint::Percentage((100-percent_x)/2)]).split(v[1])[1]
}
