use std::{
    fs::{self, File},
    fmt::Write,
    io::{self, BufRead, BufReader, Error, Seek},
    sync::{self, mpsc::Sender}, 
    thread,
    time::SystemTime,
    path::PathBuf,
};

use notify::{self, INotifyWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use minus::{dynamic_paging, Pager, LineNumbers};
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
    assert!(start_pos < end_pos);

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

fn watch_file(path: &String, tx: Sender<LogsMessage>) -> Result<INotifyWatcher, Error> {
    let file_handle = fs::File::open(path)
        .unwrap();
    let event_handler = FileEventHandler {
        file_handle, tx,
        id: path.clone(),
        last_read_file_pos: 0,
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

    // Initialize pager with line numbers
    let pager = Pager::new();
    pager.set_line_numbers(LineNumbers::Enabled).unwrap();
    
    // Clone pager for the dynamic paging thread
    let pager_clone = pager.clone();
    
    // Start the pager in a separate thread
    info!("Starting pager");
    let _pager_thread = thread::spawn(move || {
        if let Err(e) = dynamic_paging(pager_clone) {
            error!("Pager error: {}", e);
        }
    });
    
    // watch files for changes
    for msg in rx {
        for line in msg.lines.into_iter() {
            let insert_result = insert.execute((&msg.file_id, line));
            if let Err(err) = insert_result {
                error!("Failed to insert to database ({:?}): {:?}", err.sqlite_error_code(), err.sqlite_error());
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
        let mut log_content = String::new();
        for log_result in logs {
            if let Ok(line) = log_result {
                writeln!(&mut log_content, "{}", line).unwrap();
            }
        }
        
        // Update the pager with the new content
        pager.set_text(&log_content).unwrap();
    }
}
