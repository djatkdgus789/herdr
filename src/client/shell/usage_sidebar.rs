//! Usages section: a block under the Agent panel that shows the custom
//! `[ui.sidebar.usages]` rows, resolved from workspace metadata tokens. A
//! reporter (for example a plan-usage plugin) owns the values; this module only
//! places and draws them.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Paragraph, Widget},
};

use super::*;

/// The divider and " usages" title above the rows, mirroring the Agent
/// panel header.
const USAGE_HEADER_ROWS: u16 = 2;
/// Rows the Agent panel keeps before the Usages block may take space from it:
/// its own three header rows plus a few entries.
const MIN_AGENT_PANEL_ROWS: u16 = 6;

/// Every workspace's metadata tokens. Earlier Spaces win a name clash, so the
/// result follows sidebar order rather than server hash order.
fn merged_workspace_tokens(workspaces: &[ClientShellWorkspace]) -> HashMap<String, String> {
    let mut merged = HashMap::new();
    for workspace in workspaces {
        for (name, value) in &workspace.tokens {
            merged.entry(name.clone()).or_insert_with(|| value.clone());
        }
    }
    merged
}

/// The Usages rows that currently have a reported value. Empty when the
/// section is unconfigured, so the default layout does no extra work.
pub(super) fn usage_section_rows(
    snapshot: Option<&ClientShellSnapshot>,
    config: &ClientShellConfig,
) -> Vec<Vec<crate::ui::ResolvedToken>> {
    if config.usages.rows.is_empty() {
        return Vec::new();
    }
    let Some(snapshot) = snapshot else {
        return Vec::new();
    };
    let tokens = merged_workspace_tokens(&snapshot.workspaces);
    crate::ui::sidebar_usage_rows(&config.usages, &tokens)
}

/// Splits the sidebar detail area into the Agent panel (top) and the Usages
/// block (bottom). With no rows, or when the block would leave the Agent panel
/// shorter than `MIN_AGENT_PANEL_ROWS`, the Agent panel keeps everything; when
/// only some rows fit, the block shrinks and drops its last rows.
pub(super) fn split_usage_section(area: Rect, row_count: usize) -> (Rect, Rect) {
    if row_count == 0 {
        return (area, Rect::default());
    }
    let available = area.height.saturating_sub(MIN_AGENT_PANEL_ROWS);
    if available <= USAGE_HEADER_ROWS {
        return (area, Rect::default());
    }
    let wanted = USAGE_HEADER_ROWS.saturating_add(row_count.min(u16::MAX as usize) as u16);
    let height = wanted.min(available);
    (
        Rect::new(area.x, area.y, area.width, area.height - height),
        Rect::new(area.x, area.bottom() - height, area.width, height),
    )
}

pub(super) fn render_usage_panel(
    buffer: &mut Buffer,
    area: Rect,
    rows: &[Vec<crate::ui::ResolvedToken>],
    config: &ClientShellConfig,
) {
    if area.height <= USAGE_HEADER_ROWS || area.width == 0 {
        return;
    }
    let palette = &config.palette;
    super::render::put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        &"─".repeat(area.width as usize),
        Style::default().fg(palette.surface_dim),
    );
    super::render::put_text(
        buffer,
        area.x,
        area.y + 1,
        area.width,
        " usages",
        Style::default()
            .fg(palette.overlay0)
            .add_modifier(Modifier::BOLD),
    );
    // Rows indent one column like Space rows and stop two columns short of the
    // edge, which keeps the bottom row clear of the sidebar collapse toggle.
    let x = area.x.saturating_add(1);
    let width = area.right().saturating_sub(2).saturating_sub(x);
    for (index, row) in rows.iter().enumerate() {
        let y = area.y + USAGE_HEADER_ROWS + index.min(u16::MAX as usize) as u16;
        if y >= area.bottom() {
            break;
        }
        let spans = crate::ui::resolved_token_spans(
            row,
            ("", Style::default()),
            Style::default(),
            Style::default(),
            Style::default(),
            Style::default().fg(palette.subtext0),
            palette,
            width as usize,
        );
        Paragraph::new(Line::from(spans)).render(Rect::new(x, y, width, 1), buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: &str, tokens: &[(&str, &str)]) -> ClientShellWorkspace {
        ClientShellWorkspace {
            workspace_id: id.to_string(),
            active_tab_id: String::new(),
            new_workspace_cwd: String::new(),
            number: 1,
            label: id.to_string(),
            custom_label: false,
            branch: None,
            git_ahead_behind: None,
            tokens: tokens
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            worktree: None,
            focused: false,
            agent_status: crate::api::schema::AgentStatus::Unknown,
        }
    }

    #[test]
    fn merged_tokens_prefer_the_earlier_space() {
        let merged = merged_workspace_tokens(&[
            workspace("wA", &[("limits_a", "first")]),
            workspace("wB", &[("limits_a", "second"), ("limits_b", "only")]),
        ]);
        assert_eq!(merged.get("limits_a").map(String::as_str), Some("first"));
        assert_eq!(merged.get("limits_b").map(String::as_str), Some("only"));
    }

    #[test]
    fn split_gives_everything_to_agents_without_rows() {
        let area = Rect::new(0, 10, 30, 20);
        assert_eq!(split_usage_section(area, 0), (area, Rect::default()));
    }

    #[test]
    fn split_takes_header_plus_rows_from_the_bottom() {
        let area = Rect::new(0, 10, 30, 20);
        let (agents, usages) = split_usage_section(area, 2);
        assert_eq!(agents, Rect::new(0, 10, 30, 16));
        assert_eq!(usages, Rect::new(0, 26, 30, 4));
    }

    #[test]
    fn split_shrinks_rows_before_squeezing_agents_and_drops_when_too_short() {
        let area = Rect::new(0, 0, 30, 10);
        let (agents, usages) = split_usage_section(area, 8);
        assert_eq!(agents.height, MIN_AGENT_PANEL_ROWS);
        assert_eq!(usages.height, 4);

        let short = Rect::new(0, 0, 30, MIN_AGENT_PANEL_ROWS + USAGE_HEADER_ROWS);
        assert_eq!(split_usage_section(short, 3), (short, Rect::default()));
    }
}
