use crate::diag::Diagnostic;
use crate::span::Span;

const MAX_INTERP_DEPTH: u32 = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub depth: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart {
    Text(String),
    Interp(Vec<Token>),
}

macro_rules! keywords {
    ($($variant:ident => $text:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Keyword {
            $($variant,)*
        }

        impl Keyword {
            pub fn lookup(s: &str) -> Option<Keyword> {
                match s {
                    $($text => Some(Keyword::$variant),)*
                    _ => None,
                }
            }

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Keyword::$variant => $text,)*
                }
            }
        }
    };
}

keywords! {
    Fn => "fn",
    Struct => "struct",
    Enum => "enum",
    Impl => "impl",
    Const => "const",
    Pub => "pub",
    Var => "var",
    With => "with",
    If => "if",
    Else => "else",
    While => "while",
    For => "for",
    In => "in",
    Match => "match",
    Return => "return",
    Break => "break",
    Continue => "continue",
    Mut => "mut",
    Take => "take",
    Borrow => "borrow",
    SelfValue => "self",
    Block => "block",
    Where => "where",
    Derives => "derives",
    Layout => "layout",
    Scope => "scope",
    Co => "co",
    True => "true",
    False => "false",
}

macro_rules! token_kinds {
    (@continues continues) => { true };
    (@continues) => { false };
    ($($variant:ident => $text:literal $(, $cont:ident)?;)*) => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum TokenKind {
            Ident(String),
            Int(u64),
            Float(f64),
            Str(Vec<StrPart>),
            Kw(Keyword),
            Newline,
            Eof,
            $($variant,)*
        }

        impl TokenKind {
            pub fn describe(&self) -> String {
                match self {
                    TokenKind::Ident(name) => format!("identifier `{name}`"),
                    TokenKind::Int(_) => "integer literal".to_string(),
                    TokenKind::Float(_) => "float literal".to_string(),
                    TokenKind::Str(_) => "string literal".to_string(),
                    TokenKind::Kw(kw) => format!("`{}`", kw.as_str()),
                    TokenKind::Newline => "newline".to_string(),
                    TokenKind::Eof => "end of file".to_string(),
                    $(TokenKind::$variant => concat!("`", $text, "`").to_string(),)*
                }
            }

            fn continues_line(&self) -> bool {
                match self {
                    $(TokenKind::$variant => token_kinds!(@continues $($cont)?),)*
                    _ => false,
                }
            }
        }
    };
}

token_kinds! {
    LParen => "(", continues;
    RParen => ")";
    LBracket => "[", continues;
    RBracket => "]";
    LBrace => "{", continues;
    RBrace => "}";
    Comma => ",", continues;
    Dot => ".", continues;
    Colon => ":";
    Question => "?";
    Arrow => "->", continues;
    FatArrow => "=>", continues;

    Plus => "+", continues;
    Minus => "-", continues;
    Star => "*", continues;
    Slash => "/", continues;
    Percent => "%", continues;
    Amp => "&", continues;
    Pipe => "|", continues;
    Caret => "^", continues;
    Tilde => "~";
    Bang => "!";
    Shl => "<<", continues;
    Shr => ">>", continues;
    AndAnd => "&&", continues;
    OrOr => "||", continues;
    EqEq => "==", continues;
    Ne => "!=", continues;
    Lt => "<", continues;
    Le => "<=", continues;
    Gt => ">", continues;
    Ge => ">=", continues;

    Eq => "=", continues;
    PlusEq => "+=", continues;
    MinusEq => "-=", continues;
    StarEq => "*=", continues;
    SlashEq => "/=", continues;
    PercentEq => "%=", continues;
    AmpEq => "&=", continues;
    PipeEq => "|=", continues;
    CaretEq => "^=", continues;
    ShlEq => "<<=", continues;
    ShrEq => ">>=", continues;
}

impl TokenKind {
    pub(crate) fn is_opener(&self) -> bool {
        matches!(
            self,
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace
        )
    }

    pub(crate) fn opener(&self) -> Option<TokenKind> {
        match self {
            TokenKind::RParen => Some(TokenKind::LParen),
            TokenKind::RBracket => Some(TokenKind::LBracket),
            TokenKind::RBrace => Some(TokenKind::LBrace),
            _ => None,
        }
    }
}

pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        src: source,
        pos: if source.starts_with('\u{feff}') { 3 } else { 0 },
        diags: Vec::new(),
        interp_depth: 0,
        line_abandoned: false,
        unterminated_quote: None,
        char_dropped: false,
    };
    let tokens = lexer
        .token_stream(None)
        .expect("only an interpolation can be cut off");
    (tokens, lexer.diags)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    diags: Vec<Diagnostic>,
    interp_depth: u32,
    line_abandoned: bool,
    unterminated_quote: Option<usize>,
    char_dropped: bool,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn error(&mut self, message: impl Into<String>, start: usize) {
        self.diags
            .push(Diagnostic::error(message, Span::new(start, self.pos)));
    }

    fn token_stream(&mut self, interp_start: Option<usize>) -> Option<Vec<Token>> {
        let mut out: Vec<Token> = Vec::new();
        let mut open: Vec<TokenKind> = Vec::new();
        let diags_before = self.diags.len();
        let base = usize::from(interp_start.is_some());
        if interp_start.is_some() {
            open.push(TokenKind::LParen);
        }
        loop {
            self.skip_spaces_and_comments();
            let start = self.pos;
            let next = self.peek();
            if let Some(interp_start) = interp_start {
                if matches!(next, None | Some('\n')) {
                    self.unterminated_interp(interp_start, diags_before);
                    return None;
                }
                if next == Some(')')
                    && open.iter().rposition(|k| *k == TokenKind::LParen) == Some(0)
                {
                    self.bump();
                    out.push(Token {
                        kind: TokenKind::RParen,
                        span: Span::new(start, self.pos),
                        depth: 0,
                    });
                    out.push(Token {
                        kind: TokenKind::Eof,
                        span: Span::new(self.pos, self.pos),
                        depth: 0,
                    });
                    return Some(out);
                }
            }
            let Some(c) = next else {
                out.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(start, start),
                    depth: 0,
                });
                return Some(out);
            };
            if c == '\n' {
                self.bump();
                self.line_abandoned = false;
                let dropped = std::mem::take(&mut self.char_dropped);
                let significant =
                    !matches!(open.last(), Some(TokenKind::LParen | TokenKind::LBracket));
                if significant {
                    push_newline(&mut out, start, (open.len() - base) as u32, dropped);
                }
                continue;
            }
            let Some(kind) = self.token(c) else {
                continue;
            };
            let mut depth = open.len() - base;
            if kind.is_opener() {
                open.push(kind.clone());
            } else if let Some(opener) = kind.opener() {
                // Not reported here: the parser reports mismatched closers.
                if let Some(i) = open.iter().rposition(|k| *k == opener) {
                    if !(interp_start.is_some() && i == 0) {
                        open.truncate(i);
                    }
                }
                depth = open.len() - base;
            } else if kind == TokenKind::Dot
                && out.last().is_some_and(|t| t.kind == TokenKind::Newline)
            {
                out.pop();
            }
            out.push(Token {
                kind,
                span: Span::new(start, self.pos),
                depth: depth as u32,
            });
        }
    }

    fn unterminated_interp(&mut self, interp_start: usize, diags_before: usize) {
        let unterminated_quote = self.unterminated_quote.take();
        if self.line_abandoned {
            return;
        }
        // Resumes after the `"` rather than at the end of the line: most likely
        // it was meant to close the string and only the `)` is missing, as in
        // `print("\(x")`. Only a `"` that opened an unterminated string
        // qualifies; resuming after one that opened a complete string would
        // start lexing in the middle of that string. Text after the quote
        // means it opened a string rather than closed one, so the rest of the
        // line is left to the usual recovery.
        if let Some(quote) = unterminated_quote.filter(|&q| q > interp_start) {
            let rest = &self.src[quote + 1..];
            if rest.starts_with([')', '\n', '\r']) || rest.is_empty() {
                self.pos = quote + 1;
            }
        }
        // Errors found inside are dropped rather than kept: without the `)`
        // we cannot tell where the interpolation was meant to end, so they are
        // mostly noise such as the closing `"` read as the start of a string.
        self.diags.truncate(diags_before);
        self.diags.push(Diagnostic::error(
            "unterminated string interpolation",
            Span::new(interp_start, interp_start + 2),
        ));
    }

    fn skip_spaces_and_comments(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.bump();
            } else if c == '/' && self.peek_at(1) == Some('/') {
                self.skip_rest_of_line();
            } else {
                break;
            }
        }
    }

    fn skip_rest_of_line(&mut self) {
        while self.peek().is_some_and(|c| c != '\n') {
            self.bump();
        }
    }

    fn token(&mut self, c: char) -> Option<TokenKind> {
        let start = self.pos;
        if c.is_ascii_alphabetic() || c == '_' {
            return Some(self.ident());
        }
        if c.is_ascii_digit() {
            return Some(self.number());
        }
        if c == '"' {
            return Some(self.string());
        }
        self.bump();
        if let Some(kind) = self.punct(c) {
            return Some(kind);
        }
        self.unexpected(c, start);
        self.char_dropped = true;
        None
    }

    fn punct(&mut self, c: char) -> Option<TokenKind> {
        use TokenKind::*;
        let kind = match c {
            '(' => LParen,
            ')' => RParen,
            '[' => LBracket,
            ']' => RBracket,
            '{' => LBrace,
            '}' => RBrace,
            ',' => Comma,
            '.' => Dot,
            ':' => Colon,
            '?' => Question,
            '~' => Tilde,
            '+' => self.with_eq(Plus, PlusEq),
            '*' => self.with_eq(Star, StarEq),
            '/' => self.with_eq(Slash, SlashEq),
            '%' => self.with_eq(Percent, PercentEq),
            '^' => self.with_eq(Caret, CaretEq),
            '!' => self.with_eq(Bang, Ne),
            '-' => {
                if self.eat('>') {
                    Arrow
                } else {
                    self.with_eq(Minus, MinusEq)
                }
            }
            '=' => {
                if self.eat('>') {
                    FatArrow
                } else {
                    self.with_eq(Eq, EqEq)
                }
            }
            '&' => {
                if self.eat('&') {
                    AndAnd
                } else {
                    self.with_eq(Amp, AmpEq)
                }
            }
            '|' => {
                if self.eat('|') {
                    OrOr
                } else {
                    self.with_eq(Pipe, PipeEq)
                }
            }
            '<' => {
                if self.eat('<') {
                    self.with_eq(Shl, ShlEq)
                } else {
                    self.with_eq(Lt, Le)
                }
            }
            '>' => {
                if self.eat('>') {
                    self.with_eq(Shr, ShrEq)
                } else {
                    self.with_eq(Gt, Ge)
                }
            }
            _ => return None,
        };
        Some(kind)
    }

    fn unexpected(&mut self, first: char, start: usize) {
        const FULL_WIDTH_SPACE: char = '\u{3000}';
        while let Some(c) = self.peek() {
            let starts_token = c.is_ascii_alphanumeric()
                || matches!(c, '_' | '"' | ' ' | '\t' | '\r' | '\n')
                || self.starts_punct(c);
            if starts_token || (c == FULL_WIDTH_SPACE) != (first == FULL_WIDTH_SPACE) {
                break;
            }
            self.bump();
        }
        let text = &self.src[start..self.pos];
        let message = if first == FULL_WIDTH_SPACE {
            "full-width space; use an ASCII space instead".to_string()
        } else if text.chars().nth(1).is_some() {
            format!("unexpected characters `{text}`")
        } else {
            format!("unexpected character `{text}`")
        };
        self.error(message, start);
    }

    fn starts_punct(&mut self, c: char) -> bool {
        let save = self.pos;
        self.bump();
        let found = self.punct(c).is_some();
        self.pos = save;
        found
    }

    fn with_eq(&mut self, plain: TokenKind, with_eq: TokenKind) -> TokenKind {
        if self.eat('=') { with_eq } else { plain }
    }

    fn ident(&mut self) -> TokenKind {
        let start = self.pos;
        self.eat_word_tail();
        let text = &self.src[start..self.pos];
        match Keyword::lookup(text) {
            Some(kw) => TokenKind::Kw(kw),
            None => TokenKind::Ident(text.to_string()),
        }
    }

    fn number(&mut self) -> TokenKind {
        let start = self.pos;
        let radix = match (self.peek(), self.peek_at(1)) {
            (Some('0'), Some('x')) => 16,
            (Some('0'), Some('b')) => 2,
            _ => 10,
        };
        if radix != 10 {
            self.pos += 2;
            let digits_start = self.pos;
            self.digits(radix);
            let digits = self.digits_text(digits_start);
            if !self.src[digits_start..].starts_with(|c: char| c.is_digit(radix)) {
                self.eat_word_tail();
                self.error("expected digits after the radix prefix", start);
                return TokenKind::Int(0);
            }
            return self.finish_int(&digits, radix, start);
        }

        self.digits(10);
        let is_float =
            self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit());
        if !is_float {
            let digits = self.digits_text(start);
            return self.finish_int(&digits, 10, start);
        }
        self.bump();
        self.digits(10);
        if matches!(self.peek(), Some('e' | 'E')) {
            let save = self.pos;
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.digits(10);
            } else {
                self.pos = save;
            }
        }
        let text = self.digits_text(start);
        self.check_suffix(start);
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => TokenKind::Float(value),
            _ => {
                self.error("float literal is too large", start);
                TokenKind::Float(0.0)
            }
        }
    }

    fn digits(&mut self, radix: u32) {
        while self.peek().is_some_and(|c| c.is_digit(radix) || c == '_') {
            self.bump();
        }
    }

    fn finish_int(&mut self, digits: &str, radix: u32, start: usize) -> TokenKind {
        self.check_suffix(start);
        match u64::from_str_radix(digits, radix) {
            Ok(n) => TokenKind::Int(n),
            Err(_) => {
                self.error("integer literal is too large", start);
                TokenKind::Int(0)
            }
        }
    }

    fn digits_text(&self, start: usize) -> String {
        self.src[start..self.pos]
            .chars()
            .filter(|&c| c != '_')
            .collect()
    }

    fn check_suffix(&mut self, start: usize) {
        if self.eat_word_tail() {
            self.error("invalid number literal", start);
        }
    }

    fn eat_word_tail(&mut self) -> bool {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.bump();
        }
        self.pos != start
    }

    fn string(&mut self) -> TokenKind {
        let start = self.pos;
        self.bump();
        let mut parts = Vec::new();
        let mut text = String::new();
        loop {
            let Some(c) = self.peek().filter(|&c| c != '\n') else {
                self.error("unterminated string literal", start);
                self.unterminated_quote = Some(start);
                break;
            };
            self.bump();
            match c {
                '"' => break,
                '\\' => {
                    let esc_start = self.pos - 1;
                    // Left for the next iteration: an escape must not swallow
                    // the newline that ends an unterminated string.
                    if matches!(self.peek(), None | Some('\n' | '\r')) {
                        continue;
                    }
                    match self.bump() {
                        Some('n') => text.push('\n'),
                        Some('t') => text.push('\t'),
                        Some('r') => text.push('\r'),
                        Some('0') => text.push('\0'),
                        Some('\\') => text.push('\\'),
                        Some('"') => text.push('"'),
                        Some('u') => self.unicode_escape(esc_start, &mut text),
                        Some('(') => {
                            if !text.is_empty() {
                                parts.push(StrPart::Text(std::mem::take(&mut text)));
                            }
                            match self.interpolation(esc_start) {
                                Some(tokens) => parts.push(StrPart::Interp(tokens)),
                                // No error here: the cause was already reported.
                                None => break,
                            }
                        }
                        _ => self.error("unknown escape sequence", esc_start),
                    }
                }
                c => text.push(c),
            }
        }
        if !text.is_empty() || parts.is_empty() {
            parts.push(StrPart::Text(text));
        }
        TokenKind::Str(parts)
    }

    fn interpolation(&mut self, start: usize) -> Option<Vec<Token>> {
        if self.interp_depth >= MAX_INTERP_DEPTH {
            self.error("string interpolation is nested too deeply", start);
            self.line_abandoned = true;
            self.skip_rest_of_line();
            return None;
        }
        self.interp_depth += 1;
        let tokens = self.token_stream(Some(start));
        self.interp_depth -= 1;
        tokens
    }

    fn unicode_escape(&mut self, esc_start: usize, text: &mut String) {
        if !self.eat('{') {
            self.error("expected `{` after `\\u`", esc_start);
            return;
        }
        let digits_start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
            self.bump();
        }
        let digits = &self.src[digits_start..self.pos];
        if !self.eat('}') || digits.is_empty() {
            self.error("malformed `\\u{...}` escape", esc_start);
            return;
        }
        match u32::from_str_radix(digits, 16)
            .ok()
            .and_then(char::from_u32)
        {
            Some(ch) => text.push(ch),
            None => self.error("invalid unicode scalar value", esc_start),
        }
    }
}

