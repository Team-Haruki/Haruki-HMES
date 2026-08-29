use std::fmt;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::registry::LookupSpan;

const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_BLUE: &str = "\x1b[34m";
const COLOR_MAGENTA: &str = "\x1b[35m";
const COLOR_RED: &str = "\x1b[31m";
const COLOR_CYAN: &str = "\x1b[36m";
const COLOR_WHITE: &str = "\x1b[37m";
const COLOR_DARK_ORANGE: &str = "\x1b[38;5;208m";
const COLOR_CYAN1: &str = "\x1b[38;5;51m";
const COLOR_DARK_SLATE_GRAY1: &str = "\x1b[38;5;123m";
const COLOR_BRIGHT_BLUE: &str = "\x1b[94m";
const COLOR_BRIGHT_MAGENTA: &str = "\x1b[95m";
const COLOR_BRIGHT_CYAN: &str = "\x1b[96m";
const COLOR_RESET: &str = "\x1b[0m";

pub fn init() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("haruki_hmes=info,warn"));
    let _ = tracing_subscriber::fmt()
        .event_format(ColoredFormatter)
        .with_env_filter(env_filter)
        .try_init();
}

struct ColoredFormatter;

impl<S, N> FormatEvent<S, N> for ColoredFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let level = level_name(metadata.level());
        let level_color = level_color(metadata.level());
        let component = component_name(metadata.target());
        let component_color = component_color(component);
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let identity_tags = visitor.identity_tags();
        let after_component = if identity_tags.is_empty() { " " } else { "" };
        let after_identity = if identity_tags.is_empty() { "" } else { " " };
        let fields = if visitor.fields.is_empty() {
            String::new()
        } else {
            format!(" {}", visitor.fields.join(" "))
        };
        let message = format!(
            "{}{}{}{}",
            COLOR_WHITE,
            visitor.message.unwrap_or_default(),
            fields,
            COLOR_RESET
        );

        writeln!(
            writer,
            "{}[{}]{}[{}{}{}][{}{}{}]{}{}{}{}",
            COLOR_DARK_SLATE_GRAY1,
            now,
            COLOR_RESET,
            level_color,
            level,
            COLOR_RESET,
            component_color,
            component,
            COLOR_RESET,
            after_component,
            identity_tags,
            after_identity,
            message
        )
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    region: Option<String>,
    user_id: Option<String>,
    fields: Vec<String>,
}

impl EventVisitor {
    fn record_value(&mut self, field: &Field, value: String) {
        match field.name() {
            "message" => {
                self.message = Some(value);
            }
            "region" | "server" | "server_region" if !value.trim().is_empty() => {
                self.region = Some(normalize_region(&value));
            }
            "user_id" if !value.trim().is_empty() => {
                self.user_id = Some(trim_debug_quotes(&value).to_string());
            }
            "log_message" => {
                self.fields.push(format!("message={value}"));
            }
            _ => {
                self.fields.push(format!("{}={}", field.name(), value));
            }
        }
    }

    fn identity_tags(&self) -> String {
        let mut tags = String::new();
        if let Some(region) = &self.region {
            tags.push_str(&format!("[{}{}{}]", COLOR_BLUE, region, COLOR_RESET));
        }
        if let Some(user_id) = &self.user_id {
            tags.push_str(&format!("[{}User-{}{}]", COLOR_CYAN1, user_id, COLOR_RESET));
        }
        tags
    }
}

impl Visit for EventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }
}

fn trim_debug_quotes(value: &str) -> &str {
    value.trim().trim_matches('"')
}

fn normalize_region(value: &str) -> String {
    match trim_debug_quotes(value) {
        "Jp" | "jp" => "JP".to_string(),
        "En" | "en" => "EN".to_string(),
        "Tw" | "tw" => "TW".to_string(),
        "Kr" | "kr" => "KR".to_string(),
        "Cn" | "cn" => "CN".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn level_name(level: &Level) -> &'static str {
    match *level {
        Level::TRACE => "TRACE",
        Level::DEBUG => "DEBUG",
        Level::INFO => "INFO",
        Level::WARN => "WARNING",
        Level::ERROR => "ERROR",
    }
}

fn level_color(level: &Level) -> &'static str {
    match *level {
        Level::TRACE => COLOR_MAGENTA,
        Level::DEBUG => COLOR_BLUE,
        Level::INFO => COLOR_GREEN,
        Level::WARN => COLOR_DARK_ORANGE,
        Level::ERROR => COLOR_RED,
    }
}

fn component_name(target: &str) -> &str {
    let mut parts = target.split("::");
    match parts.next() {
        Some("haruki_hmes") => match parts.next() {
            None => "main",
            Some("handlers") => "http",
            Some("cloud") => "cloud",
            Some("state") => "state",
            Some("config") => "config",
            Some(component) => component,
        },
        Some(component) => component,
        None => "main",
    }
}

