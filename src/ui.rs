use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::session::{Session, SessionStatus};
use crate::unicode_utils::{truncate_id, truncate_message};

pub fn draw(
    f: &mut ratatui::Frame,
    sessions_by_project: &HashMap<String, Vec<&Session>>,
    project_filter: &Option<String>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.size());

    draw_header(f, chunks[0], sessions_by_project);
    draw_main_content(f, chunks[1], sessions_by_project, project_filter);
    draw_footer(f, chunks[2]);
}

fn draw_header(
    f: &mut ratatui::Frame,
    area: Rect,
    sessions_by_project: &HashMap<String, Vec<&Session>>,
) {
    let total_sessions = sessions_by_project.values().map(|v| v.len()).sum::<usize>();
    let active_sessions = sessions_by_project
        .values()
        .flatten()
        .filter(|s| matches!(s.status, SessionStatus::Active))
        .count();
    
    let header_text = format!(
        "Claude Session Monitor - {} projects, {} sessions ({} active)",
        sessions_by_project.len(),
        total_sessions,
        active_sessions
    );

    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Status"));

    f.render_widget(header, area);
}

fn draw_main_content(
    f: &mut ratatui::Frame,
    area: Rect,
    sessions_by_project: &HashMap<String, Vec<&Session>>,
    project_filter: &Option<String>,
) {
    let mut projects: Vec<_> = sessions_by_project.iter().collect();
    projects.sort_by(|a, b| {
        // 最新のセッションの時刻でソート
        let a_latest = a.1.iter().map(|s| s.last_activity).max().unwrap_or_default();
        let b_latest = b.1.iter().map(|s| s.last_activity).max().unwrap_or_default();
        b_latest.cmp(&a_latest)
    });

    // プロジェクトフィルタリング
    if let Some(filter) = project_filter {
        projects.retain(|(name, _)| name.contains(filter));
    }

    // セッションが見つからない場合のメッセージ
    if projects.is_empty() {
        let message = if project_filter.is_some() {
            "No sessions found for the specified project filter.\nTry removing the filter or check if Claude sessions exist."
        } else {
            "No Claude sessions found.\nMake sure Claude is running and has active sessions in ~/.claude/projects/"
        };
        
        let help_text = Text::from(vec![
            Line::from(""),
            Line::from(Span::styled("🔍 No Sessions Found", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(message),
            Line::from(""),
            Line::from("Waiting for Claude sessions to start..."),
        ]);
        
        let help_paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL).title("Status"));
        
        f.render_widget(help_paragraph, area);
        return;
    }

    let mut y_offset = 0;
    for (project_name, sessions) in projects {
        if y_offset >= area.height {
            break;
        }

        let project_height = (sessions.len() as u16 + 2).min(area.height - y_offset);
        let project_area = Rect {
            x: area.x,
            y: area.y + y_offset,
            width: area.width,
            height: project_height,
        };

        draw_project_group(f, project_area, project_name, sessions);
        y_offset += project_height;
    }
}

fn draw_project_group(
    f: &mut ratatui::Frame,
    area: Rect,
    project_name: &str,
    sessions: &[&Session],
) {
    let active_count = sessions.iter().filter(|s| matches!(s.status, SessionStatus::Active)).count();
    
    let title = format!("{} ({} sessions, {} active)", project_name, sessions.len(), active_count);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // セッションリスト
    let session_items: Vec<ListItem> = sessions
        .iter()
        .take((inner.height as usize).saturating_sub(1))
        .map(|session| create_session_item(session))
        .collect();

    let session_list = List::new(session_items);
    f.render_widget(session_list, inner);
}

fn create_session_item(session: &Session) -> ListItem {
    let status_style = match session.status {
        SessionStatus::Active => Style::default().fg(Color::Green),
        SessionStatus::Approve => Style::default().fg(Color::Yellow),
        SessionStatus::Finish => Style::default().fg(Color::Blue),
        SessionStatus::Error => Style::default().fg(Color::Red),
        SessionStatus::Idle => Style::default().fg(Color::Gray),
    };

    let time_ago = format_time_ago(session.last_activity);
    let current_task = session.current_task
        .as_ref()
        .map(|t| format!(" - {}", t))
        .unwrap_or_default();

    let content = vec![Line::from(vec![
        Span::styled(
            format!("{} {}", session.status.icon(), session.status.label()),
            status_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            truncate_id(&session.id),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" "),
        Span::styled(time_ago, Style::default().fg(Color::Gray)),
        Span::styled(current_task, Style::default().fg(Color::White)),
    ])];

    // 最新メッセージがある場合は表示
    let mut lines = content;
    if !session.last_message.is_empty() {
        let message_preview = truncate_message(&session.last_message, 77);
        
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(message_preview, Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
        ]));
    }

    ListItem::new(Text::from(lines))
}

fn draw_footer(f: &mut ratatui::Frame, area: Rect) {
    let footer_text = "Press 'q' to quit | 'r' to refresh | Arrow keys to navigate";
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}

fn format_time_ago(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);

    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else {
        format!("{}d ago", duration.num_days())
    }
}