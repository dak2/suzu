use crate::span::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Fn(FnDef),
    Struct(StructDef),
    Enum(EnumDef),
    Impl(ImplDef),
    Const(ConstDef),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDef {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ret: Option<RetType>,
    pub where_clause: Vec<GenericParam>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenericParam {
    pub name: Ident,
    pub bounds: Vec<Path>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassMode {
    Shared,
    Mut,
    Take,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub mode: PassMode,
    pub kind: ParamKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    SelfParam,
    Named { name: Ident, ty: Type },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetMode {
    Owned,
    Borrow,
    Mut,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetType {
    pub mode: RetMode,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDef {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub derives: Vec<Path>,
    pub layout: Option<Ident>,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub is_pub: bool,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDef {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub derives: Vec<Path>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: Ident,
    pub fields: Vec<Type>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplDef {
    pub generics: Vec<GenericParam>,
    pub protocol: Option<Type>,
    pub target: Type,
    pub where_clause: Vec<GenericParam>,
    pub methods: Vec<FnDef>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstDef {
    pub is_pub: bool,
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    Named {
        path: Path,
        args: Vec<Type>,
    },
    Optional(Box<Type>),
    Block {
        params: Vec<ParamType>,
        ret: Option<Box<Type>>,
    },
    Fn {
        params: Vec<ParamType>,
        ret: Option<Box<Type>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamType {
    pub mode: PassMode,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Var {
        name: Ident,
        ty: Option<Type>,
        init: Option<Expr>,
        else_clause: Option<ElseClause>,
    },
    With {
        name: Ident,
        value: Expr,
        else_clause: Option<ElseClause>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    If(IfStmt),
    While {
        cond: Expr,
        body: Block,
    },
    For {
        is_mut: bool,
        binding: Ident,
        iter: Expr,
        body: Block,
    },
    Co(Expr),
    Block(Block),
    Expr(Expr),
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElseClause {
    pub binding: Option<Ident>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_block: Block,
    pub else_branch: Option<ElseBranch>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ElseBranch {
    If(Box<IfStmt>),
    Block(Block),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(u64),
    Float(f64),
    Str(Vec<StrPart>),
    Bool(bool),
    Ident(String),
    SelfValue,
    Paren(Box<Expr>),
    Array(Vec<Expr>),
    BlockLit {
        params: Vec<BlockParam>,
        body: Block,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    Scope {
        args: Vec<Arg>,
        body: Block,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Take(Box<Expr>),
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Field {
        base: Box<Expr>,
        name: Ident,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    Index {
        base: Box<Expr>,
        args: Vec<BracketArg>,
    },
    Try(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart {
    Text(String),
    Interp(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinaryOp {
    pub fn as_str(self) -> &'static str {
        use BinaryOp::*;
        match self {
            Or => "||",
            And => "&&",
            Eq => "==",
            Ne => "!=",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            BitOr => "|",
            BitXor => "^",
            BitAnd => "&",
            Shl => "<<",
            Shr => ">>",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub label: Option<Ident>,
    pub is_mut: bool,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BracketArg {
    Expr(Expr),
    Type(Type),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockParam {
    pub mode: PassMode,
    pub name: Ident,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub pattern: Pattern,
    pub body: ArmBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArmBody {
    Block(Block),
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    Wildcard,
    Int {
        negative: bool,
        value: u64,
    },
    Str(String),
    Bool(bool),
    Path {
        path: Path,
        args: Option<Vec<Pattern>>,
    },
}
