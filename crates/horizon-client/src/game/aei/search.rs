//! Field-aware AEI queries. AND binds tighter than OR; spaces imply AND.
//! Prefixes scope one term; quoted phrases keep spaces and operators literal.

#[derive(Default)]
pub(super) struct Document {
    pub id: String,
    pub name: String,
    pub mods: Vec<String>,
    pub tooltip: Vec<String>,
    pub tags: Vec<String>,
    pub tabs: Vec<String>,
}
impl Document {
    pub fn normalize(&mut self) {
        self.name = horizon_client::clipboard::strip_formatting(&self.name).to_lowercase();
        for values in [
            &mut self.mods,
            &mut self.tooltip,
            &mut self.tags,
            &mut self.tabs,
        ] {
            for value in values {
                *value = horizon_client::clipboard::strip_formatting(value).to_lowercase();
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Term(String),
    And,
    Or,
    Open,
    Close,
}
#[derive(Debug)]
pub(super) enum Query {
    All,
    Term(char, String),
    And(Vec<Query>),
    Or(Vec<Query>),
}
impl Query {
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let tokens = tokenize(text)?;
        if tokens.is_empty() {
            return Ok(Self::All);
        }
        let mut parser = Parser {
            tokens: &tokens,
            position: 0,
            depth: 0,
        };
        let query = parser.or()?;
        if parser.position != tokens.len() {
            return Err("Unexpected closing parenthesis");
        }
        Ok(query)
    }
    pub fn matches(&self, document: &Document) -> bool {
        match self {
            Self::All => true,
            Self::And(terms) => terms.iter().all(|q| q.matches(document)),
            Self::Or(terms) => terms.iter().any(|q| q.matches(document)),
            Self::Term(field, text) => {
                let values = match field {
                    '@' => &document.mods,
                    '#' => &document.tooltip,
                    '$' => &document.tags,
                    '%' => &document.tabs,
                    _ => return document.id.contains(text) || document.name.contains(text),
                };
                values.iter().any(|value| value.contains(text))
            }
        }
    }
}
fn tokenize(text: &str) -> Result<Vec<Token>, &'static str> {
    let mut chars = text.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        tokens.push(match c {
            '(' => Token::Open,
            ')' => Token::Close,
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                Token::Or
            }
            '&' => {
                if chars.next() != Some('&') {
                    return Err("Use && for AND, or quote a literal &");
                }
                Token::And
            }
            _ => {
                let mut term = String::new();
                let mut quoted = false;
                let mut current = Some(c);
                loop {
                    let c = current.take().or_else(|| chars.next());
                    let Some(c) = c else {
                        break;
                    };
                    match c {
                        '"' => quoted = !quoted,
                        '\\' if quoted => {
                            term.push(chars.next().ok_or("Incomplete escape in quoted phrase")?)
                        }
                        _ => term.push(c),
                    }
                    if !quoted
                        && chars
                            .peek()
                            .is_none_or(|c| c.is_whitespace() || "&|()".contains(*c))
                    {
                        break;
                    }
                }
                if quoted {
                    return Err("Close the quoted phrase with a double quote");
                }
                Token::Term(term.to_lowercase())
            }
        });
    }
    Ok(tokens)
}
struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    depth: usize,
}
impl Parser<'_> {
    fn or(&mut self) -> Result<Query, &'static str> {
        let mut terms = vec![self.and()?];
        while self.tokens.get(self.position) == Some(&Token::Or) {
            self.position += 1;
            terms.push(self.and()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            Query::Or(terms)
        })
    }
    fn and(&mut self) -> Result<Query, &'static str> {
        let mut terms = vec![self.atom()?];
        loop {
            match self.tokens.get(self.position) {
                Some(Token::And) => {
                    self.position += 1;
                }
                Some(Token::Term(_) | Token::Open) => {}
                _ => break,
            }
            terms.push(self.atom()?);
        }
        Ok(if terms.len() == 1 {
            terms.pop().unwrap()
        } else {
            Query::And(terms)
        })
    }
    fn atom(&mut self) -> Result<Query, &'static str> {
        let token = self
            .tokens
            .get(self.position)
            .ok_or("Enter a search term after the operator")?;
        self.position += 1;
        match token {
            Token::Term(text) => {
                let first = text.chars().next().unwrap_or(' ');
                let scoped = "@#$%".contains(first);
                Ok(Query::Term(
                    if scoped { first } else { ' ' },
                    if scoped {
                        text[1..].into()
                    } else {
                        text.clone()
                    },
                ))
            }
            Token::Open => {
                self.depth += 1;
                if self.depth > 32 {
                    return Err("Too many nested search groups");
                }
                let query = self.or()?;
                if self.tokens.get(self.position) != Some(&Token::Close) {
                    return Err("Close the search group with )");
                }
                self.position += 1;
                self.depth -= 1;
                Ok(query)
            }
            _ => Err("Expected a search term or a parenthesized group"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn iron() -> Document {
        Document {
            id: "example:iron_ingot".into(),
            name: "iron ingot".into(),
            mods: vec!["example".into(), "example machines".into()],
            tooltip: vec!["a sturdy crafting material".into()],
            tags: vec!["c:ingots/iron".into()],
            tabs: vec!["ingredients".into()],
        }
    }
    fn matches(query: &str) -> bool {
        Query::parse(query).unwrap().matches(&iron())
    }
    #[test]
    fn filters_use_their_own_metadata_and_combine() {
        assert!(matches("@example #sturdy $ingots %ingredients iron"));
        assert!(matches("@\"Example Machines\" && $c:ingots/iron"));
        for wrong in [
            "@sturdy",
            "#example machines",
            "$ingredients",
            "%iron",
            "missing",
        ] {
            assert!(!matches(wrong), "{wrong}");
        }
        assert!(matches("example:iron"));
    }
    #[test]
    fn and_binds_before_or_and_parentheses_override_it() {
        assert!(matches("missing && @absent | iron && @example"));
        assert!(!matches("(missing | iron) && @absent"));
        assert!(matches("(@absent | @example) && (iron | gold)"));
        assert!(matches("gold|IRON"));
        assert!(matches("gold||IRON"));
        assert!(!matches("iron gold"));
    }
    #[test]
    fn quoted_operators_and_unicode_are_literal() {
        let document = Document {
            id: "a:test".into(),
            name: "lâmina | a && b".into(),
            ..Default::default()
        };
        assert!(
            Query::parse("\"LÂMINA | a && b\"")
                .unwrap()
                .matches(&document)
        );
        assert!(matches("\"iron ingot\""));
        assert!(!matches("\"iron  ingot\""));
    }
    #[test]
    fn empty_queries_and_unfinished_input_are_safe() {
        assert!(matches(" \t "));
        assert!(matches("@"));
        for bad in [
            "iron &&",
            "| iron",
            "iron |",
            "()",
            "(iron",
            "iron)",
            "\"iron",
            "iron & gold",
        ] {
            assert!(Query::parse(bad).is_err(), "{bad}");
        }
        assert!(Query::parse(&format!("{}iron{}", "(".repeat(40), ")".repeat(40))).is_err());
    }
}
