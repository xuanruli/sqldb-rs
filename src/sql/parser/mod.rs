use crate::{error::Result};
use lexer::{Keyword, Lexer, Token};
use std::iter::Peekable;

pub mod ast;
mod lexer;

use crate::error::Error;
use ast::{Column, Statement};
use crate::sql::types::DataType;

pub struct Parser<'a> {
    lexer: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Parser {
            lexer: Lexer::new(input).peekable(),
        }
    }
    pub fn parse(&mut self) -> Result<Statement> {
        let statement = self.parse_statement()?;
        self.next_expect(Token::Semicolon)?;
        if let Some(token) = self.peek()? {
            return Err(Error::Parse(format!("[Parse] Unexpected token {}", token)));
        }
        Ok(statement)
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        match self.peek()? {
            Some(Token::Keyword(Keyword::Create)) => self.parse_ddl(),
            Some(Token::Keyword(Keyword::Insert)) => self.parse_insert(),
            Some(Token::Keyword(Keyword::Select)) => self.parse_select(),
            Some(token) => Err(Error::Parse(format!("[Parse] Unexpected token {}", token))),
            None => Err(Error::Parse(format!("[Parse] Unexpected end of input"))),
        }
    }

    fn parse_ddl(&mut self) -> Result<Statement> {
        match self.next()? {
            Token::Keyword(Keyword::Create) => match self.next()? {
                Token::Keyword(Keyword::Table) => self.parse_ddl_create_table(),
                token => Err(Error::Parse(format!(
                    "[Parse] Unexpected end of token: {}",
                    token
                ))),
            },
            token => Err(Error::Parse(format!(
                "[Parse] Unexpected end of token: {}",
                token
            ))),
        }
    }

    fn parse_select(&mut self) -> Result<Statement> {
        self.next_expect(Token::Keyword(Keyword::Select))?;
        self.next_expect(Token::Asterisk)?;
        self.next_expect(Token::Keyword(Keyword::From))?;
        let table_name = self.next_ident()?;
        Ok(Statement::Select { table_name })
    }

    fn parse_insert(&mut self) -> Result<Statement> {
        self.next_expect(Token::Keyword(Keyword::Insert))?;
        self.next_expect(Token::Keyword(Keyword::Into))?;
        let table_name = self.next_ident()?;

        // check if insert for certain columns
        let columns = if self.next_if_token(Token::OpenParen).is_some() {
            let mut cols = Vec::new();
            loop {
                cols.push(self.next_ident()?.to_string());
                match self.next()? {
                    Token::CloseParen => break,
                    Token::Comma => continue,
                    token => {
                        return Err(Error::Parse(format!("[Parse] Unexpected token: {}", token)))
                    }
                }
            }
            Some(cols)
        } else {
            None
        };

        self.next_expect(Token::Keyword(Keyword::Values))?;
        let mut values = Vec::new();
        loop {
            self.next_expect(Token::OpenParen)?;
            let mut exprs = Vec::new();
            loop {
                exprs.push(self.parse_expression()?);
                match self.next()? {
                    Token::CloseParen => break,
                    Token::Comma => continue,
                    token => {
                        return Err(Error::Parse(format!("[Parse] Unexpected token: {}", token)))
                    }
                }
            }
            values.push(exprs);
            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }
        Ok(Statement::Insert {
            table_name,
            columns,
            values,
        })
    }

    fn parse_ddl_create_table(&mut self) -> Result<Statement> {
        // expect table name
        let table_name = self.next_ident()?;
        // expect (
        self.next_expect(Token::OpenParen)?;
        let mut columns = Vec::new();

        loop {
            columns.push(self.parse_ddl_column()?);
            //if there is no comma, parse finished
            if self.next_if_token(Token::Comma).is_none() {
                break;
            }
        }
        self.next_expect(Token::CloseParen)?;
        Ok(Statement::CreateTable {
            name: table_name,
            columns,
        })
    }

    fn parse_ddl_column(&mut self) -> Result<Column> {
        let mut column = Column {
            name: self.next_ident()?,
            datatype: match self.next()? {
                Token::Keyword(Keyword::Int) | Token::Keyword(Keyword::Integer) => {
                    DataType::Integer
                }
                Token::Keyword(Keyword::Float) | Token::Keyword(Keyword::Double) => DataType::Float,
                Token::Keyword(Keyword::Bool) | Token::Keyword(Keyword::Boolean) => {
                    DataType::Boolean
                }
                Token::Keyword(Keyword::String)
                | Token::Keyword(Keyword::Text)
                | Token::Keyword(Keyword::Varchar) => DataType::String,
                token => {
                    return Err(Error::Parse(format!(
                        "[Parse] Unexpected datatype: {}",
                        token
                    )));
                }
            },
            nullable: None,
            default: None,
        };
        // parse column default, and if can be null
        while let Some(Token::Keyword(keyword)) = self.next_if_keyword() {
            match keyword {
                Keyword::Null => column.nullable = Some(true),
                Keyword::Not => {
                    self.next_expect(Token::Keyword(Keyword::Null))?;
                    column.nullable = Some(false);
                }
                Keyword::Default => column.default = Some(self.parse_expression()?),
                k => return Err(Error::Parse(format!("[Parse] Unexpected keyword: {}", k))),
            }
        }
        Ok(column)
    }

    fn parse_expression(&mut self) -> Result<ast::Expression> {
        Ok(match self.next()? {
            Token::Number(n) => {
                if n.chars().all(|c| c.is_ascii_digit()) {
                    ast::Consts::Integer(n.parse()?).into()
                } else {
                    ast::Consts::Float(n.parse()?).into()
                }
            }
            Token::String(s) => ast::Consts::String(s).into(),
            Token::Keyword(Keyword::True) => ast::Consts::Boolean(true).into(),
            Token::Keyword(Keyword::False) => ast::Consts::Boolean(false).into(),
            Token::Keyword(Keyword::Null) => ast::Consts::Null.into(),
            token => {
                return Err(Error::Parse(format!(
                    "[Parse] Unexpected expression: {}",
                    token
                )))
            }
        })
    }

    fn peek(&mut self) -> Result<Option<Token>> {
        self.lexer.peek().cloned().transpose()
    }

    fn next(&mut self) -> Result<Token> {
        self.lexer
            .next()
            .unwrap_or_else(|| Err(Error::Parse(format!("[Parse] Unexpected end of input"))))
    }

    fn next_ident(&mut self) -> Result<String> {
        match self.next()? {
            Token::Ident(ident) => Ok(ident),
            token => Err(Error::Parse(format!(
                "[Parse] Expect Ident, got token: {}",
                token
            ))),
        }
    }

    fn next_expect(&mut self, expect: Token) -> Result<()> {
        let token = self.next()?;
        if token != expect {
            return Err(Error::Parse(format!(
                "[Parse] Expect token: {}, got token: {}",
                expect, token
            )));
        }
        Ok(())
    }

    fn next_if<F: Fn(&Token) -> bool>(&mut self, predicate: F) -> Option<Token> {
        self.peek().unwrap_or(None).filter(|t| predicate(t))?;
        self.next().ok()
    }
    fn next_if_keyword(&mut self) -> Option<Token> {
        self.next_if(|t| matches!(t, Token::Keyword(_)))
    }

    fn next_if_token(&mut self, token: Token) -> Option<Token> {
        self.next_if(|t| t == &token)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ast::{Consts, Expression},
        Parser, Statement,
    };
    use crate::error::Result;

    #[test]
    fn test_parser_create_table() -> Result<()> {
        let sql1 = "create table tbl1 (
        a int default 100, b float not null, c varchar null, d bool default true);
        ";
        let stmt1 = Parser::new(sql1).parse()?;

        let sql2 = "create table tbl1 (
            a int default 100, b float not null,       c varchar null, d bool default     true);
            ";
        let stmt2 = Parser::new(sql2).parse()?;

        let sql3 = "create tabl tbl1 (
            a int default 100, b float not null,       c varchar null, d bool default     true);
            ";
        let stmt3 = Parser::new(sql3).parse();
        assert_eq!(stmt1, stmt2);
        assert!(stmt3.is_err());
        Ok(())
    }

    #[test]
    fn test_parser_insert() -> Result<()> {
        let sql1 = "insert into tbl1 values (1,2,3, 'a', true);";
        let stmt1 = Parser::new(sql1).parse()?;
        println!("{:?}", stmt1);
        assert_eq!(
            stmt1,
            Statement::Insert {
                table_name: "tbl1".to_string(),
                columns: None,
                values: vec![vec![
                    Expression::Consts(Consts::Integer(1)),
                    Expression::Consts(Consts::Integer(2)),
                    Expression::Consts(Consts::Integer(3)),
                    Expression::Consts(Consts::String("a".to_string())),
                    Expression::Consts(Consts::Boolean(true))
                ]]
            }
        );

        let sql2 = "insert into tbl2 (a,b,c) values (3, 'a', true), (4, 'b', false);";
        let stmt2 = Parser::new(sql2).parse()?;
        println!("{:?}", stmt2);
        Ok(())
    }
}
