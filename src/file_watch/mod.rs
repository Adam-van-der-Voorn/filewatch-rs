use std::fs::File;
use std::io::{BufRead, BufReader, Seek};
use std::{fs, io};
use std::sync::mpsc::Sender;

use notify::{INotifyWatcher, RecommendedWatcher, RecursiveMode, Watcher};

pub struct LogsMessage {
    pub lines: Vec<String>,
    pub file_id: String,
}

pub fn watch_file(path: &String, tx: Sender<LogsMessage>) -> Result<INotifyWatcher, io::Error> {
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
                Err(_) => { log::error!("File event handler {} failed to send", &id); 0 }
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


struct FileEventHandler {
    id: String,
    tx: Sender<LogsMessage>,
    file_handle: File,
    last_read_file_pos: u64
}

impl notify::EventHandler for FileEventHandler {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        if !should_handle_event(&event) {
            log::debug!("Skip Event: {:?}", event);
            return;
        }
        log::debug!("Event: {:?}", event);
        let pos = self.last_read_file_pos;
        // ignore any event that didn't change the pos
        let file_len = self.file_handle.metadata().unwrap().len();
        if file_len == pos {
            log::debug!("Ignoring event as file length = cursor position");
        }
        else if file_len < pos {
            let msg = LogsMessage {
                file_id: self.id.clone(),
                lines: vec![format!("filewatch: File truncated to position {file_len}")],
            };
            match self.tx.send(msg) {
                Ok(_) => { /* noop */ },
                Err(_) => log::error!("File event handler {} failed to send (meta)", &self.id)
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
                    Err(_) => log::error!("File event handler {} failed to send", &self.id)
                }
            }        
        }
    }
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
            log::error!("Event error: {:?}", error);
            return false;
        }
    }
}

fn get_lines_for_interval(file_handle: &mut File, start_pos: u64, end_pos: u64) -> Option<Vec<String>> {
    if start_pos > end_pos {
        log::info!("will not read file, start pos ({start_pos}) > end pos ({end_pos})");
        return Option::Some(vec![]);
    }

    log::debug!("Reading from position {} to {}", start_pos, end_pos);

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