fn push_newline(out: &mut Vec<Token>, pos: usize, depth: u32, char_dropped: bool) {
    match out.last() {
        Some(t) if t.kind != TokenKind::Newline && (char_dropped || !t.kind.continues_line()) => {
            out.push(Token {
                kind: TokenKind::Newline,
                span: Span::new(pos, pos + 1),
                depth,
            })
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let (tokens, diags) = lex(src);
        assert_eq!(diags, vec![], "unexpected diagnostics for {src:?}");
        tokens.into_iter().map(|t| t.kind).collect()
    }

    fn ident(s: &str) -> TokenKind {
        Ident(s.to_string())
    }

    fn text(s: &str) -> StrPart {
        StrPart::Text(s.to_string())
    }

    #[test]
    fn idents_and_keywords() {
        assert_eq!(
            kinds("fn foo _bar self none"),
            vec![
                Kw(Keyword::Fn),
                ident("foo"),
                ident("_bar"),
                Kw(Keyword::SelfValue),
                ident("none"),
                Eof
            ]
        );
    }

    #[test]
    fn operators_take_the_longest_match() {
        assert_eq!(
            kinds("a <<= b >> c -> d => e != f || g && h"),
            vec![
                ident("a"),
                ShlEq,
                ident("b"),
                Shr,
                ident("c"),
                Arrow,
                ident("d"),
                FatArrow,
                ident("e"),
                Ne,
                ident("f"),
                OrOr,
                ident("g"),
                AndAnd,
                ident("h"),
                Eof
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds("0 1_000 0xff 0b1010 3.25 1.5e3 2.0E-1"),
            vec![
                Int(0),
                Int(1000),
                Int(255),
                Int(10),
                Float(3.25),
                Float(1500.0),
                Float(0.2),
                Eof
            ]
        );
    }

    #[test]
    fn method_call_on_int_is_not_a_float() {
        assert_eq!(
            kinds("1.to_string"),
            vec![Int(1), Dot, ident("to_string"), Eof]
        );
    }

    #[test]
    fn bad_numbers_are_reported() {
        assert_eq!(lex("12abc").1.len(), 1);
        assert_eq!(lex("0x").1.len(), 1);
        assert_eq!(lex("99999999999999999999").1.len(), 1);
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            kinds(r#""a\tb\n\"\\\u{3042}" """#),
            vec![Str(vec![text("a\tb\n\"\\あ")]), Str(vec![text("")]), Eof]
        );
    }

    #[test]
    fn string_interpolation_holds_tokens() {
        let toks = kinds(r#""x = \(f(a, "s")) end""#);
        let Str(parts) = &toks[0] else {
            panic!("expected a string, got {toks:?}")
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], text("x = "));
        let StrPart::Interp(inner) = &parts[1] else {
            panic!()
        };
        let inner: Vec<_> = inner.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            inner,
            vec![
                ident("f"),
                LParen,
                ident("a"),
                Comma,
                Str(vec![text("s")]),
                RParen,
                RParen,
                Eof
            ]
        );
        assert_eq!(parts[2], text(" end"));
    }

    #[test]
    fn unterminated_string() {
        let (_, diags) = lex("\"abc\nx");
        assert_eq!(diags[0].message, "unterminated string literal");
        let (_, diags) = lex("\"\\(a");
        assert_eq!(diags[0].message, "unterminated string interpolation");
    }

    #[test]
    fn unterminated_interpolation_is_one_error_at_its_opening() {
        for src in [
            "\"\\(a",
            "\"\\(a\"",
            "\"\\(a\nb",
            "\"\\(a \"s\nb",
            "\"\\(a // c)\"\nb",
        ] {
            let (_, diags) = lex(src);
            assert_eq!(diags.len(), 1, "{src:?}: {diags:#?}");
            assert_eq!(diags[0].message, "unterminated string interpolation");
            assert_eq!(diags[0].span, Span::new(1, 3), "{src:?}");
        }
    }

    #[test]
    fn line_after_unterminated_interpolation_is_lexed_normally() {
        let (tokens, _) = lex("\"\\(a // c)\"\nb");
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![Str(vec![text("")]), Newline, ident("b"), Eof]);
    }

    #[test]
    fn deeply_nested_interpolation_is_one_error() {
        let n = 1000;
        let src = format!("\"{}x{}\"\nb", "\\(\"".repeat(n), "\")".repeat(n));
        let (tokens, diags) = lex(&src);
        let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, vec!["string interpolation is nested too deeply"]);
        assert_eq!(tokens[tokens.len() - 2].kind, ident("b"));
    }

    #[test]
    fn radix_prefix_without_digits_consumes_the_rest_of_the_word() {
        let (tokens, diags) = lex("0xg");
        assert_eq!(diags.len(), 1);
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![Int(0), Eof]);
    }

    #[test]
    fn run_of_unexpected_characters_is_one_error() {
        let (tokens, diags) = lex("名前 = 1");
        let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, vec!["unexpected characters `名前`"]);
        assert_eq!(diags[0].span, Span::new(0, 6));
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![Eq, Int(1), Eof]);
    }

    #[test]
    fn unexpected_run_stops_at_a_valid_token() {
        let (tokens, diags) = lex("@#+x");
        let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, vec!["unexpected characters `@#`"]);
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![Plus, ident("x"), Eof]);
    }

    #[test]
    fn single_unexpected_character() {
        let (_, diags) = lex("a @ b");
        assert_eq!(diags[0].message, "unexpected character `@`");
    }

    #[test]
    fn full_width_spaces_get_their_own_message() {
        let (tokens, diags) = lex("a\u{3000}\u{3000}名");
        let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "full-width space; use an ASCII space instead",
                "unexpected character `名`"
            ]
        );
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![ident("a"), Eof]);
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(
            kinds("a // hi\nb"),
            vec![ident("a"), Newline, ident("b"), Eof]
        );
    }

    #[test]
    fn newlines_collapse_and_are_ignored_inside_parens() {
        assert_eq!(
            kinds("\n\na\n\n\nf(\n1,\n2\n)\n"),
            vec![
                ident("a"),
                Newline,
                ident("f"),
                LParen,
                Int(1),
                Comma,
                Int(2),
                RParen,
                Newline,
                Eof
            ]
        );
    }

    #[test]
    fn newlines_inside_braces_within_parens_are_significant() {
        assert_eq!(
            kinds("f(|x| {\na\nb\n})"),
            vec![
                ident("f"),
                LParen,
                Pipe,
                ident("x"),
                Pipe,
                LBrace,
                ident("a"),
                Newline,
                ident("b"),
                Newline,
                RBrace,
                RParen,
                Eof
            ]
        );
    }

    #[test]
    fn line_ending_in_operator_continues() {
        assert_eq!(
            kinds("a +\nb =\nc"),
            vec![ident("a"), Plus, ident("b"), Eq, ident("c"), Eof]
        );
    }

    #[test]
    fn line_starting_with_dot_continues() {
        assert_eq!(
            kinds("a\n  .b()\n  .c"),
            vec![
                ident("a"),
                Dot,
                ident("b"),
                LParen,
                RParen,
                Dot,
                ident("c"),
                Eof
            ]
        );
    }

    #[test]
    fn spans_are_byte_offsets() {
        let (tokens, _) = lex("ab  \"あ\"");
        assert_eq!(tokens[0].span, Span::new(0, 2));
        assert_eq!(tokens[1].span, Span::new(4, 9));
    }

    #[test]
    fn backslash_at_end_of_line_leaves_the_string_unterminated() {
        let (tokens, diags) = lex("x = \"abc\\\ny = 2\n");
        let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, ["unterminated string literal"]);
        assert!(tokens.iter().any(|t| t.kind == Ident("y".into())));
    }

    #[test]
    fn backslash_at_end_of_a_crlf_line_leaves_the_string_unterminated() {
        let (_, diags) = lex("x = \"abc\\\r\ny = 2\r\n");
        let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, ["unterminated string literal"]);
    }

    #[test]
    fn invalid_float_suffix_is_reported_once() {
        for src in ["x = 1.5abc\n", "y = 2.0e\n"] {
            let (_, diags) = lex(src);
            let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
            assert_eq!(messages, ["invalid number literal"], "{src:?}");
        }
    }

    #[test]
    fn text_after_an_unterminated_interpolation_quote_is_not_lexed_as_code() {
        let (tokens, diags) = lex("x = \"\\(f(\"a\n");
        assert_eq!(diags.len(), 1);
        assert!(!tokens.iter().any(|t| t.kind == Ident("a".into())));
    }

    #[test]
    fn leading_byte_order_mark_is_skipped() {
        let (tokens, diags) = lex("\u{feff}fn");
        assert_eq!(diags, vec![]);
        assert_eq!(tokens[0].kind, Kw(Keyword::Fn));
    }
}
