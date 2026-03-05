use std::fmt;
use std::sync::{Arc, Mutex};
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

pub struct BufferLayer {
    buffer: Arc<Mutex<Vec<String>>>,
    max_lines: usize,
}

impl BufferLayer {
    pub fn new(buffer: Arc<Mutex<Vec<String>>>, max_lines: usize) -> Self {
        Self { buffer, max_lines }
    }
}

impl<S> Layer<S> for BufferLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = StringVisitor::new();
        event.record(&mut visitor);

        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let message = format!("[{}] {} {}", timestamp, event.metadata().level(), visitor.0);

        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= self.max_lines {
                buf.remove(0);
            }
            buf.push(message);
        }
    }
}

struct StringVisitor(String);

impl StringVisitor {
    fn new() -> Self {
        Self(String::new())
    }
}

impl tracing::field::Visit for StringVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(format!("{:?}", value).trim_matches('"'));
        } else {
            // self.0.push_str(&format!(" {}={:?}", field.name(), value));
        }
    }
}
