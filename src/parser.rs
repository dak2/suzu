use crate::ast::*;
use crate::diag::Diagnostic;
use crate::lexer::{self, Keyword, Token, TokenKind};
use crate::span::Span;

pub fn parse(source: &str) -> (File, Vec<Diagnostic>) {
    let (tokens, mut diags) = lexer::lex(source);
    let mut parser = Parser::new(source, tokens, 0);
    let file = parser.file();
    diags.append(&mut parser.diags);
    diags.sort_by_key(|d| d.span.start);
    (file, diags)
}

const MAX_NESTING: u32 = 128;

#[derive(Debug)]
struct Recover;

type PResult<T> = Result<T, Recover>;

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
    nesting: u32,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, tokens: Vec<Token>, nesting: u32) -> Parser<'a> {
        debug_assert_eq!(tokens.last().map(|t| &t.kind), Some(&TokenKind::Eof));
        Parser {
            src,
            tokens,
            pos: 0,
            diags: Vec::new(),
            nesting,
        }
    }

    fn nested<T>(&mut self, rule: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.nesting >= MAX_NESTING {
            return Err(self.error("nesting is too deep", self.span()));
        }
        self.nesting += 1;
        let result = rule(self);
        self.nesting -= 1;
        result
    }

    fn depth(&self) -> u32 {
        self.tokens[self.pos].depth
    }

    fn at_line_start(&self) -> bool {
        self.pos == 0 || self.tokens[self.pos - 1].kind == TokenKind::Newline
    }

    fn at_column_zero(&self) -> bool {
        let start = self.span().start as usize;
        start == 0 || self.src.as_bytes().get(start - 1) == Some(&b'\n')
    }

    fn at_unindented_item(&self) -> bool {
        self.at_column_zero() && self.at_item_start()
    }

    fn at_item_start(&self) -> bool {
        let item_kw = |kind: &TokenKind| {
            matches!(
                kind,
                TokenKind::Kw(
                    Keyword::Fn | Keyword::Struct | Keyword::Enum | Keyword::Impl | Keyword::Const
                )
            )
        };
        item_kw(self.peek()) || (self.at_kw(Keyword::Pub) && item_kw(self.peek2()))
    }

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek2(&self) -> &TokenKind {
        let i = (self.pos + 1).min(self.tokens.len() - 1);
        &self.tokens[i].kind
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn prev_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    fn bump(&mut self) -> Span {
        let span = self.span();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        span
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn at_kw(&self, kw: Keyword) -> bool {
        *self.peek() == TokenKind::Kw(kw)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, kw: Keyword) -> bool {
        self.eat(&TokenKind::Kw(kw))
    }

    fn expect(&mut self, kind: &TokenKind) -> PResult<Span> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.error_expected(&kind.describe()))
        }
    }

    fn expect_kw(&mut self, kw: Keyword) -> PResult<Span> {
        self.expect(&TokenKind::Kw(kw))
    }

    fn expect_ident(&mut self) -> PResult<Ident> {
        if let TokenKind::Ident(name) = self.peek() {
            let name = name.clone();
            let span = self.bump();
            Ok(Ident { name, span })
        } else {
            Err(self.error_expected("an identifier"))
        }
    }

    fn error_expected(&mut self, what: &str) -> Recover {
        let found = self.peek().describe();
        self.error(format!("expected {what}, found {found}"), self.span())
    }

    fn error(&mut self, message: impl Into<String>, span: Span) -> Recover {
        self.diags.push(Diagnostic::error(message, span));
        Recover
    }

    fn skip_newlines(&mut self) {
        while self.eat(&TokenKind::Newline) {}
    }

    fn synchronize(&mut self, depth: u32) {
        loop {
            match self.peek() {
                TokenKind::Eof => return,
                TokenKind::Newline if self.depth() == depth => return,
                _ if self.depth() < depth => return,
                _ => {}
            }
            self.bump();
        }
    }

    fn skip_to_next_item(&mut self) {
        while !self.at(&TokenKind::Eof) {
            let top_level = self.depth() == 0 && self.at_line_start();
            // Not only top-level items: an item keyword at column 0 also stops
            // here, so that a missing `}` does not swallow the rest of the file.
            if (top_level && self.at_item_start()) || self.at_unindented_item() {
                return;
            }
            self.bump();
        }
    }

    fn comma_list<T>(
        &mut self,
        close: &TokenKind,
        mut item: impl FnMut(&mut Self) -> PResult<T>,
    ) -> PResult<(Vec<T>, Span)> {
        let mut items = Vec::new();
        loop {
            if self.at(close) {
                return Ok((items, self.bump()));
            }
            items.push(item(self)?);
            if !self.eat(&TokenKind::Comma) {
                let end = self.expect(close)?;
                return Ok((items, end));
            }
        }
    }

    fn sep_list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> PResult<T>,
    ) -> PResult<(Vec<T>, Span)> {
        let mut items = Vec::new();
        self.skip_newlines();
        loop {
            if self.at(&TokenKind::RBrace) {
                return Ok((items, self.bump()));
            }
            items.push(item(self)?);
            if self.eat(&TokenKind::Comma) || self.at(&TokenKind::Newline) {
                self.skip_newlines();
            } else {
                let end = self.expect(&TokenKind::RBrace)?;
                return Ok((items, end));
            }
        }
    }

    fn file(&mut self) -> File {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at(&TokenKind::Eof) {
            match self.item() {
                Ok(item) => {
                    items.push(item);
                    if !self.at(&TokenKind::Eof) && !self.at(&TokenKind::Newline) {
                        self.error_expected("a newline after the item");
                        self.skip_to_next_item();
                    }
                }
                Err(Recover) => self.skip_to_next_item(),
            }
            self.skip_newlines();
        }
        File { items }
    }

    fn item(&mut self) -> PResult<Item> {
        let start = self.span();
        let is_pub = self.eat_kw(Keyword::Pub);
        match self.peek() {
            TokenKind::Kw(Keyword::Fn) => self.fn_def(is_pub, start).map(Item::Fn),
            TokenKind::Kw(Keyword::Struct) => self.struct_def(is_pub, start).map(Item::Struct),
            TokenKind::Kw(Keyword::Enum) => self.enum_def(is_pub, start).map(Item::Enum),
            TokenKind::Kw(Keyword::Const) => self.const_def(is_pub, start).map(Item::Const),
            TokenKind::Kw(Keyword::Impl) if !is_pub => self.impl_def().map(Item::Impl),
            TokenKind::Kw(Keyword::Impl) => Err(self.error("`impl` cannot be `pub`", start)),
            _ => Err(self.error_expected("an item (`fn`, `struct`, `enum`, `impl` or `const`)")),
        }
    }

    fn fn_def(&mut self, is_pub: bool, start: Span) -> PResult<FnDef> {
        self.expect_kw(Keyword::Fn)?;
        let name = self.expect_ident()?;
        let generics = self.opt_generics()?;
        self.expect(&TokenKind::LParen)?;
        let (params, _) = self.comma_list(&TokenKind::RParen, Self::param)?;
        let ret = if self.eat(&TokenKind::Arrow) {
            let mode = if self.eat_kw(Keyword::Borrow) {
                RetMode::Borrow
            } else if self.eat_kw(Keyword::Mut) {
                RetMode::Mut
            } else {
                RetMode::Owned
            };
            Some(RetType {
                mode,
                ty: self.ty()?,
            })
        } else {
            None
        };
        let where_clause = self.opt_where()?;
        let body = self.block()?;
        Ok(FnDef {
            is_pub,
            name,
            generics,
            params,
            ret,
            where_clause,
            span: start.to(body.span),
            body,
        })
    }

    fn param(&mut self) -> PResult<Param> {
        let start = self.span();
        let mode = self.pass_mode();
        if self.eat_kw(Keyword::SelfValue) {
            return Ok(Param {
                mode,
                kind: ParamKind::SelfParam,
                span: start.to(self.prev_span()),
            });
        }
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.ty()?;
        Ok(Param {
            mode,
            span: start.to(ty.span),
            kind: ParamKind::Named { name, ty },
        })
    }

    fn pass_mode(&mut self) -> PassMode {
        if self.eat_kw(Keyword::Mut) {
            PassMode::Mut
        } else if self.eat_kw(Keyword::Take) {
            PassMode::Take
        } else {
            PassMode::Shared
        }
    }

    fn opt_generics(&mut self) -> PResult<Vec<GenericParam>> {
        if !self.at(&TokenKind::LBracket) {
            return Ok(Vec::new());
        }
        let open = self.bump();
        let (params, close) = self.comma_list(&TokenKind::RBracket, Self::generic_param)?;
        if params.is_empty() {
            return Err(self.error("generic parameter list is empty", open.to(close)));
        }
        Ok(params)
    }

    fn generic_param(&mut self) -> PResult<GenericParam> {
        let name = self.expect_ident()?;
        let bounds = if self.eat(&TokenKind::Colon) {
            self.bounds()?
        } else {
            Vec::new()
        };
        Ok(GenericParam {
            span: name.span.to(self.prev_span()),
            name,
            bounds,
        })
    }

    fn bounds(&mut self) -> PResult<Vec<Path>> {
        let mut bounds = vec![self.path()?];
        while self.eat(&TokenKind::Plus) {
            bounds.push(self.path()?);
        }
        Ok(bounds)
    }

    fn opt_where(&mut self) -> PResult<Vec<GenericParam>> {
        if !self.eat_kw(Keyword::Where) {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        loop {
            let item = self.generic_param()?;
            if item.bounds.is_empty() {
                return Err(self.error("a `where` item needs bounds, as in `T: Ord`", item.span));
            }
            items.push(item);
            if !self.eat(&TokenKind::Comma) {
                return Ok(items);
            }
        }
    }

    fn path(&mut self) -> PResult<Path> {
        let first = self.expect_ident()?;
        let mut span = first.span;
        let mut segments = vec![first];
        while self.at(&TokenKind::Dot) {
            self.bump();
            let seg = self.expect_ident()?;
            span = span.to(seg.span);
            segments.push(seg);
        }
        Ok(Path { segments, span })
    }

    fn struct_def(&mut self, is_pub: bool, start: Span) -> PResult<StructDef> {
        self.expect_kw(Keyword::Struct)?;
        let name = self.expect_ident()?;
        let generics = self.opt_generics()?;
        let derives = self.opt_derives()?;
        let layout = if self.eat_kw(Keyword::Layout) {
            self.expect(&TokenKind::LParen)?;
            let repr = self.expect_ident()?;
            self.expect(&TokenKind::RParen)?;
            Some(repr)
        } else {
            None
        };
        self.expect(&TokenKind::LBrace)?;
        let (fields, end) = self.sep_list(Self::field)?;
        Ok(StructDef {
            is_pub,
            name,
            generics,
            derives,
            layout,
            fields,
            span: start.to(end),
        })
    }

    fn opt_derives(&mut self) -> PResult<Vec<Path>> {
        if !self.eat_kw(Keyword::Derives) {
            return Ok(Vec::new());
        }
        self.expect(&TokenKind::LParen)?;
        let (paths, _) = self.comma_list(&TokenKind::RParen, Self::path)?;
        Ok(paths)
    }

    fn field(&mut self) -> PResult<Field> {
        let start = self.span();
        let is_pub = self.eat_kw(Keyword::Pub);
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.ty()?;
        Ok(Field {
            is_pub,
            name,
            span: start.to(ty.span),
            ty,
        })
    }

    fn enum_def(&mut self, is_pub: bool, start: Span) -> PResult<EnumDef> {
        self.expect_kw(Keyword::Enum)?;
        let name = self.expect_ident()?;
        let generics = self.opt_generics()?;
        let derives = self.opt_derives()?;
        self.expect(&TokenKind::LBrace)?;
        let (variants, end) = self.sep_list(Self::variant)?;
        Ok(EnumDef {
            is_pub,
            name,
            generics,
            derives,
            variants,
            span: start.to(end),
        })
    }

    fn variant(&mut self) -> PResult<Variant> {
        let name = self.expect_ident()?;
        let fields = if self.eat(&TokenKind::LParen) {
            let (fields, close) = self.comma_list(&TokenKind::RParen, Self::ty)?;
            if fields.is_empty() {
                return Err(self.error(
                    "a variant without fields is written without `()`",
                    name.span.to(close),
                ));
            }
            fields
        } else {
            Vec::new()
        };
        Ok(Variant {
            span: name.span.to(self.prev_span()),
            name,
            fields,
        })
    }

    fn impl_def(&mut self) -> PResult<ImplDef> {
        let start = self.expect_kw(Keyword::Impl)?;
        let generics = self.opt_generics()?;
        let first = self.ty()?;
        let (protocol, target) = if self.eat_kw(Keyword::For) {
            (Some(first), self.ty()?)
        } else {
            (None, first)
        };
        let where_clause = self.opt_where()?;
        let inner = self.depth() + 1;
        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        self.skip_newlines();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) && self.depth() >= inner {
            let fn_start = self.span();
            let is_pub = self.eat_kw(Keyword::Pub);
            let method = if self.at_kw(Keyword::Fn) {
                self.fn_def(is_pub, fn_start)
            } else {
                Err(self.error_expected("`fn` (only methods can appear in `impl`)"))
            };
            match method {
                Ok(m) => {
                    methods.push(m);
                    if !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Newline) {
                        self.error_expected("a newline after the method");
                        self.synchronize(inner);
                    }
                }
                // A method that failed and stopped at a column-0 item lost its
                // `}`; synchronizing would swallow every item that follows.
                Err(Recover) if self.at_unindented_item() => return Err(Recover),
                Err(Recover) => self.synchronize(inner),
            }
            self.skip_newlines();
        }
        let end = self.expect(&TokenKind::RBrace)?;
        Ok(ImplDef {
            generics,
            protocol,
            target,
            where_clause,
            methods,
            span: start.to(end),
        })
    }

    fn const_def(&mut self, is_pub: bool, start: Span) -> PResult<ConstDef> {
        self.expect_kw(Keyword::Const)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.ty()?;
        self.expect(&TokenKind::Eq)?;
        let value = self.expr()?;
        Ok(ConstDef {
            is_pub,
            name,
            ty,
            span: start.to(value.span),
            value,
        })
    }

    fn ty(&mut self) -> PResult<Type> {
        self.nested(Self::ty_unguarded)
    }

    fn ty_unguarded(&mut self) -> PResult<Type> {
        let start = self.span();
        if self.at_kw(Keyword::Fn) || self.at_kw(Keyword::Block) {
            return self.callable_type();
        }
        let path = self.path()?;
        let args = if self.eat(&TokenKind::LBracket) {
            let (args, close) = self.comma_list(&TokenKind::RBracket, Self::ty)?;
            if args.is_empty() {
                return Err(self.error("type argument list is empty", start.to(close)));
            }
            args
        } else {
            Vec::new()
        };
        let mut ty = Type {
            kind: TypeKind::Named { path, args },
            span: start.to(self.prev_span()),
        };
        if self.eat(&TokenKind::Question) {
            ty = Type {
                kind: TypeKind::Optional(Box::new(ty)),
                span: start.to(self.prev_span()),
            };
        }
        Ok(ty)
    }

    fn callable_type(&mut self) -> PResult<Type> {
        let start = self.span();
        let is_block = self.at_kw(Keyword::Block);
        self.bump();
        self.expect(&TokenKind::LParen)?;
        let (params, _) = self.comma_list(&TokenKind::RParen, |p| {
            let mode = p.pass_mode();
            Ok(ParamType { mode, ty: p.ty()? })
        })?;
        let ret = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(self.ty()?))
        } else {
            None
        };
        let kind = if is_block {
            TypeKind::Block { params, ret }
        } else {
            TypeKind::Fn { params, ret }
        };
        Ok(Type {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    fn block(&mut self) -> PResult<Block> {
        self.nested(Self::block_unguarded)
    }

    fn block_unguarded(&mut self) -> PResult<Block> {
        let inner = self.depth() + 1;
        let start = self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_newlines();
        loop {
            let ended = matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof)
                || self.depth() < inner
                // Not parsed as a statement: an item can never be one, so at
                // column 0 it means the block is missing its `}`.
                || self.at_unindented_item();
            if ended {
                break;
            }
            match self.stmt() {
                Ok(stmt) => {
                    stmts.push(stmt);
                    if !matches!(
                        self.peek(),
                        TokenKind::RBrace | TokenKind::Newline | TokenKind::Eof
                    ) {
                        self.error_expected("a newline or `}` after the statement");
                        self.synchronize(inner);
                    }
                }
                Err(Recover) => self.synchronize(inner),
            }
            self.skip_newlines();
        }
        let end = self.expect(&TokenKind::RBrace)?;
        Ok(Block {
            stmts,
            span: start.to(end),
        })
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.span();
        let kind = match self.peek() {
            TokenKind::Kw(Keyword::Else) => {
                return Err(self.error(
                    "`else` must be on the same line as the closing `}`",
                    self.span(),
                ));
            }
            TokenKind::Kw(Keyword::Var) => {
                self.bump();
                let name = self.expect_ident()?;
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.ty()?)
                } else {
                    None
                };
                let (init, else_clause) = if self.eat(&TokenKind::Eq) {
                    (Some(self.expr()?), self.opt_else_clause()?)
                } else {
                    (None, None)
                };
                StmtKind::Var {
                    name,
                    ty,
                    init,
                    else_clause,
                }
            }
            TokenKind::Kw(Keyword::With) => {
                self.bump();
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Eq)?;
                let value = self.expr()?;
                let else_clause = self.opt_else_clause()?;
                StmtKind::With {
                    name,
                    value,
                    else_clause,
                }
            }
            TokenKind::Kw(Keyword::Return) => {
                self.bump();
                StmtKind::Return(self.opt_return_value()?)
            }
            TokenKind::Kw(Keyword::Break) => {
                self.bump();
                StmtKind::Break
            }
            TokenKind::Kw(Keyword::Continue) => {
                self.bump();
                StmtKind::Continue
            }
            TokenKind::Kw(Keyword::If) => StmtKind::If(self.if_stmt()?),
            TokenKind::Kw(Keyword::While) => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                StmtKind::While { cond, body }
            }
            TokenKind::Kw(Keyword::For) => {
                self.bump();
                let is_mut = self.eat_kw(Keyword::Mut);
                let binding = self.expect_ident()?;
                self.expect_kw(Keyword::In)?;
                let iter = self.expr()?;
                let body = self.block()?;
                StmtKind::For {
                    is_mut,
                    binding,
                    iter,
                    body,
                }
            }
            TokenKind::Kw(Keyword::Co) => {
                self.bump();
                StmtKind::Co(self.postfix_expr()?)
            }
            TokenKind::LBrace => StmtKind::Block(self.block()?),
            _ => {
                let target = self.expr()?;
                match assign_op(self.peek()) {
                    Some(op) => {
                        self.bump();
                        let value = self.expr()?;
                        StmtKind::Assign { target, op, value }
                    }
                    None => StmtKind::Expr(target),
                }
            }
        };
        Ok(Stmt {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    fn opt_return_value(&mut self) -> PResult<Option<Expr>> {
        match self.peek() {
            TokenKind::Newline | TokenKind::RBrace | TokenKind::Comma | TokenKind::Eof => Ok(None),
            _ => Ok(Some(self.expr()?)),
        }
    }

    fn opt_else_clause(&mut self) -> PResult<Option<ElseClause>> {
        if !self.at_kw(Keyword::Else) {
            return Ok(None);
        }
        let start = self.bump();
        let binding = if self.eat(&TokenKind::Pipe) {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Pipe)?;
            Some(name)
        } else {
            None
        };
        let body = self.block()?;
        Ok(Some(ElseClause {
            binding,
            span: start.to(body.span),
            body,
        }))
    }

    fn if_stmt(&mut self) -> PResult<IfStmt> {
        let start = self.expect_kw(Keyword::If)?;
        let cond = self.expr()?;
        let then_block = self.block()?;
        let else_branch = if self.eat_kw(Keyword::Else) {
            if self.at_kw(Keyword::If) {
                Some(ElseBranch::If(Box::new(self.nested(Self::if_stmt)?)))
            } else {
                Some(ElseBranch::Block(self.block()?))
            }
        } else {
            None
        };
        Ok(IfStmt {
            cond,
            then_block,
            else_branch,
            span: start.to(self.prev_span()),
        })
    }

    fn expr(&mut self) -> PResult<Expr> {
        self.binary(0)
    }

    fn binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut lhs = self.unary_expr()?;
        while let Some((op, prec)) = binary_op(self.peek()) {
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.binary(prec + 1)?;
            lhs = Expr {
                span: lhs.span.to(rhs.span),
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
            if prec == CMP_PREC && binary_op(self.peek()).is_some_and(|(_, p)| p == CMP_PREC) {
                return Err(self.error(
                    "comparison operators cannot be chained; use `&&`",
                    self.span(),
                ));
            }
        }
        Ok(lhs)
    }

    fn unary_expr(&mut self) -> PResult<Expr> {
        self.nested(Self::unary_expr_unguarded)
    }

    fn unary_expr_unguarded(&mut self) -> PResult<Expr> {
        let start = self.span();
        let op = match self.peek() {
            TokenKind::Minus => UnaryOp::Neg,
            TokenKind::Bang => UnaryOp::Not,
            TokenKind::Tilde => UnaryOp::BitNot,
            TokenKind::Kw(Keyword::Take) => {
                self.bump();
                let operand = self.postfix_expr()?;
                return Ok(Expr {
                    span: start.to(operand.span),
                    kind: ExprKind::Take(Box::new(operand)),
                });
            }
            _ => return self.postfix_expr(),
        };
        self.bump();
        let operand = self.unary_expr()?;
        Ok(Expr {
            span: start.to(operand.span),
            kind: ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
        })
    }

    fn postfix_expr(&mut self) -> PResult<Expr> {
        let mut expr = self.primary()?;
        loop {
            let start = expr.span;
            let kind = match self.peek() {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.expect_ident()?;
                    ExprKind::Field {
                        base: Box::new(expr),
                        name,
                    }
                }
                TokenKind::LParen => {
                    self.bump();
                    let (args, _) = self.comma_list(&TokenKind::RParen, Self::arg)?;
                    ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    }
                }
                TokenKind::LBracket => {
                    let open = self.bump();
                    let (args, close) = self.comma_list(&TokenKind::RBracket, Self::bracket_arg)?;
                    if args.is_empty() {
                        return Err(
                            self.error("expected an index or type argument", open.to(close))
                        );
                    }
                    ExprKind::Index {
                        base: Box::new(expr),
                        args,
                    }
                }
                TokenKind::Question => {
                    self.bump();
                    ExprKind::Try(Box::new(expr))
                }
                _ => return Ok(expr),
            };
            expr = Expr {
                span: start.to(self.prev_span()),
                kind,
            };
        }
    }

    fn arg(&mut self) -> PResult<Arg> {
        let start = self.span();
        let label =
            if matches!(self.peek(), TokenKind::Ident(_)) && *self.peek2() == TokenKind::Colon {
                let label = self.expect_ident()?;
                self.bump();
                Some(label)
            } else {
                None
            };
        let is_mut = self.eat_kw(Keyword::Mut);
        let value = self.expr()?;
        Ok(Arg {
            label,
            is_mut,
            span: start.to(value.span),
            value,
        })
    }

    fn bracket_arg(&mut self) -> PResult<BracketArg> {
        if self.at_kw(Keyword::Fn) || self.at_kw(Keyword::Block) {
            Ok(BracketArg::Type(self.callable_type()?))
        } else {
            Ok(BracketArg::Expr(self.expr()?))
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let start = self.span();
        let atom = match self.peek() {
            TokenKind::Int(n) => Some(ExprKind::Int(*n)),
            TokenKind::Float(f) => Some(ExprKind::Float(*f)),
            TokenKind::Kw(Keyword::True) => Some(ExprKind::Bool(true)),
            TokenKind::Kw(Keyword::False) => Some(ExprKind::Bool(false)),
            TokenKind::Kw(Keyword::SelfValue) => Some(ExprKind::SelfValue),
            TokenKind::Ident(name) => Some(ExprKind::Ident(name.clone())),
            _ => None,
        };
        if let Some(kind) = atom {
            self.bump();
            return Ok(Expr { kind, span: start });
        }
        let kind = match self.peek() {
            TokenKind::Str(_) => {
                let parts = self.take_str();
                ExprKind::Str(self.string_parts(parts))
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.expr()?;
                self.expect(&TokenKind::RParen)?;
                ExprKind::Paren(Box::new(inner))
            }
            TokenKind::LBracket => {
                self.bump();
                let (elems, _) = self.comma_list(&TokenKind::RBracket, Self::expr)?;
                ExprKind::Array(elems)
            }
            TokenKind::Pipe => {
                self.bump();
                let (params, _) = self.comma_list(&TokenKind::Pipe, |p| {
                    let mode = p.pass_mode();
                    let name = p.expect_ident()?;
                    Ok(BlockParam { mode, name })
                })?;
                let body = self.block()?;
                ExprKind::BlockLit { params, body }
            }
            TokenKind::OrOr => {
                self.bump();
                let body = self.block()?;
                ExprKind::BlockLit {
                    params: Vec::new(),
                    body,
                }
            }
            TokenKind::Kw(Keyword::Match) => self.match_expr()?,
            TokenKind::Kw(Keyword::Scope) => {
                self.bump();
                let args = if self.eat(&TokenKind::LParen) {
                    Some(self.comma_list(&TokenKind::RParen, Self::arg)?.0)
                } else {
                    None
                };
                let body = self.block()?;
                ExprKind::Scope { args, body }
            }
            _ => return Err(self.error_expected("an expression")),
        };
        Ok(Expr {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    fn take_str(&mut self) -> Vec<lexer::StrPart> {
        let TokenKind::Str(parts) = &mut self.tokens[self.pos].kind else {
            unreachable!("take_str called off a string token")
        };
        let parts = std::mem::take(parts);
        self.bump();
        parts
    }

    fn string_parts(&mut self, parts: Vec<lexer::StrPart>) -> Vec<StrPart> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                lexer::StrPart::Text(text) => out.push(StrPart::Text(text)),
                lexer::StrPart::Interp(tokens) => {
                    let mut sub = Parser::new(self.src, tokens, self.nesting);
                    if let Some(expr) = sub.interp_expr() {
                        out.push(StrPart::Interp(expr));
                    }
                    self.diags.append(&mut sub.diags);
                }
            }
        }
        out
    }

    fn interp_expr(&mut self) -> Option<Expr> {
        if self.at(&TokenKind::RParen) {
            let span = self.span();
            self.error("empty string interpolation", span);
            return None;
        }
        let expr = self.expr().ok()?;
        self.expect(&TokenKind::RParen).ok()?;
        Some(expr)
    }

    fn match_expr(&mut self) -> PResult<ExprKind> {
        self.expect_kw(Keyword::Match)?;
        let scrutinee = self.expr()?;
        self.expect(&TokenKind::LBrace)?;
        let (arms, _) = self.sep_list(Self::arm)?;
        Ok(ExprKind::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    fn arm(&mut self) -> PResult<Arm> {
        let pattern = self.pattern()?;
        self.expect(&TokenKind::FatArrow)?;
        let body = match self.peek() {
            TokenKind::LBrace => ArmBody::Block(self.block()?),
            TokenKind::Kw(Keyword::Return) => {
                self.bump();
                ArmBody::Return(self.opt_return_value()?)
            }
            TokenKind::Kw(Keyword::Break) => {
                self.bump();
                ArmBody::Break
            }
            TokenKind::Kw(Keyword::Continue) => {
                self.bump();
                ArmBody::Continue
            }
            _ => ArmBody::Expr(self.expr()?),
        };
        Ok(Arm {
            span: pattern.span.to(self.prev_span()),
            pattern,
            body,
        })
    }

    fn pattern(&mut self) -> PResult<Pattern> {
        self.nested(Self::pattern_unguarded)
    }

    fn pattern_unguarded(&mut self) -> PResult<Pattern> {
        let start = self.span();
        let kind = match self.peek() {
            TokenKind::Ident(name) if name == "_" => {
                self.bump();
                PatternKind::Wildcard
            }
            TokenKind::Minus => {
                self.bump();
                match *self.peek() {
                    TokenKind::Int(value) => {
                        self.bump();
                        PatternKind::Int {
                            negative: true,
                            value,
                        }
                    }
                    _ => return Err(self.error_expected("an integer after `-`")),
                }
            }
            &TokenKind::Int(value) => {
                self.bump();
                PatternKind::Int {
                    negative: false,
                    value,
                }
            }
            TokenKind::Str(parts) => {
                let text = match parts.as_slice() {
                    [lexer::StrPart::Text(text)] => Some(text.clone()),
                    _ => None,
                };
                self.bump();
                match text {
                    Some(text) => PatternKind::Str(text),
                    None => {
                        return Err(self.error(
                            "string patterns cannot contain interpolation",
                            self.prev_span(),
                        ));
                    }
                }
            }
            TokenKind::Kw(Keyword::True) => {
                self.bump();
                PatternKind::Bool(true)
            }
            TokenKind::Kw(Keyword::False) => {
                self.bump();
                PatternKind::Bool(false)
            }
            TokenKind::Ident(_) => {
                let path = self.path()?;
                let args = if self.eat(&TokenKind::LParen) {
                    Some(self.comma_list(&TokenKind::RParen, Self::pattern)?.0)
                } else {
                    None
                };
                PatternKind::Path { path, args }
            }
            _ => return Err(self.error_expected("a pattern")),
        };
        Ok(Pattern {
            kind,
            span: start.to(self.prev_span()),
        })
    }
}

const CMP_PREC: u8 = 3;

fn binary_op(kind: &TokenKind) -> Option<(BinaryOp, u8)> {
    use BinaryOp::*;
    let op = match kind {
        TokenKind::OrOr => (Or, 1),
        TokenKind::AndAnd => (And, 2),
        TokenKind::EqEq => (Eq, CMP_PREC),
        TokenKind::Ne => (Ne, CMP_PREC),
        TokenKind::Lt => (Lt, CMP_PREC),
        TokenKind::Le => (Le, CMP_PREC),
        TokenKind::Gt => (Gt, CMP_PREC),
        TokenKind::Ge => (Ge, CMP_PREC),
        TokenKind::Pipe => (BitOr, 4),
        TokenKind::Caret => (BitXor, 5),
        TokenKind::Amp => (BitAnd, 6),
        TokenKind::Shl => (Shl, 7),
        TokenKind::Shr => (Shr, 7),
        TokenKind::Plus => (Add, 8),
        TokenKind::Minus => (Sub, 8),
        TokenKind::Star => (Mul, 9),
        TokenKind::Slash => (Div, 9),
        TokenKind::Percent => (Rem, 9),
        _ => return None,
    };
    Some(op)
}

fn assign_op(kind: &TokenKind) -> Option<AssignOp> {
    let op = match kind {
        TokenKind::Eq => AssignOp::Assign,
        TokenKind::PlusEq => AssignOp::Add,
        TokenKind::MinusEq => AssignOp::Sub,
        TokenKind::StarEq => AssignOp::Mul,
        TokenKind::SlashEq => AssignOp::Div,
        TokenKind::PercentEq => AssignOp::Rem,
        TokenKind::AmpEq => AssignOp::BitAnd,
        TokenKind::PipeEq => AssignOp::BitOr,
        TokenKind::CaretEq => AssignOp::BitXor,
        TokenKind::ShlEq => AssignOp::Shl,
        TokenKind::ShrEq => AssignOp::Shr,
        _ => return None,
    };
    Some(op)
}

#[cfg(test)]
mod tests;
