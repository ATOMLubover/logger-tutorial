use std::sync::{Arc, Mutex};

use logger_demo::Appender;
use logger_demo::Level;
use logger_demo::Logger;
use logger_demo::info;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("log output is utf-8")
    }
}

impl Appender for Capture {
    fn append(&self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
}

#[test]
fn info_macro_expands_from_another_crate() {
    let capture = Capture::default();
    let logger = Logger::new(Level::Info, vec![Box::new(capture.clone())]);

    info!(logger, "hello {}", 42);

    assert!(
        capture.text().ends_with(" [INFO] hello 42\n"),
        "unexpected output: {:?}",
        capture.text()
    );
}
