use crate::parser::*;
use crate::pattern::Pattern;
use crate::source_location::*;
use crate::token::{Token, TokenKind};
use crate::type_ann::TypeAnn;

const EOF: Token = Token {
    kind: TokenKind::Eof,
    loc: SourceLocation {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    },
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FuncParam {
    pub pattern: Pattern,
    pub type_ann: Option<TypeAnn>,
    pub optional: bool,
}

impl<'a> Parser<'a> {
    pub fn parse_params(&mut self) -> Vec<FuncParam> {
        assert_eq!(
            self.next().unwrap_or(EOF.clone()).kind,
            TokenKind::LeftParen
        );

        let mut params: Vec<FuncParam> = Vec::new();
        while self.peek().unwrap_or(&EOF).kind != TokenKind::RightParen {
            let pattern = self.parse_pattern();

            let optional = if let TokenKind::Question = self.peek().unwrap_or(&EOF).kind {
                self.next().unwrap_or(EOF.clone());
                true
            } else {
                false
            };

            if let TokenKind::Colon = self.peek().unwrap_or(&EOF).kind {
                self.next().unwrap_or(EOF.clone());
                params.push(FuncParam {
                    pattern,
                    type_ann: Some(self.parse_type_ann()),
                    optional,
                });
            } else {
                params.push(FuncParam {
                    pattern,
                    type_ann: None,
                    optional: false, // Should `?` be supported when there's not type param?
                });
            }

            // TODO: param defaults

            match self.peek().unwrap_or(&EOF).kind {
                TokenKind::RightParen => break,
                TokenKind::Comma => {
                    self.next().unwrap_or(EOF.clone());
                }
                _ => panic!(
                    "Expected comma or right paren, got {:?}",
                    self.peek().unwrap_or(&EOF)
                ),
            }
        }

        assert_eq!(
            self.next().unwrap_or(EOF.clone()).kind,
            TokenKind::RightParen
        );

        params
    }
}