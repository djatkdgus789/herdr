//! Usages section: a block under the Agent panel that shows the custom
//! `[ui.sidebar.usages]` rows, resolved from workspace metadata tokens. A
//! reporter (for example a plan-usage plugin) owns the values; this module only
//! places and draws them.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

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
    // Meters sit one column deeper than the account line above them, and all
    // share one layout so their bars and columns line up.
    let meter_x = x.saturating_add(1);
    let meter_width = width.saturating_sub(1);
    let meters = rows
        .iter()
        .filter_map(|row| row_meter(row))
        .collect::<Vec<_>>();
    let layout = MeterLayout::fit(&meters, usize::from(meter_width));
    for (index, row) in rows.iter().enumerate() {
        let y = area.y + USAGE_HEADER_ROWS + index.min(u16::MAX as usize) as u16;
        if y >= area.bottom() {
            break;
        }
        if let Some(meter) = row_meter(row) {
            let spans = meter_spans(&meter, &layout, palette);
            Paragraph::new(Line::from(spans)).render(Rect::new(meter_x, y, meter_width, 1), buffer);
            continue;
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

/// Narrowest bar worth drawing; optional columns are dropped to keep it.
const MIN_METER_BAR: usize = 8;
/// Partial-cell glyphs for one to seven eighths of a bar cell.
const METER_EIGHTHS: [&str; 7] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// A usage window reported as plain text, so it stays readable anywhere the
/// token is shown: `"<tag> <n>% left|used[ · <reset>]"`, or `"<tag> —"` when
/// the reporter has no current reading.
#[derive(Debug, PartialEq, Eq)]
struct UsageMeter<'a> {
    tag: &'a str,
    reading: Option<MeterReading>,
    reset: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
struct MeterReading {
    /// The number as reported, which is also the bar fill.
    shown: u8,
    /// Share still available, which picks the tone either way.
    remaining: u8,
    word: &'static str,
}

fn parse_usage_meter(value: &str) -> Option<UsageMeter<'_>> {
    let (body, reset) = match value.split_once(" · ") {
        Some((body, reset)) => (body, Some(reset.trim()).filter(|reset| !reset.is_empty())),
        None => (value, None),
    };
    if let Some(tag) = body.strip_suffix(" —") {
        let tag = tag.trim();
        return (!tag.is_empty()).then_some(UsageMeter {
            tag,
            reading: None,
            reset,
        });
    }
    let mut words = body.rsplitn(3, ' ');
    let word = match words.next()? {
        "left" => "left",
        "used" => "used",
        _ => return None,
    };
    let shown = words.next()?.strip_suffix('%')?.parse::<u8>().ok()?;
    let tag = words.next()?.trim();
    if shown > 100 || tag.is_empty() {
        return None;
    }
    let remaining = if word == "left" { shown } else { 100 - shown };
    Some(UsageMeter {
        tag,
        reading: Some(MeterReading {
            shown,
            remaining,
            word,
        }),
        reset,
    })
}

/// A row renders as a meter only when its single custom value parses as one;
/// every other row keeps the ordinary token rendering.
fn row_meter(row: &[crate::ui::ResolvedToken]) -> Option<UsageMeter<'_>> {
    match row {
        [token] => match &token.kind {
            crate::ui::ResolvedTokenKind::Custom(value) => parse_usage_meter(value),
            _ => None,
        },
        _ => None,
    }
}

/// Fill `fill_percent` of `width` cells in eighths of a cell.
fn meter_bar(fill_percent: u8, width: usize) -> (String, usize) {
    let eighths = (usize::from(fill_percent) * width * 8 + 50) / 100;
    let mut bar = "█".repeat(eighths / 8);
    let mut cells = eighths / 8;
    if eighths % 8 > 0 && cells < width {
        bar.push_str(METER_EIGHTHS[eighths % 8 - 1]);
        cells += 1;
    }
    (bar, width - cells)
}

/// Column widths shared by every meter in the block.
#[derive(Debug, PartialEq, Eq)]
struct MeterLayout {
    tag_width: usize,
    bar_width: usize,
    show_word: bool,
    /// Width of the reset column, without its leading space; 0 hides it.
    reset_width: usize,
}

/// " 100%": a space and the right-aligned number.
const METER_NUMBER_WIDTH: usize = 5;
/// " left" / " used".
const METER_WORD_WIDTH: usize = 5;

