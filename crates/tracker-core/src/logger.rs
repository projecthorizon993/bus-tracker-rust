use log::{Level, Metadata, Record};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

/// File logger that writes all log output to `log.txt` in the current directory.
pub struct FileLogger {
    file: Mutex<std::fs::File>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut file = self.file.lock().unwrap();
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(
                file,
                "[{}][{}] {}",
                timestamp,
                record.level(),
                record.args()
            );
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        let mut file = self.file.lock().unwrap();
        let _ = file.flush();
    }
}

/// Initialize file logging to `log.txt` in the current working directory.
pub fn init_file_logger() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::current_dir()?.join("log.txt");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let logger = Box::new(FileLogger {
        file: Mutex::new(file),
    });
    log::set_boxed_logger(logger)?;
    log::set_max_level(log::LevelFilter::Debug);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_logger_creation() {
        // Just verify the module compiles
        assert!(true);
    }
}
