mod file_watch;
mod ui;

use std::{fs, sync, usize};
use std::path::PathBuf;
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

use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{self, KeyCode};

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
            if let Err(e) = file_watch::watch_file(&path, tx_clone) {
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
    let mut app = ui::App::default();
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|frame| app.render(frame)).expect("draw should work");
        let page_size = app.logs_widget_state.height;
        let elapsed_time = last_tick.elapsed();
        let timeout = tick_rate.saturating_sub(elapsed_time);
        if event::poll(timeout).expect("bad poll") {
            log::debug!("event recived");
            if let Some(key) = event::read().unwrap().as_key_press_event() {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('g') => app.set_scroll(usize::MAX),
                    KeyCode::Char('j') | KeyCode::Down => app.scroll_down(1),
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_up(1),
                    KeyCode::PageUp => app.scroll_up(page_size.into()),
                    KeyCode::PageDown => app.scroll_down(page_size.into()),
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
