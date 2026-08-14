//! Parsing and evaluation for metadata filter expressions.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use crate::document::Metadata;

/// A compiled metadata filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    expression: Expression,
}

impl Filter {
    /// Evaluate this filter against one document's metadata.
    pub fn matches(&self, metadata: &Metadata) -> bool {
        self.expression.matches(metadata)
    }
}

impl FromStr for Filter {
    type Err = ParseError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let tokens = tokenize(source)?;
        let mut parser = Parser::new(source, tokens);
        let expression = parser.parse_expression()?;
        if let Some(token) = parser.peek() {
            return Err(parser.error_at(token.offset, "unexpected token"));
        }
        Ok(Self { expression })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expression {
    Priority(Comparison, i64),
    Tags(SetOperation, BTreeSet<String>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
}

impl Expression {
    fn matches(&self, metadata: &Metadata) -> bool {
        match self {
            Self::Priority(comparison, expected) => metadata
                .priority
                .is_some_and(|priority| comparison.matches(i64::from(priority), *expected)),
            Self::Tags(operation, expected) => {
                let actual = metadata
                    .tags
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                match operation {
                    SetOperation::All => expected.iter().all(|tag| actual.contains(tag.as_str())),
                    SetOperation::Any => expected.iter().any(|tag| actual.contains(tag.as_str())),
                    SetOperation::None => expected.iter().all(|tag| !actual.contains(tag.as_str())),
                    SetOperation::Exact => {
                        actual.len() == expected.len()
                            && expected.iter().all(|tag| actual.contains(tag.as_str()))
                    }
                }
            }
            Self::And(left, right) => left.matches(metadata) && right.matches(metadata),
            Self::Or(left, right) => left.matches(metadata) || right.matches(metadata),
            Self::Not(expression) => !expression.matches(metadata),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Comparison {
    fn matches(self, actual: i64, expected: i64) -> bool {
        match self {
            Self::Equal => actual == expected,
            Self::NotEqual => actual != expected,
            Self::Less => actual < expected,
            Self::LessOrEqual => actual <= expected,
            Self::Greater => actual > expected,
            Self::GreaterOrEqual => actual >= expected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetOperation {
    All,
    Any,
    None,
    Exact,
}

/// A syntax error in a filter expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    offset: usize,
    message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "filter syntax error at byte {}: {}",
            self.offset + 1,
            self.message
        )
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Word(String),
    Dot,
    Comma,
    LeftParenthesis,
    RightParenthesis,
    And,
    Or,
    Not,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        let character = source[offset..].chars().next().expect("valid offset");
        if character.is_whitespace() {
            offset += character.len_utf8();
            continue;
        }

        let start = offset;
        let (kind, consumed) = match character {
            '.' => (TokenKind::Dot, 1),
            ',' => (TokenKind::Comma, 1),
            '(' => (TokenKind::LeftParenthesis, 1),
            ')' => (TokenKind::RightParenthesis, 1),
            '&' => (TokenKind::And, 1),
            '|' => (TokenKind::Or, 1),
            '=' => (TokenKind::Equal, 1),
            '!' if source[start + 1..].starts_with('=') => (TokenKind::NotEqual, 2),
            '!' => (TokenKind::Not, 1),
            '<' if source[start + 1..].starts_with('=') => (TokenKind::LessOrEqual, 2),
            '<' => (TokenKind::Less, 1),
            '>' if source[start + 1..].starts_with('=') => (TokenKind::GreaterOrEqual, 2),
            '>' => (TokenKind::Greater, 1),
            '\'' | '"' => {
                let (value, end) = quoted_word(source, start, character)?;
                tokens.push(Token {
                    kind: TokenKind::Word(value),
                    offset: start,
                });
                offset = end;
                continue;
            }
            _ => {
                let end = source[start..]
                    .char_indices()
                    .find_map(|(index, candidate)| {
                        (index > 0 && is_delimiter(candidate)).then_some(start + index)
                    })
                    .unwrap_or(source.len());
                let value = &source[start..end];
                if value.is_empty() {
                    return Err(ParseError {
                        offset: start,
                        message: format!("unexpected character {character:?}"),
                    });
                }
                tokens.push(Token {
                    kind: TokenKind::Word(value.to_owned()),
                    offset: start,
                });
                offset = end;
                continue;
            }
        };
        tokens.push(Token {
            kind,
            offset: start,
        });
        offset += consumed;
    }
    Ok(tokens)
}

fn quoted_word(source: &str, start: usize, quote: char) -> Result<(String, usize), ParseError> {
    let mut value = String::new();
    let mut escaped = false;
    for (relative, character) in source[start + quote.len_utf8()..].char_indices() {
        let offset = start + quote.len_utf8() + relative;
        if escaped {
            value.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return Ok((value, offset + character.len_utf8()));
        } else {
            value.push(character);
        }
    }
    Err(ParseError {
        offset: start,
        message: "unterminated quoted value".to_owned(),
    })
}

fn is_delimiter(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '.' | ',' | '(' | ')' | '&' | '|' | '!' | '=' | '<' | '>' | '\'' | '"'
        )
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            position: 0,
        }
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        if self.tokens.is_empty() {
            return Err(self.error_at(0, "expected an expression"));
        }
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_and()?;
        while self.take_kind(&TokenKind::Or) || self.take_word("or") {
            expression = Expression::Or(Box::new(expression), Box::new(self.parse_and()?));
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_unary()?;
        while self.take_kind(&TokenKind::And) || self.take_word("and") {
            expression = Expression::And(Box::new(expression), Box::new(self.parse_unary()?));
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        if self.take_kind(&TokenKind::Not) || self.take_word("not") {
            return Ok(Expression::Not(Box::new(self.parse_unary()?)));
        }
        if self.take_kind(&TokenKind::LeftParenthesis) {
            let expression = self.parse_or()?;
            self.expect_kind(TokenKind::RightParenthesis, "expected ')' after expression")?;
            return Ok(expression);
        }
        self.parse_predicate()
    }

    fn parse_predicate(&mut self) -> Result<Expression, ParseError> {
        let (field, field_offset) = self.expect_word("expected field name")?;
        if self.take_kind(&TokenKind::Dot) {
            if field != "tags" {
                return Err(self.error_at(field_offset, format!("unknown set field {field:?}")));
            }
            let (operation, operation_offset) = self.expect_word("expected set operation")?;
            let operation = match operation.as_str() {
                "all" => SetOperation::All,
                "any" => SetOperation::Any,
                "none" => SetOperation::None,
                "exact" => SetOperation::Exact,
                _ => {
                    return Err(self.error_at(
                        operation_offset,
                        format!("unknown tags operation {operation:?}"),
                    ));
                }
            };
            self.expect_kind(
                TokenKind::LeftParenthesis,
                "expected '(' after set operation",
            )?;
            let values = self.parse_set_values()?;
            return Ok(Expression::Tags(operation, values));
        }

        if field != "priority" {
            return Err(self.error_at(field_offset, format!("unknown scalar field {field:?}")));
        }
        let comparison = self.parse_comparison()?;
        let (value, offset) = self.expect_word("expected integer after comparison")?;
        let value = value
            .parse::<i64>()
            .map_err(|_| self.error_at(offset, format!("expected integer, found {value:?}")))?;
        Ok(Expression::Priority(comparison, value))
    }

    fn parse_set_values(&mut self) -> Result<BTreeSet<String>, ParseError> {
        let mut values = BTreeSet::new();
        if self.take_kind(&TokenKind::RightParenthesis) {
            return Ok(values);
        }
        loop {
            let (value, _) = self.expect_word("expected tag value")?;
            values.insert(value);
            if self.take_kind(&TokenKind::RightParenthesis) {
                return Ok(values);
            }
            self.expect_kind(TokenKind::Comma, "expected ',' or ')' after tag value")?;
        }
    }

    fn parse_comparison(&mut self) -> Result<Comparison, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.error_at(self.source.len(), "expected scalar comparison"));
        };
        let comparison = match token.kind {
            TokenKind::Equal => Comparison::Equal,
            TokenKind::NotEqual => Comparison::NotEqual,
            TokenKind::Less => Comparison::Less,
            TokenKind::LessOrEqual => Comparison::LessOrEqual,
            TokenKind::Greater => Comparison::Greater,
            TokenKind::GreaterOrEqual => Comparison::GreaterOrEqual,
            _ => return Err(self.error_at(token.offset, "expected scalar comparison")),
        };
        self.position += 1;
        Ok(comparison)
    }

    fn expect_word(&mut self, message: &str) -> Result<(String, usize), ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.error_at(self.source.len(), message));
        };
        let TokenKind::Word(value) = token.kind else {
            return Err(self.error_at(token.offset, message));
        };
        self.position += 1;
        Ok((value, token.offset))
    }

    fn expect_kind(&mut self, kind: TokenKind, message: &str) -> Result<(), ParseError> {
        if self.take_kind(&kind) {
            Ok(())
        } else {
            let offset = self.peek().map_or(self.source.len(), |token| token.offset);
            Err(self.error_at(offset, message))
        }
    }

    fn take_word(&mut self, expected: &str) -> bool {
        if matches!(self.peek(), Some(Token { kind: TokenKind::Word(value), .. }) if value == expected)
        {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn take_kind(&mut self, expected: &TokenKind) -> bool {
        if self.peek().is_some_and(|token| &token.kind == expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn error_at(&self, offset: usize, message: impl Into<String>) -> ParseError {
        ParseError {
            offset,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(priority: Option<u8>, tags: &[&str]) -> Metadata {
        Metadata {
            priority,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            ..Metadata::default()
        }
    }

    fn matches(source: &str, metadata: &Metadata) -> bool {
        source.parse::<Filter>().unwrap().matches(metadata)
    }

    #[test]
    fn compares_scalar_values() {
        let metadata = metadata(Some(50), &[]);
        assert!(matches("priority >= 50", &metadata));
        assert!(matches("priority = 50", &metadata));
        assert!(matches("priority != 49", &metadata));
        assert!(matches("priority < 51", &metadata));
        assert!(matches("priority <= 50", &metadata));
        assert!(!matches("priority > 50", &metadata));
        assert!(!matches("priority = 0", &Metadata::default()));
    }

    #[test]
    fn evaluates_all_set_operations_as_sets() {
        let metadata = metadata(Some(50), &["foo", "bar", "foo"]);
        assert!(matches("tags.all(foo, bar)", &metadata));
        assert!(matches("tags.any(nope, foo)", &metadata));
        assert!(matches("tags.none(nope, other)", &metadata));
        assert!(matches("tags.exact(bar, foo)", &metadata));
        assert!(!matches("tags.all(foo, nope)", &metadata));
        assert!(!matches("tags.any(nope, other)", &metadata));
        assert!(!matches("tags.none(foo, other)", &metadata));
        assert!(!matches("tags.exact(foo)", &metadata));
    }

    #[test]
    fn composes_with_words_symbols_parentheses_and_precedence() {
        let metadata = metadata(Some(50), &["foo"]);
        assert!(matches(
            "tags.any(foo) & tags.none(baz) and priority >= 50",
            &metadata
        ));
        assert!(matches("not tags.any(baz) or priority = 0", &metadata));
        assert!(matches("tags.any(baz) | priority = 50", &metadata));
        assert!(!matches(
            "(tags.any(foo) or tags.any(bar)) and !priority >= 50",
            &metadata
        ));
    }

    #[test]
    fn supports_quoted_and_unicode_tag_values() {
        let metadata = metadata(None, &["two words", "café", "it's"]);
        assert!(matches(
            r#"tags.all("two words", café, 'it\'s')"#,
            &metadata
        ));
    }

    #[test]
    fn reports_useful_syntax_errors() {
        for source in [
            "",
            "score = 1",
            "tags.some(foo)",
            "labels.any(foo)",
            "tags.any(foo",
            "tags.any(foo,)",
            "priority",
            "priority >= nope",
            "priority = 1 and",
            "priority = 1 trailing",
        ] {
            let error = source.parse::<Filter>().unwrap_err().to_string();
            assert!(error.contains("filter syntax error at byte"), "{error}");
        }
    }
}
