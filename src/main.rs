use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Error, Seek},
    sync::{self, mpsc::Sender}, time::SystemTime,
};

use notify::{self, INotifyWatcher, RecommendedWatcher, RecursiveMode, Watcher};

struct FileEventHandler {
    id: String,
    tx: Sender<LogsMessage>,
    file_handle: File,
    last_read_file_pos: u64
}

impl notify::EventHandler for FileEventHandler {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        if !should_read_file(&event) {
            return;
        }
        eprintln!("event: {:?}", event);
        let pos = self.last_read_file_pos;
        let result = get_lines_from_position(&mut self.file_handle, pos);
        if let Some(a) = result {
            let msg = LogsMessage {
                file_id: self.id.clone(),
                lines: a.lines,
            };
            match self.tx.send(msg) {
                Ok(_) => { self.last_read_file_pos = a.new_file_len },
                Err(_) => eprintln!("file event handler {} failed to send", &self.id)
            }
        }        
    }
}

struct LogsMessage {
    lines: Vec<String>,
    file_id: String,
}

struct GetLinesResult {
    lines: Vec<String>,
    new_file_len: u64
}

fn should_read_file(event: &notify::Result<notify::Event>) -> bool {
    match event {
        Ok(_event) => {
            use notify::{event::*, EventKind::*};
            if _event.kind != Access(AccessKind::Close(AccessMode::Write)) {
                // ignore any event that is not a close write
                // i.e. we don't care about any "intermediate" write states, just the final one
                return false;
            }
            return true;
        }
        Err(error) => {
            eprintln!("event error: {:?}", error);
            return false;
        }
    }
}

fn get_lines_from_position(file_handle: &mut File, start_pos: u64) -> Option<GetLinesResult> {
    let file_len = file_handle.metadata().unwrap().len();
    eprintln!("read from {} to {}", start_pos, file_len);

    // ignore any event that didn't change the pos
    if file_len == start_pos {
        eprintln!("ignore event as file length = cursor pos");
        return Option::None;
    }

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
    Option::Some(GetLinesResult {
        lines: lines,
        new_file_len: file_len
    })
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
    let file_paths: Vec<String> = std::env::args().skip(1).collect();
    // let watchers = vec![];
    let (tx, rx) = sync::mpsc::channel();

    for path in file_paths {
        let tx_clone = tx.clone();        
        std::thread::spawn(move || {
            if let Err(e) = watch_file(&path, tx_clone) {
                eprintln!("Error tailing file {}: {}", &path, e);
            }
        });
    }

    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();

    let conn = rusqlite::Connection::open(format!("./db/{}.db3", ts))
        .expect("failed to open db");

    conn.execute(
        "CREATE TABLE log ( id INTEGER PRIMARY KEY, file_id TEXT NOT NULL, message TEXT NOT NULL )",
        (),
    )
        .unwrap();

    let mut query = conn.prepare("select file_id, message from log")
        .unwrap();

    let mut insert = conn.prepare("INSERT INTO log (file_id, message) VALUES (?, ?)")
        .unwrap();

    // watch
    for msg in rx {
        for line in msg.lines.into_iter() {
            let insert_result = insert.execute((&msg.file_id, line));
            if let Err(err) = insert_result {
                eprintln!("failed to insert ({:?}): {:?}", err.sqlite_error_code(), err.sqlite_error());
            }
        }
        
        let logs = query
            .query_map([], |row| {
                let file_id: String = row.get("file_id").unwrap();
                let message: String = row.get("message").unwrap();
                let line = format!("{}: {}", file_id, message);
                Ok(line)
            })
            .unwrap();
        println!("CURRENT:");
        logs.for_each(|msg| println!(">> {}", msg.unwrap()))
    }
}
