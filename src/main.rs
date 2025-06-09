use std::{
    fs::{self, File}, io::{self, BufRead, BufReader, Seek}, path::PathBuf, sync::{self, mpsc::Sender}, time::SystemTime, usize
};

use notify::{self, INotifyWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use log::{debug, error, info, LevelFilter};
use simplelog::{CombinedLogger, Config, TermLogger, WriteLogger, TerminalMode, ColorChoice};
use clap::Parser;

/// A file watcher and log aggregator
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Files to watch
    #[clap(required = true)]
    files: Vec<String>,
    
    /// Enable debug logging to a file (default: filewatch.log)
    #[clap(short = 'o', long)]
    debug_output: Option<PathBuf>,
}

use std::time::{Duration, Instant};

use crossterm::event::{self, KeyCode};
use ratatui::{buffer::Buffer, layout::{Constraint, Layout, Rect}, style::Style, text::Span, widgets::StatefulWidget};
use ratatui::style::{Stylize};
use ratatui::widgets::{Block};
use ratatui::Frame;

struct LogsWidget {
    pub logs: Vec<String>,
    pub scroll_y: usize,
}

#[derive(Default)]
struct LogsWidgetState {
    pub actual_scroll_y: usize,
    pub was_at_bottom: bool,
    pub last_log_count: usize,
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
        let (log_idx, char_offset, scroll_y_actual, at_bottom) = self.get_page_index(area, scroll_y);
        
        // Update state
        state.actual_scroll_y = scroll_y_actual;
        state.last_log_count = input_log_count;
        state.was_at_bottom = at_bottom;

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
    /// * `area` - The rendering area containing width and height information
    /// * `scroll_y` - The line to start from
    /// 
    /// # Returns
    /// A tuple `(log_index, char_offset, line_offset)` where:
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
    /// If scroll_y=3, this would return (2, 10, 3) meaning start at log 2,
    /// skip 10 characters (start from "message here").
    fn get_page_index(&self, area: Rect, scroll_y: usize) -> (usize, usize, usize, bool) {
        let width: usize = area.width.into();
        let target_line = scroll_y;
        
        let mut current_line = 0;
        
        for (log_idx, log) in self.logs.iter().enumerate() {
            let is_last_log = log_idx == self.logs.len() - 1;
            let log_chars = log.chars().count();
            let lines_for_this_log = if log_chars == 0 { 1 } else { (log_chars + width - 1) / width };
            if current_line + lines_for_this_log > target_line {
                let lines_into_log = target_line - current_line;
                let char_offset = lines_into_log * width;
                let at_bottom = is_last_log && lines_into_log == lines_for_this_log.saturating_sub(1);
                return (log_idx, char_offset, current_line + lines_into_log, at_bottom);
            }
            else if is_last_log {
                // user has scrolled below last line
                let lines_into_log = lines_for_this_log.saturating_sub(1);
                let char_offset = lines_into_log * width;
                let at_bottom = true;
                return (log_idx, char_offset, current_line + lines_into_log, at_bottom);
            }
            
            current_line += lines_for_this_log;
        }
        
        // no logs
        (0, 0, 0, true)
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
struct App {
    pub vertical_scroll_pos: usize,
    pub logs: Vec<String>,
    pub logs_widget_state: LogsWidgetState,
}

impl App {
    fn scroll_down(&mut self) {
        self.vertical_scroll_pos = self.vertical_scroll_pos.saturating_add(1);
    }

    fn scroll_to_bottom(&mut self) {
        self.vertical_scroll_pos = usize::MAX;
    }

    const fn scroll_up(&mut self) {
        self.vertical_scroll_pos = self.vertical_scroll_pos.saturating_sub(1);
    }

    fn set_log_lines(&mut self, logs: Vec<String>) {
        self.logs = logs;
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Percentage(100),
            Constraint::Min(1),
        ])
        .split(area);

        let title = Block::new()
            .title(Span::from("filewatch").underlined() + Span::from("  Use j k or ▲ ▼ to scroll").blue());
        frame.render_widget(title, chunks[1]);
        self.render_logs(frame, chunks[0])

    }

    fn render_logs(&mut self, frame: &mut Frame, area: Rect) {
        let lw = LogsWidget::new(self.logs.clone())
            .scroll(self.vertical_scroll_pos);
        frame.render_stateful_widget(lw, area, &mut self.logs_widget_state);
        self.vertical_scroll_pos = self.logs_widget_state.actual_scroll_y;
    }
}


struct FileEventHandler {
    id: String,
    tx: Sender<LogsMessage>,
    file_handle: File,
    last_read_file_pos: u64
}

impl notify::EventHandler for FileEventHandler {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        if !should_handle_event(&event) {
            debug!("Skip Event: {:?}", event);
            return;
        }
        debug!("Event: {:?}", event);
        let pos = self.last_read_file_pos;
        // ignore any event that didn't change the pos
        let file_len = self.file_handle.metadata().unwrap().len();
        if file_len == pos {
            debug!("Ignoring event as file length = cursor position");
        }
        else if file_len < pos {
            let msg = LogsMessage {
                file_id: self.id.clone(),
                lines: vec![format!("filewatch: File truncated to position {file_len}")],
            };
            match self.tx.send(msg) {
                Ok(_) => { /* noop */ },
                Err(_) => error!("File event handler {} failed to send (meta)", &self.id)
            }
            self.last_read_file_pos = file_len;
        }
        else {
            let result = get_lines_for_interval(&mut self.file_handle, pos, file_len);
            if let Some(lines) = result {
                let msg = LogsMessage {
                    file_id: self.id.clone(),
                    lines: lines,
                };
                match self.tx.send(msg) {
                    Ok(_) => { self.last_read_file_pos = file_len },
                    Err(_) => error!("File event handler {} failed to send", &self.id)
                }
            }        
        }
    }
}

