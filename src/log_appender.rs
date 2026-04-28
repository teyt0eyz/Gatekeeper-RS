// log_appender.rs — Daily-rotating file appender that uses *local* time.
//
// Why this exists: tracing_appender's built-in RollingFileAppender rotates
// on UTC midnight, so users in UTC+N timezones see "today's" log file
// stamped with yesterday's date until UTC midnight (e.g. UTC+7 has to wait
// until 07:00 local for the date in the filename to advance).
//
// This appender re-checks chrono::Local on every write and re-opens the
// output file when the local date changes. Because tracing_appender's
// `non_blocking` worker is single-threaded, the writer doesn't need any
// internal locking.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

pub struct LocalDailyAppender {
    dir:    PathBuf,
    prefix: String,
    suffix: String,
    /// (yyyy-mm-dd of currently open file, file handle)
    state:  Option<(String, File)>,
}

impl LocalDailyAppender {
    pub fn new(dir: impl Into<PathBuf>, prefix: &str, suffix: &str) -> Self {
        Self {
            dir:    dir.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            state:  None,
        }
    }

    fn ensure_today(&mut self) -> io::Result<&mut File> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let needs_open = !matches!(&self.state, Some((d, _)) if d == &today);
        if needs_open {
            let path = self.dir.join(format!(
                "{}.{}.{}",
                self.prefix, today, self.suffix,
            ));
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            self.state = Some((today, file));
        }
        Ok(&mut self.state.as_mut().unwrap().1)
    }
}

impl Write for LocalDailyAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.ensure_today()?.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some((_, f)) = &mut self.state {
            f.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_to_local_dated_filename() {
        let dir = std::env::temp_dir().join(format!(
            "gk-appender-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut a = LocalDailyAppender::new(&dir, "gatekeeper", "log");
        a.write_all(b"hello\n").unwrap();
        a.flush().unwrap();

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let expected = dir.join(format!("gatekeeper.{today}.log"));
        assert!(
            expected.exists(),
            "expected log file at {expected:?}"
        );
        assert_eq!(std::fs::read_to_string(&expected).unwrap(), "hello\n");

        std::fs::remove_dir_all(&dir).ok();
    }
}
