use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Clear, ListItem, Paragraph, Widget},
    Frame,
};
use tui_input::Input;

pub struct Ui {}

impl Ui {
    pub fn load_delete_confirm_items(items: &mut Vec<ListItem>) {
        items.clear();
        items.push(ListItem::from(Span::styled(
            "Confirm",
            Style::new().fg(Color::Red),
        )));
        items.push(ListItem::from(Span::styled(
            "Cancel",
            Style::new().fg(Color::Gray),
        )));
    }

    pub fn create_rect_area(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let content_height = percent_y.min(r.height.saturating_sub(2));
        let vertical_margin = (r.height.saturating_sub(content_height)) / 2;

        let popup_layout = Layout::vertical([
            Constraint::Length(vertical_margin),
            Constraint::Length(content_height),
            Constraint::Min(vertical_margin),
        ])
        .split(r);

        Layout::horizontal([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Min(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
    }

    pub fn create_modal<W: Widget>(
        f: &mut Frame,
        percent_x: u16,
        percent_y: u16,
        area: Rect,
        widget: W,
    ) {
        let area = Ui::create_rect_area(percent_x, percent_y, area);
        f.render_widget(Clear, area); //this clears out the background
        f.render_widget(widget, area);
    }

    pub fn create_input_modal(title: &str, f: &mut Frame, area: Rect, input: &Input) {
        let area = Ui::create_rect_area(50, 3, area);

        let width = area.width.max(3) - 3;
        let scroll = input.visual_scroll(width as usize);

        let input_widget = Paragraph::new(input.value())
            .block(Block::default().borders(Borders::ALL).title(title))
            .scroll((0, scroll as u16));

        f.render_widget(Clear, area); //this clears out the background
        f.render_widget(input_widget, area);

        f.set_cursor(
            // Put cursor past the end of the input text
            area.x + ((input.visual_cursor()).max(scroll) - scroll) as u16 + 1,
            // Move one line down, from the border to the input line
            area.y + 1,
        )
    }
}
