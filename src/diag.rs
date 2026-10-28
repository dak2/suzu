use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, path: &str, source: &str) -> String {
        let start = (self.span.start as usize).min(source.len());
        let (line_no, line_start) = line_of(source, start);
        let line_end = source[line_start..]
            .find('\n')
            .map_or(source.len(), |i| line_start + i);
        let line_end = if source[..line_end].ends_with('\r') {
            line_end - 1
        } else {
            line_end
        };
        let start = start.min(line_end);
        let line = &source[line_start..line_end];
        let col = source[line_start..start].chars().count() + 1;
        let end = (self.span.end as usize).clamp(start, line_end);
        let width = source[start..end]
            .chars()
            .map(display_width)
            .sum::<usize>()
            .max(1);

        let gutter = line_no.to_string().len();
        let pad = " ".repeat(gutter);
        let indent: String = source[line_start..start]
            .chars()
            .map(|c| match c {
                '\t' => "\t".to_string(),
                c => " ".repeat(display_width(c)),
            })
            .collect();
        format!(
            "error: {msg}\n{pad}--> {path}:{line_no}:{col}\n{pad} |\n{line_no} | {line}\n{pad} | {indent}{carets}\n",
            msg = self.message,
            carets = "^".repeat(width),
        )
    }
}

fn display_width(c: char) -> usize {
    // Not the `unicode-width` crate: the compiler has no dependencies, and the
    // common East Asian wide ranges are enough to line up a caret.
    match c as u32 {
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

fn line_of(source: &str, pos: usize) -> (usize, usize) {
    let before = &source[..pos];
    let line_no = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line_no, line_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_line_and_caret() {
        let src = "fn main() {\n    foo(1 +)\n}\n";
        let d = Diagnostic::error("expected expression", Span::new(23, 24));
        assert_eq!(
            d.render("a.sz", src),
            "error: expected expression\n --> a.sz:2:12\n  |\n2 |     foo(1 +)\n  |            ^\n"
        );
    }

    #[test]
    fn span_on_the_newline_of_a_crlf_line_does_not_panic() {
        let src = "fn f()\r\n{\r\n}\r\n";
        let at = src.find('\n').unwrap();
        let d = Diagnostic::error("expected `{`", Span::new(at, at + 1));
        assert_eq!(
            d.render("a.sz", src),
            "error: expected `{`\n --> a.sz:1:7\n  |\n1 | fn f()\n  |       ^\n"
        );
    }

    #[test]
    fn span_at_eof_after_a_trailing_cr_does_not_panic() {
        let src = "fn f() {\r";
        let d = Diagnostic::error("expected `}`", Span::new(src.len(), src.len()));
        d.render("a.sz", src);
    }

    #[test]
    fn caret_accounts_for_wide_characters() {
        let src = "f(\"あいう\", @)";
        let at = src.find('@').unwrap();
        let d = Diagnostic::error("unexpected character `@`", Span::new(at, at + 1));
        let rendered = d.render("a.sz", src);
        let caret_line = rendered.lines().last().unwrap();
        assert_eq!(caret_line, format!("  | {}^", " ".repeat(12)));
    }

    #[test]
    fn caret_under_a_wide_character_is_two_columns() {
        let src = "x = あ";
        let d = Diagnostic::error("unexpected character `あ`", Span::new(4, 7));
        assert!(d.render("a.sz", src).ends_with("  |     ^^\n"));
    }

    #[test]
    fn span_at_end_of_file() {
        let src = "fn f(";
        let d = Diagnostic::error("unexpected end of file", Span::new(5, 5));
        assert!(d.render("a.sz", src).contains("1:6"));
    }

    #[test]
    fn crlf_line_endings_do_not_reach_the_snippet() {
        let src = "x = @\r\ny\r\n";
        let d = Diagnostic::error("unexpected character `@`", Span::new(4, 6));
        assert_eq!(
            d.render("a.sz", src),
            "error: unexpected character `@`\n --> a.sz:1:5\n  |\n1 | x = @\n  |     ^\n"
        );
    }
}
