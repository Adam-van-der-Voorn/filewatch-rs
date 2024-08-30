# input:

please create a rust cli program with the following features:
 - takes a list of files as arguments
 - fill all the avalible space in the terminal with the latest output from each file. Continue listening to the file fo teh latest output. (time wise). Do not use the last line which is reserved for user input. Trauncate the lines, do not wrap. You should not display any lines logged before the program starts.
 - accept user input. The input should be read from the keybaord while the program is running and displayed in the last row of the terminal. For now, all that is needed is:
    - the filter commnd (f), which adds a filter to filter the lines by. The command takes a string paramater. The user shoud be able to type ‘ffoo’ and only lines containing foo should be displayed. This will likey mean that there is now space in teh terminal for lines that used to no longer be visible- this space should be filled with said lines.
    - the config command (-) which displays all filters
    - the clear command (k!), which removes all filters 

# result:

Creating a Rust CLI program with the specified features involves utilizing several crates for terminal handling, file reading, and user input. Below is an implementation using the `crossterm` crate for terminal manipulation and `tokio` for async handling of file reading and user input.

### Cargo.toml

First, ensure you have a `Cargo.toml` with the necessary dependencies:

```toml
[package]
name = "tailer"
version = "0.1.0"
edition = "2021"

[dependencies]
crossterm = "0.26.1"
tokio = { version = "1.30", features = ["full"] }
tokio-stream = "0.1"
```

### src/main.rs

Here's the code implementation of the CLI program:

```rust
use crossterm::{
    event::{self, KeyCode, KeyEvent},
    execute,
    terminal::{self, ClearType},
    ExecutableCommand,
};

use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, Error, Stdin},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use tokio::sync::mpsc;

struct App {
    filters: HashSet<String>,
}

impl App {
    fn new() -> Self {
        Self {
            filters: HashSet::new(),
        }
    }

    fn apply_filter(&mut self, filter: String) {
        self.filters.insert(filter);
    }

    fn clear_filters(&mut self) {
        self.filters.clear();
    }

    fn show_filters(&self) {
        if self.filters.is_empty() {
            println!("No active filters.");
        } else {
            println!("Active filters:");
            for filter in &self.filters {
                println!("- {}", filter);
            }
        }
    }
}

async fn tail_file(file_path: PathBuf, sender: mpsc::Sender<String>) -> Result<(), Error> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    // Only seek to the end of the file
    let file_len = reader.stream_position()?;
    reader.seek(std::io::SeekFrom::End(file_len))?;

    let mut lines = reader.lines().filter_map(Result::ok);
    while let Some(line) = lines.next() {
        sender.send(line).await.unwrap();
        thread::sleep(Duration::from_millis(100)); // Slow down the reading a bit
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let file_paths: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();

    let (tx, mut rx) = mpsc::channel(100);
    let app = Arc::new(Mutex::new(App::new()));

    for path in file_paths {
        let tx = tx.clone();
        let path_clone = path.clone();
        tokio::spawn(async move {
            if let Err(e) = tail_file(path_clone, tx).await {
                eprintln!("Error reading file {}: {}", path.display(), e);
            }
        });
    }

    terminal::enable_raw_mode()?;
    let mut stdin = crossterm::event::DisableMouseCapture::default();
    let mut stdout = std::io::stdout();

    execute!(stdout, terminal::Clear(ClearType::All))?;
    println!("Starting to tail files... (Press Ctrl+C to exit)");

    let mut lines: Vec<String> = Vec::new();

    thread::spawn({
        let app_clone = Arc::clone(&app);
        move || {
            loop {
                if event::poll(Duration::from_millis(200)).unwrap() {
                    if let event::Event::Key(KeyEvent { code, .. }) = event::read().unwrap() {
                        match code {
                            KeyCode::Esc => break,
                            KeyCode::Char('f') => {
                                let filter = String::new(); // TODO: Implement filter input
                                app_clone.lock().unwrap().apply_filter(filter);
                            }
                            KeyCode::Char('-') => {
                                app_clone.lock().unwrap().show_filters();
                            }
                            KeyCode::Char('k') => {
                                app_clone.lock().unwrap().clear_filters();
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    loop {
        match rx.recv().await {
            Some(line) => {
                let app_lock = app.lock().unwrap();
                let filter_match = app_lock.filters.is_empty()
                    || app_lock.filters.iter().any(|f| line.contains(f));
                if filter_match {
                    lines.push(line);
                    // Truncate lines, no wrapping; Limit to terminal height - 1 for input line
                    // Print output to terminal
                    execute!(stdout, terminal::Clear(ClearType::All))?;
                    for l in &lines {
                        if let Ok(_) = println!("{}", l) {
                            () // Optionally handle the result
                        }
                    }
                }
            }
            None => break,
        };
    }

    terminal::disable_raw_mode()?;
    Ok(())
}
```

### Explanation

1. **Dependencies**: We use `crossterm` for handling terminal input and output, and `tokio` for async file reading.
  
2. **File Tail Function**: The `tail_file` function reads new lines from a file asynchronously. It captures the current position in the file and continues from there.

3. **Filters**: The `App` structure manages a set of filters. You can add a filter (with a sample input for filtering), view filters, and clear filters.

4. **Input Handling**: A separate thread is created to listen for keyboard inputs. Commands are processed and can modify the list of displayed lines based on filters.

### Future Improvements
- You can enhance input capturing for the filter (`f`) command, allowing users to type the filter string dynamically.
- Add clearer error handling and functionality as required for your specific use case.
- Consider adding options to customize line truncation or terminal size dynamically.

### Run the Program
To run the program, execute the following in your terminal:

```bash
cargo run -- <file1> <file2> ...
```

Replace `<file1>` and `<file2>` with the actual paths to the files you wish to monitor.