fn component_color(component: &str) -> &'static str {
    match component {
        "main" => COLOR_BRIGHT_CYAN,
        "http" | "handlers" => COLOR_GREEN,
        "cloud" => COLOR_MAGENTA,
        "state" => COLOR_CYAN,
        "config" => COLOR_BRIGHT_MAGENTA,
        "sse" => COLOR_BRIGHT_BLUE,
        _ => COLOR_BRIGHT_CYAN,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    struct BufferWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for BufferWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Buffer {
        type Writer = BufferWriter;

        fn make_writer(&'a self) -> Self::Writer {
            BufferWriter(self.0.clone())
        }
    }

    impl Buffer {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    #[test]
    fn formatter_records_identity_and_typed_fields() {
        let output = Buffer::default();
        let subscriber = tracing_subscriber::fmt()
            .event_format(ColoredFormatter)
            .with_max_level(Level::TRACE)
            .with_writer(output.clone())
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                target: "haruki_hmes::handlers",
                region = "jp",
                user_id = 42_u64,
                active = true,
                delta = -2_i64,
                details = ?vec![1, 2],
                log_message = "extra",
                "connected"
            );
            tracing::trace!(target: "haruki_hmes", "trace event");
            tracing::debug!(target: "haruki_hmes::cloud", "debug event");
            tracing::warn!(target: "haruki_hmes::state", "warn event");
            tracing::error!(target: "external", "error event");
        });

        let rendered = output.contents();
        assert!(rendered.contains("INFO"), "{rendered:?}");
        assert!(rendered.contains("http"), "{rendered:?}");
        assert!(rendered.contains("JP"), "{rendered:?}");
        assert!(rendered.contains("User-42"), "{rendered:?}");
        assert!(rendered.contains("connected"), "{rendered:?}");
        assert!(rendered.contains("active=true"), "{rendered:?}");
        assert!(rendered.contains("delta=-2"), "{rendered:?}");
        assert!(rendered.contains("details=[1, 2]"), "{rendered:?}");
        assert!(rendered.contains("message=extra"), "{rendered:?}");
        assert!(rendered.contains("TRACE"), "{rendered:?}");
        assert!(rendered.contains("DEBUG"), "{rendered:?}");
        assert!(rendered.contains("WARNING"), "{rendered:?}");
        assert!(rendered.contains("ERROR"), "{rendered:?}");
    }

    #[test]
    fn normalizes_regions_levels_and_components() {
        for (input, expected) in [
            ("Jp", "JP"),
            ("en", "EN"),
            ("Tw", "TW"),
            ("kr", "KR"),
            ("Cn", "CN"),
            ("\"custom\"", "CUSTOM"),
        ] {
            assert_eq!(normalize_region(input), expected);
        }
        assert_eq!(trim_debug_quotes(" \"42\" "), "42");

        for (level, name, color) in [
            (Level::TRACE, "TRACE", COLOR_MAGENTA),
            (Level::DEBUG, "DEBUG", COLOR_BLUE),
            (Level::INFO, "INFO", COLOR_GREEN),
            (Level::WARN, "WARNING", COLOR_DARK_ORANGE),
            (Level::ERROR, "ERROR", COLOR_RED),
        ] {
            assert_eq!(level_name(&level), name);
            assert_eq!(level_color(&level), color);
        }

        for (target, expected) in [
            ("haruki_hmes", "main"),
            ("haruki_hmes::handlers", "http"),
            ("haruki_hmes::cloud", "cloud"),
            ("haruki_hmes::state", "state"),
            ("haruki_hmes::config", "config"),
            ("haruki_hmes::worker", "worker"),
            ("external::worker", "external"),
            ("", ""),
        ] {
            assert_eq!(component_name(target), expected);
        }

        for (component, expected) in [
            ("main", COLOR_BRIGHT_CYAN),
            ("http", COLOR_GREEN),
            ("handlers", COLOR_GREEN),
            ("cloud", COLOR_MAGENTA),
            ("state", COLOR_CYAN),
            ("config", COLOR_BRIGHT_MAGENTA),
            ("sse", COLOR_BRIGHT_BLUE),
            ("worker", COLOR_BRIGHT_CYAN),
        ] {
            assert_eq!(component_color(component), expected);
        }
    }

    #[test]
    fn identity_tags_support_empty_and_partial_values() {
        assert_eq!(EventVisitor::default().identity_tags(), "");

        let region_only = EventVisitor {
            region: Some("EN".to_string()),
            ..Default::default()
        };
        assert!(region_only.identity_tags().contains("EN"));

        let user_only = EventVisitor {
            user_id: Some("7".to_string()),
            ..Default::default()
        };
        assert!(user_only.identity_tags().contains("User-7"));
    }

    #[test]
    fn init_can_be_called_more_than_once() {
        init();
        init();
    }
}
