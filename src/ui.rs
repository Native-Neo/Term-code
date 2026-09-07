use crate::theme::Theme;
use crate::{App, Mode, PROVIDERS};
use ratatui::{prelude::*, widgets::*};

pub fn draw(f: &mut Frame, app: &App) {
    let theme = Theme::default();
    let c = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(f.area());
    draw_header(f, app, c[0], &theme);
    draw_chat(f, app, c[1], &theme);
    match app.mode {
        Mode::Models => draw_models_modal(f, app, c[1], &theme),
        Mode::Providers => draw_providers_modal(f, app, c[1], &theme),
        Mode::Setup(s) => draw_setup_modal(f, app, c[1], s, &theme),
        Mode::Chat => {}
    }
    draw_input(f, app, c[2], &theme);
    draw_footer(f, app, c[3], &theme);
}
fn draw_header(f: &mut Frame, a: &App, r: Rect, t: &Theme) {
    let p = PROVIDERS
        .get(a.provider)
        .map(|x| x.name)
        .unwrap_or("Unknown");
    let m = a.config.model.as_deref().unwrap_or("auto");
    let (st, ss) = if a.busy {
        (
            " [BUSY] Thinking... ",
            Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
        )
    } else if a.error.is_some() {
        (
            " [ERR] Error ",
            Style::default().fg(t.error).add_modifier(Modifier::BOLD),
        )
    } else {
        (
            " [OK] Ready ",
            Style::default().fg(t.success).add_modifier(Modifier::BOLD),
        )
    };
    let x = Layout::horizontal([
        Constraint::Length(18),
        Constraint::Min(20),
        Constraint::Length(22),
    ])
    .split(r);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " # ",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "TERM CODE",
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.primary)),
        ),
        x[0],
    );
    let d = a
        .cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| a.cwd.display().to_string());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Provider: ", Style::default().fg(t.text_muted)),
            Span::styled(
                p,
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Model: ", Style::default().fg(t.text_muted)),
            Span::styled(
                m,
                Style::default()
                    .fg(t.secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Dir: ", Style::default().fg(t.text_muted)),
            Span::styled(
                d,
                Style::default().fg(t.success).add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.border_inactive)),
        ),
        x[1],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(st, ss)]))
            .alignment(Alignment::Right)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(if a.busy {
                        Style::default().fg(t.warning)
                    } else if a.error.is_some() {
                        Style::default().fg(t.error)
                    } else {
                        Style::default().fg(t.border_inactive)
                    }),
            ),
        x[2],
    )
}
fn draw_chat(f: &mut Frame, a: &App, r: Rect, t: &Theme) {
    let th = if a.tool_calls.is_empty() {
        0
    } else if a.tools_expanded {
        a.tool_calls.len().min(4) as u16 + 1
    } else {
        1
    };
    let parts = if th > 0 {
        let x = Layout::vertical([Constraint::Min(1), Constraint::Length(th)]).split(r);
        (x[0], Some(x[1]))
    } else {
        (r, None)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_inactive))
        .title(Span::styled(
            format!(
                " # Conversation{} ",
                if a.scroll > 0 {
                    format!(" (Scroll: +{} lines up) ", a.scroll)
                } else {
                    String::new()
                }
            ),
            Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
        ));
    if a.messages.is_empty() {
        let p = PROVIDERS
            .get(a.provider)
            .map(|x| x.name)
            .unwrap_or("Unknown");
        let m = a.config.model.as_deref().unwrap_or("auto");
        let l = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "   # ",
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "WELCOME TO TERM CODE (AGENTIC EDITION)",
                    Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![Span::styled(
                "     Agentic Cloud AI Terminal Interface",
                Style::default().fg(t.text_muted),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Workspace Dir: ", Style::default().fg(t.text_muted)),
                Span::styled(
                    a.cwd.display().to_string(),
                    Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("   Connected to:  ", Style::default().fg(t.text_muted)),
                Span::styled(
                    p,
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  |  Model: ", Style::default().fg(t.text_muted)),
                Span::styled(
                    m,
                    Style::default()
                        .fg(t.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!("   Greeting:      {}", a.greeting),
                Style::default().fg(t.text_muted),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "   Agentic Capabilities Enabled:",
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("     Tools:     ", Style::default().fg(t.primary)),
                Span::styled(
                    "list_dir, read_file, write_file, run_cmd, search, spawn_subagent",
                    Style::default().fg(t.text_muted),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "   Quick Commands:",
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("     /model     ", Style::default().fg(t.primary)),
                Span::styled("Select AI model", Style::default().fg(t.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("     /provider  ", Style::default().fg(t.primary)),
                Span::styled(
                    "Switch provider (OpenAI, Anthropic, Google...)",
                    Style::default().fg(t.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("     /config    ", Style::default().fg(t.primary)),
                Span::styled(
                    "View current active configuration",
                    Style::default().fg(t.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("     /new       ", Style::default().fg(t.primary)),
                Span::styled("Clear chat history", Style::default().fg(t.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("     /quit      ", Style::default().fg(t.primary)),
                Span::styled("Exit application", Style::default().fg(t.text_muted)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "   Type your message or command below to get started...",
                Style::default()
                    .fg(t.text_dim)
                    .add_modifier(Modifier::ITALIC),
            )]),
        ];
        f.render_widget(
            Paragraph::new(l).block(block).wrap(Wrap { trim: false }),
            parts.0,
        )
    } else {
        let mut l = Vec::new();
        let m = a.config.model.as_deref().unwrap_or("auto");
        for (i, msg) in a.messages.iter().enumerate() {
            if i > 0 {
                l.push(Line::from(""))
            }
            if msg.role == "user" {
                l.push(Line::from(vec![
                    Span::styled(
                        " > ",
                        Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " YOU ",
                        Style::default()
                            .bg(t.primary)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                l.extend(parse_markdown_to_lines(&msg.content, true, t))
            } else if msg.role == "system" {
                l.push(Line::from(vec![
                    Span::styled(
                        " > ",
                        Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " TOOL RESULT ",
                        Style::default()
                            .bg(t.success)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                l.extend(parse_markdown_to_lines(&msg.content, false, t))
            } else {
                l.push(Line::from(vec![
                    Span::styled(
                        " > ",
                        Style::default()
                            .fg(t.secondary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " AI ",
                        Style::default()
                            .bg(t.secondary)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" [{}]", m), Style::default().fg(t.text_dim)),
                ]));
                if crate::tools::parse_tool_call(&msg.content).is_some() {
                    let mut content = msg.content.clone();
                    if let Some(start) = content.find("```json").or_else(|| content.find("```")) {
                        if let Some(end) = content[start + 3..].find("```") {
                            content.replace_range(start..start + 3 + end + 3, "")
                        }
                    }
                    l.extend(
                        parse_markdown_to_lines(&content, false, t)
                            .into_iter()
                            .map(|line| line.clone()),
                    )
                } else {
                    l.extend(
                        parse_markdown_to_lines(&msg.content, false, t)
                            .into_iter()
                            .map(|line| line.clone()),
                    )
                }
            }
        }
        let width = parts.0.width.saturating_sub(2).max(1) as usize;
        let height = parts.0.height.saturating_sub(2) as usize;
        let visual_lines = l
            .iter()
            .map(|line| ((line.width().max(1) + width - 1) / width).max(1))
            .sum::<usize>();
        let max_scroll = visual_lines.saturating_sub(height);
        let scroll = max_scroll
            .saturating_sub(a.scroll as usize)
            .min(u16::MAX as usize) as u16;
        f.render_widget(
            Paragraph::new(l)
                .block(block)
                .scroll((scroll, 0))
                .wrap(Wrap { trim: false }),
            parts.0,
        )
    }
    if let Some(x) = parts.1 {
        a.tool_click_y.set(x.y);
        let title = format!(
            " {} Tool Call{} {} ",
            if a.tools_expanded { "▼" } else { "▶" },
            if a.tool_calls.len() == 1 { "" } else { "s" },
            a.tool_calls.len()
        );
        let mut l = vec![Line::from(Span::styled(
            title,
            Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
        ))];
        if a.tools_expanded {
            for tc in a.tool_calls.iter().rev().take(4) {
                l.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ", tc.tool),
                        Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(tc.args.to_string(), Style::default().fg(t.text_muted)),
                ]))
            }
        }
        f.render_widget(
            Paragraph::new(l).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(t.border_inactive)),
            ),
            x,
        )
    }
}
fn draw_models_modal(f: &mut Frame, a: &App, r: Rect, t: &Theme) {
    let x = centered_rect(65, 70, r);
    f.render_widget(Clear, x);
    let i = a
        .models
        .iter()
        .map(|m| {
            let q = a.config.model.as_deref() == Some(m.as_str());
            ListItem::new(if q {
                Line::from(vec![
                    Span::styled(
                        format!("  {} ", m),
                        Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "[OK] Active",
                        Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(Span::styled(
                    format!("  {}", m),
                    Style::default().fg(t.text),
                ))
            })
        })
        .collect::<Vec<_>>();
    let w = List::new(i)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.border_active))
                .title(Span::styled(
                    format!(" [ Select Model ({}) ] ", a.models.len()),
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_symbol(" > ")
        .highlight_style(
            Style::default()
                .bg(t.primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    let mut s = a.model_state.clone();
    f.render_stateful_widget(w, x, &mut s)
}
fn draw_providers_modal(f: &mut Frame, a: &App, r: Rect, t: &Theme) {
    let x = centered_rect(55, 60, r);
    f.render_widget(Clear, x);
    let i = PROVIDERS
        .iter()
        .enumerate()
        .map(|(n, p)| {
            let q = n == a.provider;
            ListItem::new(if q {
                Line::from(vec![
                    Span::styled(
                        format!("  {} ", p.name),
                        Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "[OK] Active",
                        Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(Span::styled(
                    format!("  {}", p.name),
                    Style::default().fg(t.text),
                ))
            })
        })
        .collect::<Vec<_>>();
    let w = List::new(i)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.border_active))
                .title(Span::styled(
                    " [ Select AI Provider ] ",
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_symbol(" > ")
        .highlight_style(
            Style::default()
                .bg(t.primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    let mut s = a.provider_state.clone();
    f.render_stateful_widget(w, x, &mut s)
}
fn draw_setup_modal(f: &mut Frame, a: &App, r: Rect, s: u8, t: &Theme) {
    let x = centered_rect(65, 75, r);
    f.render_widget(Clear, x);
    let title = match s {
        0 => "Step 1 of 3: Select Provider",
        1 => "Step 2 of 3: Enter API Key",
        2 => "Step 3 of 3: User Profile",
        3 => "Configuration Error",
        _ => "Setup Wizard",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_active))
        .title(Span::styled(
            format!(" [ Term Code Setup -- {} ] ", title),
            Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(block, x);
    let inner = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .margin(1)
    .split(x);
    let steps = Line::from(vec![
        Span::styled(
            " [1] Provider ",
            Style::default().fg(if s == 0 { t.primary } else { t.text_muted }),
        ),
        Span::raw(" --> "),
        Span::styled(
            " [2] API Key ",
            Style::default().fg(if s == 1 { t.primary } else { t.text_muted }),
        ),
        Span::raw(" --> "),
        Span::styled(
            " [3] Profile ",
            Style::default().fg(if s == 2 { t.primary } else { t.text_muted }),
        ),
    ]);
    f.render_widget(Paragraph::new(steps).alignment(Alignment::Center), inner[0]);
    match s {
        0 => {
            let items = PROVIDERS
                .iter()
                .enumerate()
                .map(|(n, p)| {
                    ListItem::new(Span::styled(
                        format!("{}{}", if n == a.provider { " > " } else { "   " }, p.name),
                        Style::default().fg(if n == a.provider { t.primary } else { t.text }),
                    ))
                })
                .collect::<Vec<_>>();
            f.render_widget(
                List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Select Cloud Provider "),
                ),
                inner[1],
            )
        }
        1 => {
            let key = if a.api_input.is_empty() {
                "<paste key here>".to_string()
            } else {
                "*".repeat(a.api_input.chars().count())
            };
            let lines = vec![
                Line::from(""),
                Line::from("Please paste or type your API key for the selected provider:"),
                Line::from(""),
                Line::from(vec![
                    Span::raw(" Key: "),
                    Span::styled(key, Style::default().fg(t.warning)),
                    Span::raw("_ "),
                ]),
            ];
            f.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" API Key Input "),
                ),
                inner[1],
            )
        }
        2 => {
            let name = if a.name_input.is_empty() {
                "Developer (default)".to_string()
            } else {
                a.name_input.clone()
            };
            let lines = vec![
                Line::from(""),
                Line::from("What should Term Code call you in greetings?"),
                Line::from(""),
                Line::from(vec![
                    Span::raw(" Name: "),
                    Span::styled(name, Style::default().fg(t.text)),
                    Span::raw("_ "),
                ]),
            ];
            f.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Profile Setup "),
                ),
                inner[1],
            )
        }
        3 => {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "[ERR] Configuration Load Error:",
                    Style::default().fg(t.error).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(
                    a.error
                        .as_deref()
                        .unwrap_or("Invalid Config.json file found."),
                ),
                Line::from(""),
                Line::from("Please fix or remove ~/.nativestuff/Config.json and restart."),
            ];
            f.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Config Error "),
                ),
                inner[1],
            )
        }
        _ => {}
    }
    let h = match s {
        0 => " [Up/Down] Navigate  |  [Enter] Confirm  |  [Ctrl+C] Quit ",
        1 => " [Type Key]  |  [Enter] Next Step  |  [Backspace] Delete ",
        2 => " [Type Name]  |  [Enter] Finish & Save  |  [Backspace] Delete ",
        3 => " [Ctrl+C] Quit Application ",
        _ => "",
    };
    f.render_widget(Paragraph::new(h).alignment(Alignment::Center), inner[2])
}
fn draw_input(f: &mut Frame, a: &App, r: Rect, t: &Theme) {
    let (bs, title) = if a.busy {
        (
            Style::default().fg(t.warning),
            Span::styled(
                " > Thinking... (Esc to stop model) ",
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            ),
        )
    } else if a.error.is_some() {
        (
            Style::default().fg(t.error),
            Span::styled(
                " > Input (Error) ",
                Style::default().fg(t.error).add_modifier(Modifier::BOLD),
            ),
        )
    } else {
        (
            Style::default().fg(t.border_active),
            Span::styled(
                " > Input (/ for commands, Tab to complete) ",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
        )
    };
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(bs)
        .title(title);
    match a.mode {
        Mode::Setup(1) => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("API Key: ", Style::default().fg(t.primary)),
                    Span::styled(
                        "*".repeat(a.api_input.chars().count()),
                        Style::default().fg(t.warning),
                    ),
                    Span::styled("_", Style::default().fg(t.primary)),
                ]))
                .block(b),
                r,
            );
            return;
        }
        Mode::Setup(2) => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(t.primary)),
                    Span::styled(&a.name_input, Style::default().fg(t.text)),
                    Span::styled("_", Style::default().fg(t.primary)),
                ]))
                .block(b),
                r,
            );
            return;
        }
        Mode::Setup(3) => {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Config error. Press Ctrl+C to exit.",
                    Style::default().fg(t.error),
                )))
                .block(b),
                r,
            );
            return;
        }
        _ => {}
    }
    let w = r.width.saturating_sub(4) as usize;
    let c = a.input.chars().collect::<Vec<_>>();
    let cur = a.input_cursor.min(c.len());
    let start = cur.saturating_sub(w.saturating_sub(1));
    let end = (start + w).min(c.len());
    let sel = a.input_anchor.map(|x| (x.min(cur), x.max(cur)));
    let mut s = vec![Span::styled(
        "> ",
        Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
    )];
    for i in start..end {
        let st = if sel.is_some_and(|(x, y)| i >= x && i < y) {
            Style::default().bg(t.primary).fg(Color::Black)
        } else {
            Style::default().fg(t.text)
        };
        s.push(Span::styled(c[i].to_string(), st))
    }
    s.push(Span::styled(
        "▌",
        Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(Line::from(s)).block(b), r)
}
fn draw_footer(f: &mut Frame, a: &App, r: Rect, t: &Theme) {
    if let Some(e) = a.error.as_deref() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " [ERR] ",
                    Style::default().fg(t.error).add_modifier(Modifier::BOLD),
                ),
                Span::styled(e, Style::default().fg(t.error)),
            ])),
            r,
        );
        return;
    }
    let s = match a.mode {
        Mode::Setup(_) | Mode::Models | Mode::Providers => vec![
            Span::styled(
                " [Enter]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Select  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Up/Down]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Navigate  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Esc]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Back  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Ctrl+C]",
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Quit", Style::default().fg(t.text_muted)),
        ],
        Mode::Chat => vec![
            Span::styled(
                " [Enter]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Send  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[/]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Commands  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Tab]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Complete  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Arrows/Ctrl/Shift]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Edit  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Up/Down/PgUp/PgDn]",
                Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Scroll  | ", Style::default().fg(t.text_muted)),
            Span::styled(
                "[Ctrl+C]",
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Quit", Style::default().fg(t.text_muted)),
        ],
    };
    f.render_widget(Paragraph::new(Line::from(s)), r)
}
fn parse_markdown_to_lines<'a>(content: &'a str, is_user: bool, t: &Theme) -> Vec<Line<'a>> {
    let mut l = Vec::new();
    let mut code = false;
    for line in content.lines() {
        if is_user {
            l.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(line.to_owned(), Style::default().fg(t.text)),
            ]));
            continue;
        }
        let q = line.trim();
        if q.starts_with("```") {
            if code {
                code = false;
                l.push(Line::from(vec![
                    Span::styled("   +--", Style::default().fg(t.border_inactive)),
                    Span::styled(
                        "--------------------------------------------------------",
                        Style::default().fg(t.border_inactive),
                    ),
                ]))
            } else {
                code = true;
                let lang = q.trim_start_matches("```").trim();
                l.push(Line::from(vec![
                    Span::styled(
                        "   +-- ",
                        Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        if lang.is_empty() { "code" } else { lang }.to_owned(),
                        Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " --------------------------------------------------",
                        Style::default().fg(t.border_inactive),
                    ),
                ]))
            }
            continue;
        }
        if code {
            l.push(Line::from(vec![
                Span::styled("   | ", Style::default().fg(t.primary)),
                Span::styled(
                    line.to_owned(),
                    Style::default().fg(Color::Rgb(190, 215, 230)).bg(t.code_bg),
                ),
            ]));
            continue;
        }
        if q.starts_with("# ") {
            l.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    q[2..].trim().to_owned(),
                    Style::default()
                        .fg(t.primary)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ]))
        } else if q.starts_with("## ") {
            l.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    q[3..].trim().to_owned(),
                    Style::default()
                        .fg(t.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        } else if q.starts_with("### ") {
            l.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    q[4..].trim().to_owned(),
                    Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
                ),
            ]))
        } else if q.starts_with("- ") || q.starts_with("* ") {
            let mut s = vec![
                Span::raw("   "),
                Span::styled(
                    "* ",
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD),
                ),
            ];
            s.extend(parse_inline_spans(q[2..].trim(), t));
            l.push(Line::from(s))
        } else if q.starts_with("> ") {
            l.push(Line::from(vec![
                Span::styled("   | ", Style::default().fg(t.warning)),
                Span::styled(
                    q[2..].trim().to_owned(),
                    Style::default()
                        .fg(t.text_muted)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]))
        } else {
            let mut s = vec![Span::raw("   ")];
            s.extend(parse_inline_spans(line, t));
            l.push(Line::from(s))
        }
    }
    if l.is_empty() {
        l.push(Line::from(""))
    }
    l
}
fn parse_inline_spans<'a>(text: &'a str, t: &Theme) -> Vec<Span<'a>> {
    let mut s = Vec::new();
    for (i, p) in text.split('`').enumerate() {
        if p.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            s.push(Span::styled(
                format!(" {} ", p),
                Style::default().fg(t.primary).bg(t.code_bg),
            ))
        } else {
            for (j, x) in p.split("**").enumerate() {
                if x.is_empty() {
                    continue;
                }
                s.push(if j % 2 == 1 {
                    Span::styled(
                        x.to_string(),
                        Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(x.to_string(), Style::default().fg(t.text))
                })
            }
        }
    }
    if s.is_empty() {
        s.push(Span::styled(text.to_string(), Style::default().fg(t.text)))
    }
    s
}
fn centered_rect(x: u16, y: u16, r: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100 - y) / 2),
        Constraint::Percentage(y),
        Constraint::Percentage((100 - y) / 2),
    ])
    .split(r);
    Layout::horizontal([
        Constraint::Percentage((100 - x) / 2),
        Constraint::Percentage(x),
        Constraint::Percentage((100 - x) / 2),
    ])
    .split(v[1])[1]
}
