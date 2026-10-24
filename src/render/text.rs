//! Stream escaped Markdown without retaining expanded copies of source prose.

use crate::presentations::BoundedText;
use std::fmt::{self, Display, Write};

pub(super) struct Fragment<F>(F);

pub(super) fn fragment<F>(write: F) -> Fragment<F>
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    Fragment(write)
}

impl<F> Display for Fragment<F>
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0(formatter)
    }
}

struct Checked<'a>(&'a mut BoundedText);

impl Write for Checked<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.push_str(value);
        if self.0.failed() {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

struct Escaping<'a, W: Write + ?Sized> {
    output: &'a mut W,
    newline: &'static str,
    previous_cr: bool,
}

impl<W: Write + ?Sized> Write for Escaping<'_, W> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let mut start = 0;
        for (offset, character) in value.char_indices() {
            let replacement = match character {
                '&' => Some("&amp;"),
                '<' => Some("&lt;"),
                '>' => Some("&gt;"),
                '\\' => Some("\\\\"),
                '|' => Some("\\|"),
                '`' => Some("&#96;"),
                '\r' => Some(self.newline),
                '\n' if self.previous_cr => Some(""),
                '\n' => Some(self.newline),
                _ => None,
            };
            self.previous_cr = character == '\r';
            if let Some(replacement) = replacement {
                self.output.write_str(&value[start..offset])?;
                self.output.write_str(replacement)?;
                start = offset + character.len_utf8();
            }
        }
        self.output.write_str(&value[start..])
    }
}

pub(super) fn cell<T: Display + ?Sized>(output: &mut BoundedText, value: &T) {
    escaped(output, value, "<br>");
}

pub(super) fn prose_into<T: Display + ?Sized>(output: &mut BoundedText, value: &T) {
    escaped(output, value, " ");
}

fn escaped<T: Display + ?Sized>(output: &mut BoundedText, value: &T, newline: &'static str) {
    if output.failed() {
        return;
    }
    let mut checked = Checked(output);
    let mut escaped = Escaping {
        output: &mut checked,
        newline,
        previous_cr: false,
    };
    let _ = write!(escaped, "{value}");
}

pub(super) struct Prose<T>(T);

pub(super) fn prose<T: Display>(value: T) -> Prose<T> {
    Prose(value)
}

impl<T: Display> Display for Prose<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut escaped = Escaping {
            output: formatter,
            newline: " ",
            previous_cr: false,
        };
        write!(escaped, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn escaping_preserves_crlf_across_formatting_chunks() {
        let chunks = fragment(|s| {
            s.write_str("&<|`\\α\r")?;
            s.write_str("\nβ\rc\n")
        });
        let mut output = BoundedText::new(1024);
        cell(&mut output, &chunks);
        assert_eq!(
            output.finish().unwrap(),
            "&amp;&lt;\\|&#96;\\\\α<br>β<br>c<br>"
        );
        assert_eq!(prose(chunks).to_string(), "&amp;&lt;\\|&#96;\\\\α β c ");
    }

    #[test]
    fn a_failed_sink_stops_expanding_the_remaining_fragments() {
        let visited = Cell::new(0);
        let repeated = fragment(|s| {
            for _ in 0..10_000 {
                visited.set(visited.get() + 1);
                s.write_str("amplified evidence")?;
            }
            Ok(())
        });
        let mut output = BoundedText::new(8);
        cell(&mut output, &repeated);
        assert!(output.failed());
        assert_eq!(visited.get(), 1);
        prose_into(&mut output, &repeated);
        assert_eq!(visited.get(), 1);
    }
}
