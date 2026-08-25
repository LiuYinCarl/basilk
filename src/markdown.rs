use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

/// Render Markdown source into a ratatui `Text` with terminal styles.
/// Supported: headings, bold/italic/strikethrough, inline code, code
/// blocks, lists (bullets and numbers, nested), blockquotes, links and
/// horizontal rules. Tables and raw HTML fall back to plain text.
pub fn render_markdown(md: &str) -> Text<'static> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, options);

    let mut renderer = Renderer::default();
    renderer.run(parser);
    Text::from(renderer.finish())
}

#[derive(Default)]
struct Renderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    style: Style,
    style_stack: Vec<Style>,
    /// `Some(n)` = ordered list with next number `n`, `None` = bullets.
    list_stack: Vec<Option<u64>>,
    quote_depth: usize,
    in_code_block: bool,
    link_url: Option<String>,
}

impl Renderer {
    fn run(&mut self, parser: Parser) {
        for event in parser {
            match event {
                Event::Start(tag) => self.start_tag(tag),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) => self.text(&text),
                Event::Code(code) => {
                    self.spans.push(Span::styled(
                        format!("`{}`", code),
                        self.style.fg(Color::LightYellow),
                    ));
                }
                Event::SoftBreak => self.spans.push(Span::raw(" ")),
                Event::HardBreak => self.flush_line(),
                Event::Rule => {
                    self.blank_line_before();
                    self.flush_line();
                    self.lines.push(Line::from(Span::styled(
                        "─".repeat(40),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                // Raw HTML and everything else: ignored
                _ => {}
            }
        }
    }

    fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                if self.list_stack.is_empty() {
                    self.blank_line_before();
                }
            }
            Tag::Heading { level, .. } => {
                self.blank_line_before();
                self.push_style(heading_style(level));
            }
            Tag::BlockQuote(_) => {
                self.blank_line_before();
                self.quote_depth += 1;
            }
            Tag::CodeBlock(_) => {
                self.blank_line_before();
                self.flush_line();
                self.in_code_block = true;
            }
            Tag::List(start) => {
                if self.list_stack.is_empty() {
                    self.blank_line_before();
                }
                self.list_stack.push(start);
            }
            Tag::Item => {
                self.flush_line();

                let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
                let marker = match self.list_stack.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{}. ", n);
                        *n += 1;
                        m
                    }
                    _ => "• ".to_string(),
                };
                self.spans
                    .push(Span::styled(format!("{}{}", indent, marker), self.style));
            }
            Tag::Strong => self.push_style(self.style.add_modifier(Modifier::BOLD)),
            Tag::Emphasis => self.push_style(self.style.add_modifier(Modifier::ITALIC)),
            Tag::Strikethrough => self.push_style(self.style.add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => {
                self.link_url = Some(dest_url.to_string());
                self.push_style(
                    self.style
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_line(),
            TagEnd::Heading(_) => {
                self.flush_line();
                self.pop_style();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_line();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            TagEnd::Item => self.flush_line(),
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => self.pop_style(),
            TagEnd::Link => {
                self.pop_style();
                if let Some(url) = self.link_url.take() {
                    self.spans.push(Span::styled(
                        format!(" ({})", url),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_code_block {
            // Code block content arrives as one chunk; emit it line by line
            for line in text.lines() {
                self.lines.push(Line::from(Span::styled(
                    format!("  {}", line),
                    Style::default().fg(Color::LightYellow),
                )));
            }
            return;
        }

        self.spans.push(Span::styled(text.to_string(), self.style));
    }

    fn push_style(&mut self, style: Style) {
        self.style_stack.push(self.style);
        self.style = style;
    }

    fn pop_style(&mut self) {
        self.style = self.style_stack.pop().unwrap_or_default();
    }

    /// Push a separating blank line when the previous line has content.
    fn blank_line_before(&mut self) {
        let has_content = self
            .lines
            .last()
            .is_some_and(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()));

        if has_content || !self.spans.is_empty() {
            self.flush_line();
            self.lines.push(Line::default());
        }
    }

    fn flush_line(&mut self) {
        if self.spans.is_empty() {
            return;
        }

        let mut spans = std::mem::take(&mut self.spans);
        if self.quote_depth > 0 {
            let mut quoted = Vec::with_capacity(spans.len() + 1);
            quoted.push(Span::styled(
                "│ ".repeat(self.quote_depth),
                Style::default().fg(Color::DarkGray),
            ));
            quoted.append(&mut spans);
            self.lines.push(Line::from(quoted));
        } else {
            self.lines.push(Line::from(spans));
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();

        while self
            .lines
            .last()
            .is_some_and(|l| !l.spans.iter().any(|s| !s.content.trim().is_empty()))
        {
            self.lines.pop();
        }

        self.lines
    }
}

fn heading_style(level: HeadingLevel) -> Style {
    match level {
        HeadingLevel::H1 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        HeadingLevel::H2 => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H3 => Style::default()
            .fg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().add_modifier(Modifier::BOLD),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn rendered_lines(md: &str) -> Vec<String> {
        render_markdown(md).lines.iter().map(line_text).collect()
    }

    #[test]
    fn empty_input_produces_no_lines() {
        assert!(render_markdown("").lines.is_empty());
        assert!(render_markdown("  \n\n ").lines.is_empty());
    }

    #[test]
    fn heading_is_styled_by_level() {
        let text = render_markdown("# Title\n\n## Sub\n\n#### Deep");
        assert_eq!(line_text(&text.lines[0]), "Title");
        assert!(text.lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED));
        assert_eq!(text.lines[0].spans[0].style.fg, Some(Color::Cyan));

        assert_eq!(line_text(&text.lines[2]), "Sub");
        assert_eq!(text.lines[2].spans[0].style.fg, Some(Color::Green));

        assert_eq!(line_text(&text.lines[4]), "Deep");
        assert_eq!(text.lines[4].spans[0].style.fg, None);
    }

    #[test]
    fn inline_styles_apply_modifiers() {
        let text = render_markdown("a **b** *c* ~~d~~ `e`");
        let spans = &text.lines[0].spans;

        let bold = spans.iter().find(|s| s.content == "b").unwrap();
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));

        let italic = spans.iter().find(|s| s.content == "c").unwrap();
        assert!(italic.style.add_modifier.contains(Modifier::ITALIC));

        let struck = spans.iter().find(|s| s.content == "d").unwrap();
        assert!(struck.style.add_modifier.contains(Modifier::CROSSED_OUT));

        let code = spans.iter().find(|s| s.content == "`e`").unwrap();
        assert_eq!(code.style.fg, Some(Color::LightYellow));

        // Styles do not leak into the surrounding text
        let plain = spans.iter().find(|s| s.content.contains('a')).unwrap();
        assert_eq!(plain.style.add_modifier, Modifier::empty());
    }

    #[test]
    fn nested_bold_italic_keeps_both_modifiers() {
        let text = render_markdown("***both***");
        let span = &text.lines[0].spans[0];
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn unordered_list_uses_bullets() {
        let lines = rendered_lines("- one\n- two");
        assert_eq!(lines, vec!["• one", "• two"]);
    }

    #[test]
    fn ordered_list_numbers_sequentially() {
        let lines = rendered_lines("1. one\n2. two\n3. three");
        assert_eq!(lines, vec!["1. one", "2. two", "3. three"]);
    }

    #[test]
    fn nested_list_is_indented() {
        let lines = rendered_lines("- outer\n  - inner");
        assert_eq!(lines, vec!["• outer", "  • inner"]);
    }

    #[test]
    fn code_block_lines_are_indented_and_styled() {
        let text = render_markdown("before\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n\nafter");
        let lines: Vec<String> = text.lines.iter().map(line_text).collect();

        assert_eq!(
            lines,
            vec!["before", "", "  let a = 1;", "  let b = 2;", "", "after"]
        );
        assert_eq!(text.lines[2].spans[0].style.fg, Some(Color::LightYellow));
    }

    #[test]
    fn blockquote_gets_bar_prefix() {
        let text = render_markdown("> quoted");
        assert_eq!(line_text(&text.lines[0]), "│ quoted");
        assert_eq!(text.lines[0].spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn link_is_underlined_with_dim_url() {
        let text = render_markdown("[click](https://example.com)");
        let spans = &text.lines[0].spans;

        assert!(spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(spans[0].content.as_ref(), "click");
        assert_eq!(spans[1].content.as_ref(), " (https://example.com)");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn rule_renders_a_dim_bar() {
        let lines = rendered_lines("a\n\n---\n\nb");
        assert_eq!(lines, vec!["a", "", "─".repeat(40).as_str(), "", "b"]);
    }

    #[test]
    fn paragraphs_are_separated_by_a_blank_line() {
        let lines = rendered_lines("first\n\nsecond");
        assert_eq!(lines, vec!["first", "", "second"]);
    }

    #[test]
    fn soft_breaks_join_into_one_line() {
        let lines = rendered_lines("foo\nbar");
        assert_eq!(lines, vec!["foo bar"]);
    }

    #[test]
    fn raw_html_is_ignored() {
        assert!(render_markdown("<div>ignored</div>").lines.is_empty());
    }

    #[test]
    fn image_alt_text_is_kept_but_the_url_is_not_shown() {
        let lines = rendered_lines("![alt](https://example.com/pic.png)");
        assert_eq!(lines, vec!["alt"]);
    }

    #[test]
    fn h3_heading_gets_the_magenta_style() {
        let text = render_markdown("### three");
        assert_eq!(line_text(&text.lines[0]), "three");
        assert_eq!(text.lines[0].spans[0].style.fg, Some(Color::LightMagenta));
    }

    #[test]
    fn hard_breaks_split_the_line() {
        let lines = rendered_lines("foo  \nbar");
        assert_eq!(lines, vec!["foo", "bar"]);
    }

    #[test]
    fn trailing_blank_lines_are_trimmed() {
        // The empty heading produces a trailing blank line that `finish` trims
        let lines = rendered_lines("# hi\n\n# ");
        assert_eq!(lines, vec!["hi"]);
    }

    #[test]
    fn unclosed_and_malformed_input_does_not_panic() {
        for md in ["**", "# ", "```\nunclosed", "[](", "> ", "-", "1.", "│ │"] {
            render_markdown(md);
        }
    }
}