struct LogsMessage {
    lines: Vec<String>,
    file_id: String,
}


fn should_handle_event(event_res: &notify::Result<notify::Event>) -> bool {
    match event_res {
        Ok(event) => {
            use notify::{event::*};
            match event.kind {
                EventKind::Modify(kind) => {
                    kind != ModifyKind::Metadata(MetadataKind::Any) &&
                    kind != ModifyKind::Name(RenameMode::Any)
                    },
                _ => false

            }
        }
        Err(error) => {
            error!("Event error: {:?}", error);
            return false;
        }
    }
}

fn get_lines_for_interval(file_handle: &mut File, start_pos: u64, end_pos: u64) -> Option<Vec<String>> {
    if start_pos > end_pos {
        log::info!("will not read file, start pos ({start_pos}) > end pos ({end_pos})");
        return Option::Some(vec![]);
    }

    debug!("Reading from position {} to {}", start_pos, end_pos);

    // read from pos to end of file
    let mut lines = Vec::new();
    file_handle.seek(io::SeekFrom::Start(start_pos)).unwrap();
    let reader = BufReader::new(file_handle);
    for line_res in reader.lines() {
        let line = line_res.unwrap();
        if line.len() == 0 {
            continue;
        }
        // else parse line, add to db?
        lines.push(line)
    }
    Option::Some(lines)
}

fn watch_file(path: &String, tx: Sender<LogsMessage>) -> Result<INotifyWatcher, io::Error> {
    let mut file_handle = fs::File::open(path)
        .unwrap();
    let id = path.clone();

    // first event, read existing file
    let file_len = file_handle.metadata().unwrap().len();
    let result = get_lines_for_interval(&mut file_handle, 0, file_len);
    let last_read = match result {
        Some(lines) => {
            let msg = LogsMessage {
                file_id: id.clone(),
                lines: lines,
            };
            match tx.send(msg) {
                Ok(_) => { file_len },
                Err(_) => { error!("File event handler {} failed to send", &id); 0 }
            }
        }
        None => 0   
    };

    let event_handler = FileEventHandler {
        file_handle, tx,
        id: id,
        last_read_file_pos: last_read,
    };

    let mut watcher = RecommendedWatcher::new(event_handler, notify::Config::default())
        .unwrap();
    watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)
        .unwrap();

    loop {}
}

fn main() -> () {
    // Parse command line arguments
    let args = Args::parse();
    
    // Configure logger based on debug_output option
    if let Some(log_path) = &args.debug_output {
        // Open existing file in append mode or create if it doesn't exist
        let log_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(log_path)
            .expect("Failed to open log file");
        
        CombinedLogger::init(vec![
            // Terminal logger is turned off to keep terminal clean for the pager
            TermLogger::new(LevelFilter::Off, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            // File logger with debug level
            WriteLogger::new(LevelFilter::Debug, Config::default(), log_file),
        ]).unwrap();
        
        info!("Debug logging enabled to file: {}", log_path.display());
    } else {
        // Initialize with Off level to suppress all output
        CombinedLogger::init(vec![
            TermLogger::new(LevelFilter::Off, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        ]).unwrap();
    }
    
    // Use the files from parsed arguments
    let file_paths = args.files;
    info!("Watching files: {:?}", file_paths);
    
    // let watchers = vec![];
    let (tx, rx) = sync::mpsc::channel();

    for path in file_paths {
        let tx_clone = tx.clone();        
        std::thread::spawn(move || {
            if let Err(e) = watch_file(&path, tx_clone) {
                error!("Error tailing file {}: {}", &path, e);
            }
        });
    }

    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();
    
    let db_path = format!("./db/{}.db3", ts);
    debug!("Creating database at {}", db_path);
    
    let conn = rusqlite::Connection::open(&db_path)
        .expect("failed to open db");
    
    debug!("Database opened successfully");

    conn.execute(
        "CREATE TABLE log ( id INTEGER PRIMARY KEY, file_id TEXT NOT NULL, message TEXT NOT NULL )",
        (),
    )
        .unwrap();

    let mut query = conn.prepare("select file_id, message from log")
        .unwrap();

    let mut insert = conn.prepare("INSERT INTO log (file_id, message) VALUES (?, ?)")
        .unwrap();

    let mut terminal = ratatui::init();
    let mut app = App::default();
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|frame| app.render(frame)).expect("draw should work");
        let elapsed_time = last_tick.elapsed();
        let timeout = tick_rate.saturating_sub(elapsed_time);
        if event::poll(timeout).expect("bad poll") {
            log::debug!("event recived");
            if let Some(key) = event::read().unwrap().as_key_press_event() {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('g') => app.scroll_to_bottom(),
                    KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
                    _ => {}
                }
            }
        }

        //hmm
        let iter = rx.try_iter();
        for msg in iter {
            // Insert new rows
            for line in msg.lines.into_iter() {
                let insert_result = insert.execute((&msg.file_id, line));
                if let Err(err) = insert_result {
                    error!("Failed to insert to database ({:?}): {:?}", err.sqlite_error_code(), err.sqlite_error());
                }
            }
        }

        // Query all logs from database
        let logs = query
            .query_map([], |row| {
                let file_id: String = row.get("file_id").unwrap();
                let message: String = row.get("message").unwrap();
                let line = format!("{}: {}", file_id, message);
                Ok(line)
            })
            .unwrap();
        
        // Collect all log lines into a single string
        let mut log_content = vec![];
        for log_result in logs {
            if let Ok(line) = log_result {
                log_content.push(line)
            }
            else {
                log::error!("bad log")
            }
        }

        app.set_log_lines(log_content);
        last_tick = Instant::now();

    }
    ratatui::restore();
}
