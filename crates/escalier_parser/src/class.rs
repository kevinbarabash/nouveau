use crate::block::Block;
use crate::expr::Expr;
use crate::func_param::FuncParam;
use crate::identifier::Ident;
use crate::parse_error::ParseError;
use crate::parser::*;
use crate::span::Span;
use crate::token::*;
use crate::type_ann::TypeAnn;
use crate::type_param::TypeParam;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Class {
    pub span: Span,
    // pub name: Option<Ident>,
    pub type_params: Option<Vec<TypeParam>>,
    pub super_class: Option<Ident>,
    pub super_type_args: Option<Vec<TypeAnn>>,
    pub body: Vec<ClassMember>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Method {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<FuncParam>,
    pub body: Block,
    pub type_ann: Option<TypeAnn>, // return type
    pub is_async: bool,
    pub is_gen: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Constructor {
    pub span: Span,
    pub params: Vec<FuncParam>,
    pub body: Block,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Field {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<TypeAnn>,
    pub init: Option<Box<Expr>>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ClassMember {
    Method(Method),
    Constructor(Constructor),
    Field(Field), // TODO: rename to property?
}

impl<'a> Parser<'a> {
    pub fn parse_class(&mut self) -> Result<Expr, ParseError> {
        let token = self.next().unwrap_or(EOF.clone());
        assert_eq!(token.kind, TokenKind::Class);

        let type_params = self.maybe_parse_type_params()?;

        let super_class = if self.peek().unwrap_or(&EOF).kind == TokenKind::Extends {
            self.next(); // consumes 'extends'
            let token = self.next().unwrap_or(EOF.clone());
            if let TokenKind::Identifier(name) = token.kind {
                Some(Ident {
                    span: token.span,
                    name,
                })
            } else {
                panic!("expected identifier");
            }
        } else {
            None
        };

        assert_eq!(
            self.next().unwrap_or(EOF.clone()).kind,
            TokenKind::LeftBrace
        );

        let mut body = vec![];

        while self.peek().unwrap_or(&EOF).kind != TokenKind::RightBrace {
            let member = self.parse_class_member()?;
            body.push(member);
        }

        assert_eq!(
            self.next().unwrap_or(EOF.clone()).kind,
            TokenKind::RightBrace
        );

        let end = self.scanner.cursor();

        let expr = Expr::Class(Class {
            span: Span {
                start: token.span.start,
                end,
            },
            type_params,
            super_class,
            super_type_args: None, // TODO
            body,
        });

        Ok(expr)
    }

    fn parse_class_member(&mut self) -> Result<ClassMember, ParseError> {
        // TODO:
        // - get/set
        // - static
        // - async
        // - generator
        // - type annotations

        let token = self.peek().unwrap_or(&EOF);
        match token.kind {
            TokenKind::Identifier(_) => self.parse_field(),
            TokenKind::Fn => self.parse_method(),
            TokenKind::Gen => self.parse_method(),
            TokenKind::Async => self.parse_method(),
            _ => panic!("unexpected token {:?}", token),
        }
    }

    fn parse_field(&mut self) -> Result<ClassMember, ParseError> {
        let token = self.next().unwrap_or(EOF.clone());
        let start = token.span.start;

        let name = if let TokenKind::Identifier(name) = &token.kind {
            Ident {
                span: token.span,
                name: name.to_owned(),
            }
        } else {
            panic!("expected identifier");
        };

        let field = match self.peek().unwrap_or(&EOF).kind {
            TokenKind::Colon => {
                self.next(); // consumes ':'
                let type_ann = self.parse_type_ann()?;
                let end = self.scanner.cursor();

                let span = Span { start, end };

                ClassMember::Field(Field {
                    span,
                    name,
                    init: None,
                    type_ann: Some(type_ann),
                })
            }
            TokenKind::Assign => {
                self.next(); // consumes '='
                let init = self.parse_expr()?;
                let end = self.scanner.cursor();

                let span = Span { start, end };

                ClassMember::Field(Field {
                    span,
                    name,
                    init: Some(Box::new(init)),
                    type_ann: None,
                })
            }
            _ => panic!("expected ':' or '='"),
        };

        Ok(field)
    }

    fn parse_method(&mut self) -> Result<ClassMember, ParseError> {
        let start = self.peek().unwrap_or(&EOF).span.start;

        let is_async = if self.peek().unwrap_or(&EOF).kind == TokenKind::Async {
            self.next(); // consumes 'async'
            true
        } else {
            false
        };

        let is_gen = if self.peek().unwrap_or(&EOF).kind == TokenKind::Gen {
            self.next(); // consumes 'gen'
            true
        } else {
            false
        };

        assert_eq!(self.next().unwrap_or(EOF.clone()).kind, TokenKind::Fn);

        // TODO: extract into `parse_ident()` method
        let next = self.next().unwrap_or(EOF.clone());
        let name = if let TokenKind::Identifier(name) = next.kind {
            Ident {
                span: next.span,
                name,
            }
        } else {
            panic!("expected identifier");
        };

        let type_params = self.maybe_parse_type_params()?;
        let params = self.parse_params()?;

        let type_ann = if self.peek().unwrap_or(&EOF).kind == TokenKind::Colon {
            self.next(); // consumes ':'
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        let end = self.scanner.cursor();

        let span = Span { start, end };

        let method = ClassMember::Method(Method {
            span,
            name,
            params,
            body,
            is_async,
            is_gen,
            type_params,
            type_ann,
        });

        Ok(method)
    }
}

/*
let Foo = class {
    field: number

    fn constructor(self) {}

    fn foo(self) {}
    fn bar(mut self) {}

    fn baz() {} // static method
}

fn foo() {
    let foo = Foo {
        field: 42,
    }
}
 */
