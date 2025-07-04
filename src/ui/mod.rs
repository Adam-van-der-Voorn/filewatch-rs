use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Span;
use ratatui::widgets::{Block, StatefulWidget};
use ratatui::Frame;


struct LogsWidget {
    pub logs: Vec<String>,
    pub scroll_y: usize,
}

#[derive(Default)]
pub struct LogsWidgetState {
    pub actual_scroll_y: usize,
    pub was_at_bottom: bool,
    pub last_log_count: usize,
    pub height: u16,
}

impl LogsWidget {
    pub fn new(logs: Vec<String>) -> Self {
        LogsWidget { logs, scroll_y: 0 }
    }

    #[allow(unused)]
    fn render_width_marker(&self, area: Rect, buf: &mut Buffer) {
        let width: usize = area.width.into();
        let mut width_str = String::new();
        width_str.push('x');
        for _ in 0..width-2 {
            width_str.push_str(format!("-").as_str())
        }
        width_str.push('x');
        buf.set_stringn(area.x, area.y, width_str, usize::MAX, Style::default());
    }

    fn render_logs(&self, area: Rect, buf: &mut Buffer, state: &mut LogsWidgetState) {
        // Check if new logs arrived and we were at bottom
        let input_log_count = self.logs.len();
        let new_logs_arrived = input_log_count > state.last_log_count;
        
        // Create a mutable copy of scroll_y for potential auto-scroll
        let scroll_y = if new_logs_arrived && state.was_at_bottom {
            // Auto-scroll to bottom when new logs arrive
            usize::MAX
        } else {
            self.scroll_y
        };

        log::debug!("render with vals: input_lc={} last_lc={} new_logs?={} was_at_bottom={} scroll_in={} scroll={}",
            input_log_count,
            state.last_log_count,
            new_logs_arrived,
            state.was_at_bottom,
            self.scroll_y,
            scroll_y,
        );
        
        let width: usize = area.width.into();
        let mut yy = 0;
        let (log_idx, char_offset, scroll_y_actual, at_bottom) = LogsWidget::get_log_at_scroll_pos(&self.logs, area, scroll_y);        
        
        // Update state
        state.actual_scroll_y = scroll_y_actual;
        state.last_log_count = input_log_count;
        state.was_at_bottom = at_bottom;
        state.height = area.height;

        let mut char_offset = char_offset;
        let logs_page = self.logs.get(log_idx..)
            .unwrap_or_default();
        for log in logs_page.iter() {
            let mut line = String::new();
            for c in log.chars() {
                if char_offset > 0 {
                    // discard
                    char_offset -= 1;
                    continue;
                }
                line.push(c);
                if line.len() >= width {
                    let y_pos = area.y + yy;
                    if y_pos < area.height {
                        buf.set_stringn(area.x, area.y + yy, &line, usize::MAX, Style::default());
                    }
                    line = String::new();
                    yy += 1;
                } 
            }
            let y_pos = area.y + yy;
            if y_pos < area.height {
                buf.set_stringn(area.x, area.y + yy, &line, usize::MAX, Style::default());
            }
            yy += 1
        }
    }

    /// Calculates which log entry and character offset to start rendering from based on scroll position.
    /// 
    /// This function handles text wrapping by calculating how many screen lines each log entry
    /// occupies given the terminal width, then determines where to start rendering based on
    /// the current scroll position.
    /// 
    /// # Arguments
    /// * `logs` - The logs to operate on
    /// * `area` - The rendering area containing width and height information
    /// * `scroll_y` - The line to start from
    /// 
    /// # Returns
    /// A tuple `(log_index, char_offset, line_offset, at_bottom)` where:
    /// * `log_index` - Index of the log entry to start rendering from
    /// * `char_offset` - Number of characters to skip within that log entry
    /// * `line_offset` - Actual number of lines scrolled.
    /// * `at_bottom` - true if the line_offset returned is the last line
    /// 
    /// # Example
    /// Given logs with wrapping at width=10:
    /// - Log 0: "hello world!" (12 chars = 2 lines)  
    /// - Log 1: "short" (5 chars = 1 line)
    /// - Log 2: "very long message here" (22 chars = 3 lines)
    /// 
    /// If scroll_y=3, this would return (2, 10, 3, true) meaning start at log 2,
    /// skip 10 characters (start from "message here").
    fn get_log_at_scroll_pos(logs: &[String], area: Rect, scroll_y: usize) -> (usize, usize, usize, bool) {
        let width: usize = area.width.into();
        let height: usize = area.height.into();
        let target_line = scroll_y.saturating_add(height);        
        let mut lines = vec![];
        let mut at_bottom = false;

        'outer: for (log_idx, log) in logs.iter().enumerate() {
            let is_last_log = log_idx == logs.len() - 1;
            let log_chars = log.chars().count();
            let lines_for_this_log = if log_chars == 0 { 1 } else { (log_chars + width - 1) / width };
            for line_idx in 0..lines_for_this_log {
                let char_offset = line_idx * width;
                log::debug!("is_last_log={} idx={} last_idx={}", is_last_log, line_idx, lines_for_this_log.saturating_sub(1));
                at_bottom = is_last_log && line_idx == lines_for_this_log.saturating_sub(1);
                lines.push((log_idx, char_offset));
                if lines.len() == target_line {
                    break 'outer;
                }
            }
        }
        
        let real_scroll_y = lines.len().saturating_sub(height);
        let (log_idx, char_offset) = *lines.get(real_scroll_y).unwrap_or(&(0, 0));
        return (log_idx, char_offset, real_scroll_y, at_bottom);
    }


    #[must_use = "method moves the value of self and returns the modified value"]
    pub const fn scroll(mut self, y: usize) -> Self {
        self.scroll_y = y;
        self
    } 
}

impl StatefulWidget for LogsWidget {
    type State = LogsWidgetState;
    
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // self.render_width_marker(area, buf);
        self.render_logs(area, buf, state);
    }
}

#[derive(Default)]
pub struct App {
    vertical_scroll_pos: usize,
    logs: Vec<String>,
    pub logs_widget_state: LogsWidgetState,
}

impl App {
    pub fn scroll_down(&mut self, scroll_amount: usize) {
        self.vertical_scroll_pos = self.vertical_scroll_pos.saturating_add(scroll_amount);
    }

    pub fn scroll_up(&mut self, scroll_amount: usize) {
        self.vertical_scroll_pos = self.vertical_scroll_pos.saturating_sub(scroll_amount);
    }

    pub fn set_scroll(&mut self, scroll_pos: usize) {
      self.vertical_scroll_pos = scroll_pos;
    }

    pub fn set_log_lines(&mut self, logs: Vec<String>) {
        self.logs = logs;
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Percentage(100),
            Constraint::Min(1),
        ])
        .split(area);

        self.render_logs(frame, chunks[0]);
        
        let info_str = format!("  {}", self.logs_widget_state.actual_scroll_y.saturating_add(1));
        let title = Block::new()
            .title(Span::from("filewatch").underlined() + Span::from(info_str).blue());
        frame.render_widget(title, chunks[1]);

    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let lw = LogsWidget::new(self.logs.clone())
            .scroll(self.vertical_scroll_pos);
        frame.render_stateful_widget(lw, area, &mut self.logs_widget_state);
        self.vertical_scroll_pos = self.logs_widget_state.actual_scroll_y;
    }
}