impl MeterLayout {
    /// Fits every meter into `width` columns, dropping the word first and then
    /// the reset column while the bar would be narrower than `MIN_METER_BAR`.
    fn fit(meters: &[UsageMeter<'_>], width: usize) -> Self {
        let tag_width = meters
            .iter()
            .map(|meter| meter.tag.width())
            .max()
            .unwrap_or(0);
        let any_word = meters.iter().any(|meter| meter.reading.is_some());
        let longest_reset = meters
            .iter()
            .filter_map(|meter| meter.reset.map(UnicodeWidthStr::width))
            .max()
            .unwrap_or(0);
        let fixed = tag_width + 1 + METER_NUMBER_WIDTH;
        let reset_columns = |reset_width: usize| {
            if reset_width == 0 {
                0
            } else {
                reset_width + 1
            }
        };
        for (show_word, reset_width) in [(any_word, longest_reset), (false, longest_reset)] {
            let used =
                fixed + usize::from(show_word) * METER_WORD_WIDTH + reset_columns(reset_width);
            if used + MIN_METER_BAR <= width {
                return Self {
                    tag_width,
                    bar_width: width - used,
                    show_word,
                    reset_width,
                };
            }
        }
        // Too narrow for any optional column: the bar takes what is left.
        Self {
            tag_width,
            bar_width: width.saturating_sub(fixed),
            show_word: false,
            reset_width: 0,
        }
    }
}

fn meter_spans(
    meter: &UsageMeter<'_>,
    layout: &MeterLayout,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let tag_width = layout.tag_width;
    let mut spans = vec![Span::styled(
        format!("{:<tag_width$} ", meter.tag),
        Style::default().fg(palette.text),
    )];
    let word_pad = if layout.show_word {
        " ".repeat(METER_WORD_WIDTH)
    } else {
        String::new()
    };
    match &meter.reading {
        Some(reading) => {
            let tone = match reading.remaining {
                50.. => palette.green,
                20.. => palette.yellow,
                _ => palette.red,
            };
            let (fill, track_cells) = meter_bar(reading.shown, layout.bar_width);
            spans.push(Span::styled(fill, Style::default().fg(tone)));
            spans.push(Span::styled(
                "░".repeat(track_cells),
                Style::default().fg(tone).add_modifier(Modifier::DIM),
            ));
            let word = if layout.show_word {
                format!(" {}", reading.word)
            } else {
                String::new()
            };
            spans.push(Span::styled(
                format!(" {:>3}%{word}", reading.shown),
                Style::default().fg(tone),
            ));
        }
        None => {
            spans.push(Span::styled(
                "░".repeat(layout.bar_width),
                Style::default().fg(palette.surface_dim),
            ));
            spans.push(Span::styled(
                format!("    —{word_pad}"),
                Style::default().fg(palette.overlay0),
            ));
        }
    }
    if layout.reset_width > 0 {
        let reset_width = layout.reset_width;
        spans.push(Span::styled(
            format!(" {:<reset_width$}", meter.reset.unwrap_or_default()),
            Style::default().fg(palette.overlay0),
        ));
    }
    spans
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
    fn parses_usage_meters() {
        assert_eq!(
            parse_usage_meter("5h 60% left · 1h 56m"),
            Some(UsageMeter {
                tag: "5h",
                reading: Some(MeterReading {
                    shown: 60,
                    remaining: 60,
                    word: "left",
                }),
                reset: Some("1h 56m"),
            })
        );
        let used = parse_usage_meter("Fable 30% used").unwrap();
        assert_eq!(used.tag, "Fable");
        assert_eq!(used.reading.unwrap().remaining, 70);
        assert_eq!(
            parse_usage_meter("7d —"),
            Some(UsageMeter {
                tag: "7d",
                reading: None,
                reset: None,
            })
        );
        assert_eq!(parse_usage_meter("UNIST · Team"), None);
        assert_eq!(parse_usage_meter("5h 160% left"), None);
        assert_eq!(parse_usage_meter("60% left"), None);
    }

    #[test]
    fn meter_bar_fills_in_eighths() {
        assert_eq!(meter_bar(100, 10), ("█".repeat(10), 0));
        assert_eq!(meter_bar(0, 10), (String::new(), 10));
        assert_eq!(meter_bar(55, 10), (format!("{}▌", "█".repeat(5)), 4));
    }

    fn meter_text(meter: &UsageMeter<'_>, layout: &MeterLayout) -> String {
        let palette = crate::app::state::Palette::catppuccin();
        meter_spans(meter, layout, &palette)
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn meters_share_one_layout_and_drop_optional_columns_before_the_bar() {
        let meters = [
            parse_usage_meter("5h 60% left · 1h 56m").unwrap(),
            parse_usage_meter("Fable 0% left · 11h 37m").unwrap(),
            parse_usage_meter("7d —").unwrap(),
        ];
        let wide = MeterLayout::fit(&meters, 40);
        assert!(wide.show_word);
        assert_eq!(wide.reset_width, 7);
        let lines = meters
            .iter()
            .map(|meter| meter_text(meter, &wide))
            .collect::<Vec<_>>();
        for line in &lines {
            assert_eq!(line.width(), 40, "{line:?}");
        }
        assert!(lines[0].ends_with(" 60% left 1h 56m "), "{:?}", lines[0]);
        assert!(lines[2].starts_with("7d    ░"), "{:?}", lines[2]);

        let narrow = MeterLayout::fit(&meters, 28);
        assert!(!narrow.show_word);
        assert_eq!(narrow.reset_width, 7);
        assert!(meter_text(&meters[0], &narrow).ends_with(" 60% 1h 56m "));

        let tight = MeterLayout::fit(&meters, 18);
        assert_eq!((tight.show_word, tight.reset_width), (false, 0));
        assert!(meter_text(&meters[0], &tight).ends_with(" 60%"));
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
