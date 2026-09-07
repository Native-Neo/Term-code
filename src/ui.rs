use crate::{App, Mode, PROVIDERS};
use ratatui::{
    prelude::*,
    widgets::*,
};
use crate::theme::Theme;

pub fn draw(f: &mut Frame, app: &App) {
    let theme = Theme::default();

    // Main layout: Header, Canvas, Input, Footer
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(f.area());

    draw_header(f, app, chunks[0], &theme);

    // Canvas background is always chat, modals render on top if active
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
    let provider_name = PROVIDERS
        .get(app.provider)
        .map(|p| p.name)
        .unwrap_or("Unknown");
    let model_name = app.config.model.as_deref().unwrap_or("auto");

    let (status_text, status_style) = if app.busy {
        (" [BUSY] Thinking... ", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))
    } else if app.error.is_some() {
        (" [ERR] Error ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))
    } else {
        (" [OK] Ready ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
    };

    let header_chunks = Layout::horizontal([
        Constraint::Length(18),
        Constraint::Min(20),
        Constraint::Length(22),
    ])
    .split(area);

    // Left: Brand Badge
    let brand = Paragraph::new(Line::from(vec![
        Span::styled(" # ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("TERM CODE", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.primary)),
    );
    f.render_widget(brand, header_chunks[0]);

    // Center: Active Provider, Model & Workspace Folder
    let folder_name = app.cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| app.cwd.display().to_string());
    let info = Paragraph::new(Line::from(vec![
        Span::styled(" Provider: ", Style::default().fg(theme.text_muted)),
        Span::styled(provider_name, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled(" | Model: ", Style::default().fg(theme.text_muted)),
        Span::styled(model_name, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
        Span::styled(" | Dir: ", Style::default().fg(theme.text_muted)),
        Span::styled(folder_name, Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_inactive)),
    );
    f.render_widget(info, header_chunks[1]);

    // Right: Status Badge
    let status_widget = Paragraph::new(Line::from(vec![
        Span::styled(status_text, status_style),
    ]))
    .alignment(Alignment::Right)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if app.busy {
                Style::default().fg(theme.warning)
            } else if app.error.is_some() {
                Style::default().fg(theme.error)
            } else {
                Style::default().fg(theme.border_inactive)
            }),
    );
    f.render_widget(status_widget, header_chunks[2]);
}

fn draw_chat(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let scroll_info = if app.scroll > 0 {
        format!(" (Scroll: +{} lines up) ", app.scroll)
    } else {
        "".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_inactive))
        .title(Span::styled(
            format!(" # Conversation{} ", scroll_info),
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        ));

    if app.messages.is_empty() {
        let provider_name = PROVIDERS
            .get(app.provider)
            .map(|p| p.name)
            .unwrap_or("Unknown");
        let model_name = app.config.model.as_deref().unwrap_or("auto");

        let greeting_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   # ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled("WELCOME TO TERM CODE (AGENTIC EDITION)", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("     Agentic Cloud AI Terminal Interface", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Workspace Dir: ", Style::default().fg(theme.text_muted)),
                Span::styled(app.cwd.display().to_string(), Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("   Connected to:  ", Style::default().fg(theme.text_muted)),
                Span::styled(provider_name, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled("  |  Model: ", Style::default().fg(theme.text_muted)),
                Span::styled(model_name, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(format!("   Greeting:      {}", app.greeting), Style::default().fg(theme.text_muted)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Agentic Capabilities Enabled:", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("     Tools:     ", Style::default().fg(theme.primary)),
                Span::styled("list_dir, read_file, write_file, run_cmd, search, spawn_subagent", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Quick Commands:", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("     /model     ", Style::default().fg(theme.primary)),
                Span::styled("Select AI model", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("     /provider  ", Style::default().fg(theme.primary)),
                Span::styled("Switch provider (OpenAI, Anthropic, Google...)", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("     /config    ", Style::default().fg(theme.primary)),
                Span::styled("View current active configuration", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("     /new       ", Style::default().fg(theme.primary)),
                Span::styled("Clear chat history", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("     /quit      ", Style::default().fg(theme.primary)),
                Span::styled("Exit application", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Type your message or command below to get started...", Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
            ]),
        ];

        let p = Paragraph::new(greeting_lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let model_name = app.config.model.as_deref().unwrap_or("auto");

    for (idx, msg) in app.messages.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::from(""));
        }

        if msg.role == "user" {
            lines.push(Line::from(vec![
                Span::styled(" > ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled(" YOU ", Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD)),
            ]));
            let msg_lines = parse_markdown_to_lines(&msg.content, true, theme);
            lines.extend(msg_lines);
        } else if msg.role == "system" {
            lines.push(Line::from(vec![
                Span::styled(" > ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
                Span::styled(" TOOL RESULT ", Style::default().bg(theme.success).fg(Color::Black).add_modifier(Modifier::BOLD)),
            ]));
            let msg_lines = parse_markdown_to_lines(&msg.content, false, theme);
            lines.extend(msg_lines);
        } else {
            lines.push(Line::from(vec![
                Span::styled(" > ", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
                Span::styled(" AI ", Style::default().bg(theme.secondary).fg(Color::Black).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" [{}]", model_name), Style::default().fg(theme.text_dim)),
            ]));
            let msg_lines = parse_markdown_to_lines(&msg.content, false, theme);
            lines.extend(msg_lines);
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_models_modal(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(65, 70, area);
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .models
        .iter()
        .map(|m| {
            let is_active = app.config.model.as_deref() == Some(m.as_str());
            let line = if is_active {
                Line::from(vec![
                    Span::styled(format!("  {} ", m), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
                    Span::styled("[OK] Active", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!("  {}", m), Style::default().fg(theme.text)),
                ])
            };
            ListItem::new(line)
        })
        .collect();

    let title = format!(" [ Select Model ({}) ] ", app.models.len());
    let list_widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_active))
                .title(Span::styled(title, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))),
        )
        .highlight_symbol(" > ")
        .highlight_style(
            Style::default()
                .bg(theme.primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = app.model_state.clone();
    f.render_stateful_widget(list_widget, popup_area, &mut state);
}

fn draw_providers_modal(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let popup_area = centered_rect(55, 60, area);
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = PROVIDERS
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let is_active = idx == app.provider;
            let line = if is_active {
                Line::from(vec![
                    Span::styled(format!("  {} ", p.name), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
                    Span::styled("[OK] Active", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!("  {}", p.name), Style::default().fg(theme.text)),
                ])
            };
            ListItem::new(line)
        })
        .collect();

    let list_widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_active))
                .title(Span::styled(" [ Select AI Provider ] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))),
        )
        .highlight_symbol(" > ")
        .highlight_style(
            Style::default()
                .bg(theme.primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = app.provider_state.clone();
    f.render_stateful_widget(list_widget, popup_area, &mut state);
}

fn draw_setup_modal(f: &mut Frame, app: &App, area: Rect, step: u8, theme: &Theme) {
    let popup_area = centered_rect(65, 75, area);
    f.render_widget(Clear, popup_area);

    let step_title = match step {
        0 => "Step 1 of 3: Select Provider",
        1 => "Step 2 of 3: Enter API Key",
        2 => "Step 3 of 3: User Profile",
        3 => "Configuration Error",
        _ => "Setup Wizard",
    };

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_active))
        .title(Span::styled(
            format!(" [ Term Code Setup -- {} ] ", step_title),
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        ));

    let inner_layout = Layout::vertical([
        Constraint::Length(3), // Progress bar
        Constraint::Min(5),    // Step content
        Constraint::Length(2), // Help line
    ])
    .margin(1)
    .split(popup_area);

    // Render outer block frame
    f.render_widget(main_block, popup_area);

    // Render progress bar
    let step_spans = vec![
        Span::styled(" [1] Provider ", if step == 0 { Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_muted) }),
        Span::styled(" --> ", Style::default().fg(theme.text_dim)),
        Span::styled(" [2] API Key ", if step == 1 { Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_muted) }),
        Span::styled(" --> ", Style::default().fg(theme.text_dim)),
        Span::styled(" [3] Profile ", if step == 2 { Style::default().bg(theme.primary).fg(Color::Black).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.text_muted) }),
    ];
    let step_bar = Paragraph::new(Line::from(step_spans)).alignment(Alignment::Center);
    f.render_widget(step_bar, inner_layout[0]);

    match step {
        0 => {
            let items: Vec<ListItem> = PROVIDERS
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let is_sel = i == app.provider;
                    let style = if is_sel {
                        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text)
                    };
                    let prefix = if is_sel { " > " } else { "   " };
                    ListItem::new(Span::styled(format!("{}{}", prefix, p.name), style))
                })
                .collect();
            let p_list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_inactive))
                    .title(" Select Cloud Provider "),
            );
            f.render_widget(p_list, inner_layout[1]);
        }
        1 => {
            let masked = "*".repeat(app.api_input.chars().count());
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled("Please paste or type your API key for the selected provider:", Style::default().fg(theme.text))),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" Key: ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(if masked.is_empty() { "<paste key here>".to_string() } else { masked }, Style::default().fg(theme.warning)),
                    Span::styled("_", Style::default().fg(theme.primary)),
                ]),
            ];
            let p_key = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_inactive))
                    .title(" API Key Input "),
            );
            f.render_widget(p_key, inner_layout[1]);
        }
        2 => {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled("What should Term Code call you in greetings?", Style::default().fg(theme.text))),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" Name: ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(if app.name_input.is_empty() { "Developer (default)".to_string() } else { app.name_input.clone() }, Style::default().fg(theme.text)),
                    Span::styled("_", Style::default().fg(theme.primary)),
                ]),
            ];
            let p_name = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_inactive))
                    .title(" Profile Setup "),
            );
            f.render_widget(p_name, inner_layout[1]);
        }
        3 => {
            let err_msg = app.error.as_deref().unwrap_or("Invalid Config.json file found.");
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled("[ERR] Configuration Load Error:", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(Span::styled(err_msg, Style::default().fg(theme.text))),
                Line::from(""),
                Line::from(Span::styled("Please fix or remove ~/.nativestuff/Config.json and restart.", Style::default().fg(theme.text_muted))),
            ];
            let p_err = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.error))
                    .title(" Config Error "),
            );
            f.render_widget(p_err, inner_layout[1]);
        }
        _ => {}
    }

    let help_text = match step {
        0 => " [Up/Down] Navigate  |  [Enter] Confirm  |  [Ctrl+C] Quit ",
        1 => " [Type Key]  |  [Enter] Next Step  |  [Backspace] Delete ",
        2 => " [Type Name]  |  [Enter] Finish & Save  |  [Backspace] Delete ",
        3 => " [Ctrl+C] Quit Application ",
        _ => "",
    };
    let help_widget = Paragraph::new(Span::styled(help_text, Style::default().fg(theme.text_muted)))
        .alignment(Alignment::Center);
    f.render_widget(help_widget, inner_layout[2]);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let (border_style, title) = if app.busy {
        (
            Style::default().fg(theme.warning),
            Span::styled(" > Thinking... (Esc to stop model) ", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
        )
    } else if app.error.is_some() {
        (
            Style::default().fg(theme.error),
            Span::styled(" > Input (Error) ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
        )
    } else {
        (
            Style::default().fg(theme.border_active),
            Span::styled(" > Input (/ for commands, Tab to complete) ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title);

    let content = match app.mode {
        Mode::Setup(1) => Line::from(vec![
            Span::styled("API Key: ", Style::default().fg(theme.primary)),
            Span::styled("*".repeat(app.api_input.chars().count()), Style::default().fg(theme.warning)),
            Span::styled("_", Style::default().fg(theme.primary)),
        ]),
        Mode::Setup(2) => Line::from(vec![
            Span::styled("Name: ", Style::default().fg(theme.primary)),
            Span::styled(&app.name_input, Style::default().fg(theme.text)),
            Span::styled("_", Style::default().fg(theme.primary)),
        ]),
        Mode::Setup(3) => Line::from(Span::styled("Config error. Press Ctrl+C to exit.", Style::default().fg(theme.error))),
        _ => {
            if app.input.starts_with('/') {
                let cmd = app.input.trim().to_lowercase();
                let hint = match cmd.as_str() {
                    "/m" | "/model" => " -> Open model selector",
                    "/p" | "/provider" => " -> Switch cloud AI provider",
                    "/c" | "/config" => " -> Show active settings info",
                    "/n" | "/new" | "/clear" => " -> Clear message history",
                    "/q" | "/quit" | "/exit" => " -> Exit Term Code",
                    _ => " -> [Tab] Complete command",
                };
                Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(&app.input, Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
                    Span::styled("_", Style::default().fg(theme.primary)),
                    Span::styled(hint, Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(&app.input, Style::default().fg(theme.text)),
                    Span::styled("_", Style::default().fg(theme.primary)),
                ])
            }
        }
    };

    let p = Paragraph::new(content).block(block);
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let error_text = app.error.as_deref();

    let spans = match app.mode {
        Mode::Setup(_) | Mode::Models | Mode::Providers => vec![
            Span::styled(" [Enter]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" Select  | ", Style::default().fg(theme.text_muted)),
            Span::styled("[Up/Down]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" Navigate  | ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" Back  | ", Style::default().fg(theme.text_muted)),
            Span::styled("[Ctrl+C]", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit", Style::default().fg(theme.text_muted)),
        ],
        Mode::Chat => {
            if let Some(err) = error_text {
                vec![
                    Span::styled(" [ERR] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
                    Span::styled(err, Style::default().fg(theme.error)),
                ]
            } else {
                vec![
                    Span::styled(" [Enter]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(" Send  | ", Style::default().fg(theme.text_muted)),
                    Span::styled("[/]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(" Commands  | ", Style::default().fg(theme.text_muted)),
                    Span::styled("[Tab]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(" Complete  | ", Style::default().fg(theme.text_muted)),
                    Span::styled("[Esc]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(if app.busy { " Stop AI  | " } else { " Clear Input  | " }, Style::default().fg(theme.text_muted)),
                    Span::styled("[Up/Down/PgUp/PgDn]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(" Scroll  | ", Style::default().fg(theme.text_muted)),
                    Span::styled("[Ctrl+C]", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
                    Span::styled(" Quit", Style::default().fg(theme.text_muted)),
                ]
            }
        }
    };

    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}

fn parse_markdown_to_lines<'a>(content: &'a str, is_user: bool, theme: &Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for line in content.lines() {
        if is_user {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(line.to_owned(), Style::default().fg(theme.text)),
            ]));
            continue;
        }

        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                in_code_block = false;
                lines.push(Line::from(vec![
                    Span::styled("   +--", Style::default().fg(theme.border_inactive)),
                    Span::styled("--------------------------------------------------------", Style::default().fg(theme.border_inactive)),
                ]));
            } else {
                in_code_block = true;
                let code_lang = trimmed.trim_start_matches("```").trim();
                let lang_display = if code_lang.is_empty() {
                    "code"
                } else {
                    code_lang
                };
                lines.push(Line::from(vec![
                    Span::styled("   +-- ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(lang_display.to_owned(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(" --------------------------------------------------", Style::default().fg(theme.border_inactive)),
                ]));
            }
            continue;
        }

        if in_code_block {
            lines.push(Line::from(vec![
                Span::styled("   | ", Style::default().fg(theme.primary)),
                Span::styled(line.to_owned(), Style::default().fg(Color::Rgb(190, 215, 230)).bg(theme.code_bg)),
            ]));
            continue;
        }

        if trimmed.starts_with("# ") {
            let title = trimmed.trim_start_matches("# ").trim();
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(title.to_owned(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            ]));
        } else if trimmed.starts_with("## ") {
            let title = trimmed.trim_start_matches("## ").trim();
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(title.to_owned(), Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
            ]));
        } else if trimmed.starts_with("### ") {
            let title = trimmed.trim_start_matches("### ").trim();
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(title.to_owned(), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            ]));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = trimmed[2..].trim();
            let spans = parse_inline_spans(item, theme);
            let mut line_spans = vec![
                Span::raw("   "),
                Span::styled("* ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            ];
            line_spans.extend(spans);
            lines.push(Line::from(line_spans));
        } else if trimmed.starts_with("> ") {
            let quote = trimmed[2..].trim();
            lines.push(Line::from(vec![
                Span::styled("   | ", Style::default().fg(theme.warning)),
                Span::styled(quote.to_owned(), Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC)),
            ]));
        } else {
            let spans = parse_inline_spans(line, theme);
            let mut line_spans = vec![Span::raw("   ")];
            line_spans.extend(spans);
            lines.push(Line::from(line_spans));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

fn parse_inline_spans<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let parts: Vec<&str> = text.split('`').collect();

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            // Inside inline code `...`
            spans.push(Span::styled(
                format!(" {} ", part),
                Style::default().fg(theme.primary).bg(theme.code_bg),
            ));
        } else {
            // Check for bold **...**
            let bold_parts: Vec<&str> = part.split("**").collect();
            for (j, bpart) in bold_parts.iter().enumerate() {
                if bpart.is_empty() {
                    continue;
                }
                if j % 2 == 1 {
                    spans.push(Span::styled(
                        bpart.to_string(),
                        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::styled(
                        bpart.to_string(),
                        Style::default().fg(theme.text),
                    ));
                }
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), Style::default().fg(theme.text)));
    }

    spans
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use crate::{Config, Msg};

    #[test]
    fn test_render_chat_welcome() {
        let backend = TestBackend::new(100, 25);
        let mut terminal = Terminal::new(backend).unwrap();

        let config = Config {
            provider: "anthropic".to_string(),
            api_key: "dummy".to_string(),
            model: Some("claude-3-7-sonnet".to_string()),
            user_name: Some("Yash".to_string()),
        };
        let mut app = App::new(config, false, None);
        app.mode = Mode::Chat;

        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        println!("\n=== RENDER OUTPUT: CHAT WELCOME ===");
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
                line.push_str(cell.symbol());
            }
            println!("{}", line);
        }
    }

    #[test]
    fn test_render_chat_messages() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let config = Config {
            provider: "anthropic".to_string(),
            api_key: "dummy".to_string(),
            model: Some("claude-3-7-sonnet".to_string()),
            user_name: Some("Yash".to_string()),
        };
        let mut app = App::new(config, false, None);
        app.mode = Mode::Chat;
        app.messages.push(Msg {
            role: "user".to_string(),
            content: "Write a hello world function in Rust".to_string(),
        });
        app.messages.push(Msg {
            role: "assistant".to_string(),
            content: "Here is how you write hello world in Rust:\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```".to_string(),
        });

        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        println!("\n=== RENDER OUTPUT: CHAT MESSAGES & CODE BLOCK ===");
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
                line.push_str(cell.symbol());
            }
            println!("{}", line);
        }
    }

    #[test]
    fn test_render_models_modal() {
        let backend = TestBackend::new(100, 25);
        let mut terminal = Terminal::new(backend).unwrap();

        let config = Config {
            provider: "openai".to_string(),
            api_key: "dummy".to_string(),
            model: Some("gpt-4o".to_string()),
            user_name: Some("Yash".to_string()),
        };
        let mut app = App::new(config, false, None);
        app.mode = Mode::Models;
        app.models = vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string(), "o1-mini".to_string()];
        app.model_state.select(Some(0));

        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        println!("\n=== RENDER OUTPUT: MODELS MODAL ===");
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
                line.push_str(cell.symbol());
            }
            println!("{}", line);
        }
    }

    #[test]
    fn test_app_init_existing_config() {
        let config = Config {
            provider: "anthropic".to_string(),
            api_key: "test-key-123".to_string(),
            model: Some("claude-3-7-sonnet".to_string()),
            user_name: Some("Developer".to_string()),
        };
        let app = App::new(config, false, None);

        assert!(app.mode == Mode::Chat, "App should start in Mode::Chat when config is loaded");
        assert_eq!(app.status, "Ready");
        assert!(app.error.is_none());
    }
}
