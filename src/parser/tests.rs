use super::*;

fn parse_ok(src: &str) -> File {
    let (file, diags) = parse(src);
    assert!(
        diags.is_empty(),
        "unexpected diagnostics for {src:?}: {diags:#?}"
    );
    file
}

fn errors(src: &str) -> Vec<String> {
    parse(src).1.into_iter().map(|d| d.message).collect()
}

fn stmts(src: &str) -> Vec<Stmt> {
    let file = parse_ok(&format!("fn main() {{\n{src}\n}}"));
    let Item::Fn(f) = file.items.into_iter().next().unwrap() else {
        panic!()
    };
    f.body.stmts
}

fn expr(src: &str) -> Expr {
    let mut s = stmts(src);
    assert_eq!(s.len(), 1);
    match s.remove(0).kind {
        StmtKind::Expr(e) => e,
        other => panic!("expected an expression statement, got {other:?}"),
    }
}

fn sx(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Float(f) => format!("{f:?}"),
        ExprKind::Str(parts) => {
            let inner: Vec<String> = parts
                .iter()
                .map(|p| match p {
                    StrPart::Text(t) => format!("{t:?}"),
                    StrPart::Interp(e) => sx(e),
                })
                .collect();
            format!("(str {})", inner.join(" "))
        }
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Ident(name) => name.clone(),
        ExprKind::SelfValue => "self".into(),
        ExprKind::Paren(inner) => sx(inner),
        ExprKind::Array(elems) => format!("[{}]", list(elems.iter().map(sx))),
        ExprKind::BlockLit { params, body } => {
            let params: Vec<_> = params.iter().map(|p| p.name.name.clone()).collect();
            format!("(block |{}| {})", params.join(","), body.stmts.len())
        }
        ExprKind::Match { scrutinee, arms } => format!("(match {} {})", sx(scrutinee), arms.len()),
        ExprKind::Scope { args, .. } => format!("(scope {})", args.as_ref().map_or(0, Vec::len)),
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
                UnaryOp::BitNot => "~",
            };
            format!("({op} {})", sx(operand))
        }
        ExprKind::Take(inner) => format!("(take {})", sx(inner)),
        ExprKind::Binary { op, lhs, rhs } => format!("({} {} {})", op.as_str(), sx(lhs), sx(rhs)),
        ExprKind::Field { base, name } => format!("{}.{}", sx(base), name.name),
        ExprKind::Call { callee, args } => {
            let args = args.iter().map(|a| {
                let label = a
                    .label
                    .as_ref()
                    .map_or(String::new(), |l| format!("{}: ", l.name));
                let m = if a.is_mut { "mut " } else { "" };
                format!("{label}{m}{}", sx(&a.value))
            });
            format!("{}({})", sx(callee), list(args))
        }
        ExprKind::Index { base, args } => {
            let args = args.iter().map(|a| match a {
                BracketArg::Expr(e) => sx(e),
                BracketArg::Type(_) => "<type>".into(),
            });
            format!("{}[{}]", sx(base), list(args))
        }
        ExprKind::Try(inner) => format!("{}?", sx(inner)),
    }
}

fn list(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join(", ")
}

#[test]
fn precedence() {
    assert_eq!(sx(&expr("1 + 2 * 3")), "(+ 1 (* 2 3))");
    assert_eq!(sx(&expr("1 - 2 - 3")), "(- (- 1 2) 3)");
    assert_eq!(sx(&expr("a || b && c == d")), "(|| a (&& b (== c d)))");
    assert_eq!(
        sx(&expr("a | b ^ c & d << 1")),
        "(| a (^ b (& c (<< d 1))))"
    );
    assert_eq!(
        sx(&expr("x == none || v > best")),
        "(|| (== x none) (> v best))"
    );
    assert_eq!(sx(&expr("a * (100 - p) / 100")), "(/ (* a (- 100 p)) 100)");
}

#[test]
fn unary_and_take() {
    assert_eq!(sx(&expr("-a.b")), "(- a.b)");
    assert_eq!(sx(&expr("!!x")), "(! (! x))");
    assert_eq!(sx(&expr("~x + 1")), "(+ (~ x) 1)");
    assert_eq!(sx(&expr("f(take name)")), "f((take name))");
}

#[test]
fn comparisons_do_not_chain() {
    assert_eq!(
        errors("fn f() {\n    a < b < c\n}"),
        vec!["comparison operators cannot be chained; use `&&`"]
    );
}

#[test]
fn postfix_chains() {
    assert_eq!(sx(&expr("a.b(c)[0]?.d")), "a.b(c)[0]?.d");
    assert_eq!(sx(&expr("Map[String, Int]()")), "Map[String, Int]()");
    assert_eq!(sx(&expr("f[fn(Int) -> Int]")), "f[<type>]");
}

#[test]
fn call_arguments() {
    assert_eq!(
        sx(&expr("User(id: 1, name: take n, mut xs, x,)")),
        "User(id: 1, name: (take n), mut xs, x)"
    );
}

#[test]
fn multiline_method_chain() {
    assert_eq!(
        sx(&expr("xs\n    .filter(f)\n    .map(g)")),
        "xs.filter(f).map(g)"
    );
}

#[test]
fn literals() {
    assert_eq!(
        sx(&expr("[1, 2.5, true, \"s\"]")),
        "[1, 2.5, true, (str \"s\")]"
    );
    assert_eq!(sx(&expr("[]")), "[]");
    assert_eq!(
        sx(&expr(r#""a \(x + 1) b \(f("c"))""#)),
        r#"(str "a " (+ x 1) " b " f((str "c")))"#
    );
}

#[test]
fn bad_interpolation_is_reported() {
    assert_eq!(
        errors("fn f() {\n    print(\"\\()\")\n}"),
        vec!["empty string interpolation"]
    );
    assert_eq!(errors("fn f() {\n    print(\"\\(1 +)\")\n}").len(), 1);
}

#[test]
fn block_literals() {
    assert_eq!(
        sx(&expr("xs.each(|x| {\n    print(x)\n})")),
        "xs.each((block |x| 1))"
    );
    assert_eq!(sx(&expr("run(|| { f() })")), "run((block || 1))");
    assert_eq!(
        sx(&expr("f(|a, b| { return a < b })")),
        "f((block |a,b| 1))"
    );
    let e = expr("f(|mut a, take b| {})");
    let ExprKind::Call { args, .. } = e.kind else {
        panic!()
    };
    let ExprKind::BlockLit { params, .. } = &args[0].value.kind else {
        panic!()
    };
    assert_eq!(params[0].mode, PassMode::Mut);
    assert_eq!(params[1].mode, PassMode::Take);
}

#[test]
fn match_arms() {
    let e = expr(
        "match x {\n    some(v) => print(v),\n    none => return\n    Shape.Circle(-1, _) => { f() }\n    \"s\" => break,\n    true => continue\n}",
    );
    let ExprKind::Match { arms, .. } = e.kind else {
        panic!()
    };
    assert_eq!(arms.len(), 5);
    assert!(matches!(arms[1].body, ArmBody::Return(None)));
    assert!(matches!(arms[2].body, ArmBody::Block(_)));
    let PatternKind::Path {
        path,
        args: Some(args),
    } = &arms[2].pattern.kind
    else {
        panic!()
    };
    assert_eq!(path.segments.len(), 2);
    assert_eq!(
        args[0].kind,
        PatternKind::Int {
            negative: true,
            value: 1
        }
    );
    assert_eq!(args[1].kind, PatternKind::Wildcard);
    assert_eq!(arms[3].pattern.kind, PatternKind::Str("s".into()));
}

#[test]
fn scope_expression() {
    assert_eq!(
        sx(&expr("scope(limit: 4) {\n    co work()\n}")),
        "(scope 1)"
    );
    assert_eq!(sx(&expr("scope {}")), "(scope 0)");
}

#[test]
fn statements() {
    let s = stmts(
        "var a\nvar b: Int? = none\nvar i = f() else |e| {\n    return\n}\nwith u = g() else {\n    continue\n}\nx.y += 1\nbreak\nco job(1)\n{\n    z = 2\n}",
    );
    assert_eq!(s.len(), 8);
    assert!(matches!(
        &s[0].kind,
        StmtKind::Var {
            ty: None,
            init: None,
            ..
        }
    ));
    let StmtKind::Var { ty: Some(ty), .. } = &s[1].kind else {
        panic!()
    };
    assert!(matches!(ty.kind, TypeKind::Optional(_)));
    let StmtKind::Var {
        else_clause: Some(ec),
        ..
    } = &s[2].kind
    else {
        panic!()
    };
    assert_eq!(ec.binding.as_ref().unwrap().name, "e");
    assert!(matches!(
        &s[3].kind,
        StmtKind::With {
            else_clause: Some(_),
            ..
        }
    ));
    assert!(matches!(
        &s[4].kind,
        StmtKind::Assign {
            op: AssignOp::Add,
            ..
        }
    ));
    assert!(matches!(s[5].kind, StmtKind::Break));
    assert!(matches!(s[6].kind, StmtKind::Co(_)));
    assert!(matches!(s[7].kind, StmtKind::Block(_)));
}

#[test]
fn control_flow() {
    let s = stmts(
        "if a {\n} else if b {\n    f()\n} else {\n}\nwhile i < 10 {\n    i += 1\n}\nfor mut s in sales {\n}",
    );
    let StmtKind::If(i) = &s[0].kind else {
        panic!()
    };
    let Some(ElseBranch::If(inner)) = &i.else_branch else {
        panic!()
    };
    assert!(matches!(inner.else_branch, Some(ElseBranch::Block(_))));
    assert!(matches!(s[1].kind, StmtKind::While { .. }));
    assert!(matches!(s[2].kind, StmtKind::For { is_mut: true, .. }));
}

#[test]
fn items() {
    let file = parse_ok(
        "pub struct Point[T: Num + Copy] derives(Copy, Printable) layout(C) {\n    pub x: T, y: T\n}\n\n\
         enum Shape {\n    Circle(Float)\n    Rect(Float, Float),\n    Empty\n}\n\
         impl[T] Printable for Point[T] where T: Printable {\n    pub fn print(self) {\n    }\n\n    fn get(mut self, take i: Int) -> mut T {\n    }\n}\n\
         const MAX: Int = 1 << 10\n\
         fn apply(f: fn(Int) -> Int, b: block(mut Sale)) -> borrow Int? where T: Ord {\n}\n",
    );
    assert_eq!(file.items.len(), 5);

    let Item::Struct(s) = &file.items[0] else {
        panic!()
    };
    assert!(s.is_pub);
    assert_eq!(s.generics[0].bounds.len(), 2);
    assert_eq!(s.derives.len(), 2);
    assert_eq!(s.layout.as_ref().unwrap().name, "C");
    assert_eq!(s.fields.len(), 2);
    assert!(s.fields[0].is_pub && !s.fields[1].is_pub);

    let Item::Enum(e) = &file.items[1] else {
        panic!()
    };
    let counts: Vec<_> = e.variants.iter().map(|v| v.fields.len()).collect();
    assert_eq!(counts, vec![1, 2, 0]);

    let Item::Impl(i) = &file.items[2] else {
        panic!()
    };
    assert!(i.protocol.is_some());
    assert_eq!(i.where_clause.len(), 1);
    assert_eq!(i.methods.len(), 2);
    assert!(i.methods[0].is_pub);
    let m = &i.methods[1];
    assert_eq!(m.params[0].mode, PassMode::Mut);
    assert_eq!(m.params[0].kind, ParamKind::SelfParam);
    assert_eq!(m.params[1].mode, PassMode::Take);
    assert_eq!(m.ret.as_ref().unwrap().mode, RetMode::Mut);

    let Item::Const(c) = &file.items[3] else {
        panic!()
    };
    assert_eq!(c.name.name, "MAX");

    let Item::Fn(f) = &file.items[4] else {
        panic!()
    };
    let ParamKind::Named { ty, .. } = &f.params[0].kind else {
        panic!()
    };
    assert!(matches!(ty.kind, TypeKind::Fn { ret: Some(_), .. }));
    let ParamKind::Named { ty, .. } = &f.params[1].kind else {
        panic!()
    };
    let TypeKind::Block { params, ret: None } = &ty.kind else {
        panic!()
    };
    assert_eq!(params[0].mode, PassMode::Mut);
    assert_eq!(f.ret.as_ref().unwrap().mode, RetMode::Borrow);
    assert_eq!(f.where_clause.len(), 1);
}

#[test]
fn spans_cover_the_source() {
    let src = "fn f() {\n    foo.bar(1)\n}";
    let s = stmts_in(src);
    assert_eq!(
        &src[s.span.start as usize..s.span.end as usize],
        "foo.bar(1)"
    );
}

fn stmts_in(src: &str) -> Stmt {
    let file = parse_ok(src);
    let Item::Fn(f) = file.items.into_iter().next().unwrap() else {
        panic!()
    };
    f.body.stmts.into_iter().next().unwrap()
}

#[test]
fn reports_several_errors_and_keeps_going() {
    let src =
        "fn a() {\n    var = 1\n    ok()\n    f(1,\n}\n\nfn b(x) {\n}\n\nfn c() {\n    good()\n}\n";
    let (file, diags) = parse(src);
    let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "expected an identifier, found `=`",
            "expected an expression, found `}`",
            "expected `:`, found `)`",
        ]
    );
    let names: Vec<_> = file
        .items
        .iter()
        .map(|i| match i {
            Item::Fn(f) => f.name.name.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(names, vec!["a", "c"]);
    let Item::Fn(a) = &file.items[0] else {
        panic!()
    };
    assert_eq!(a.body.stmts.len(), 1);
}

#[test]
fn statements_need_a_newline_between_them() {
    assert_eq!(
        errors("fn f() {\n    a b\n}"),
        vec!["expected a newline or `}` after the statement, found identifier `b`"]
    );
}

#[test]
fn trailing_block_is_not_call_syntax() {
    let s = stmts("if xs.is_empty() {\n    return\n}");
    let StmtKind::If(i) = &s[0].kind else {
        panic!()
    };
    assert_eq!(sx(&i.cond), "xs.is_empty()");
}

#[test]
fn not_an_item() {
    assert_eq!(
        errors("var x = 1"),
        vec!["expected an item (`fn`, `struct`, `enum`, `impl` or `const`), found `var`"]
    );
}

#[test]
fn broken_item_body_is_skipped() {
    let (file, diags) = parse("struct A {\n    x Int\n    y: Int\n}\n\nfn ok() {\n}\n");
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].message, "expected `:`, found identifier `Int`");
    assert_eq!(file.items.len(), 1);
}

#[test]
fn error_inside_interpolation_points_at_the_paren() {
    assert_eq!(
        errors("fn f() {\n    print(\"\\(1 +)\")\n}"),
        vec!["expected an expression, found `)`"]
    );
}

#[test]
fn error_inside_a_match_does_not_close_the_enclosing_block() {
    let src = "fn f() {\n    match x {\n        a => 1 1\n        b => 2\n    }\n    after()\n}\n";
    let (file, diags) = parse(src);
    let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, vec!["expected `}`, found integer literal"]);
    let Item::Fn(f) = &file.items[0] else {
        panic!()
    };
    assert_eq!(f.body.stmts.len(), 1);
}

#[test]
fn error_in_a_struct_field_does_not_resume_at_a_pub_field() {
    let (file, diags) = parse("struct A {\n    x Int\n    pub y: Int\n}\n\nfn ok() {\n}\n");
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(file.items.len(), 1);
}

#[test]
fn missing_closing_brace_recovers_at_the_next_unindented_item() {
    let (file, diags) = parse("fn a() {\n    g()\n\nfn b() {\n    h()\n}\n");
    let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, vec!["expected `}`, found `fn`"]);
    let names: Vec<_> = file
        .items
        .iter()
        .map(|i| match i {
            Item::Fn(f) => f.name.name.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(names, vec!["b"]);
}

#[test]
fn unindented_method_in_impl_is_still_a_method() {
    let file = parse_ok("impl A {\nfn m(self) {\n}\n}\n");
    let Item::Impl(i) = &file.items[0] else {
        panic!()
    };
    assert_eq!(i.methods.len(), 1);
}

#[test]
fn block_cut_off_by_end_of_file_is_one_error() {
    assert_eq!(
        errors("fn f() {\n    a"),
        vec!["expected `}`, found end of file"]
    );
}

#[test]
fn block_closed_by_an_outer_paren_terminates() {
    let diags = errors("fn f() {\n    g(|x| {\n        a b )\n}\n");
    assert!(!diags.is_empty());
}

#[test]
fn unterminated_interpolation_is_one_error() {
    assert_eq!(
        errors("fn f() {\n    print(\"\\(x\")\n}\n"),
        vec!["unterminated string interpolation"]
    );
}

#[test]
fn nested_string_that_is_complete_does_not_add_errors_to_unterminated_interpolation() {
    for src in ["\"\\(f(\"a\")\nb", "print(\"\\(f(\"x\")\")"] {
        let messages: Vec<_> = crate::lexer::lex(src)
            .1
            .into_iter()
            .map(|d| d.message)
            .collect();
        assert_eq!(
            messages,
            vec!["unterminated string interpolation"],
            "{src:?}"
        );
    }
}

#[test]
fn dropped_character_does_not_join_the_next_line() {
    let (_, diags) = parse("fn f() {\n    x = 1 + @\n    y = 2\n}\n");
    assert!(
        diags.iter().all(|d| d.span.start < 24),
        "errors leaked onto the next line: {diags:#?}"
    );
}

#[test]
fn float_literal_out_of_range_is_an_error() {
    assert_eq!(
        errors("fn f() {\n    x = 1.0e999\n}\n"),
        vec!["float literal is too large"]
    );
}

#[test]
fn trailing_backslash_is_one_lexer_error() {
    let messages: Vec<_> = crate::lexer::lex("\"\\")
        .1
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(messages, vec!["unterminated string literal"]);
}

#[test]
fn diagnostics_are_in_source_order() {
    let (_, diags) = parse("fn f() {\n    var = 1\n}\nfn g() {\n    x = @\n}\n");
    let starts: Vec<_> = diags.iter().map(|d| d.span.start).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert!(starts.len() >= 2);
    assert_eq!(starts, sorted);
}

#[test]
fn where_item_needs_bounds() {
    assert_eq!(
        errors("fn f() where T {\n}\n"),
        vec!["a `where` item needs bounds, as in `T: Ord`"]
    );
}

#[test]
fn deep_nesting_is_an_error_not_a_crash() {
    let n = 100_000;
    let cases = [
        format!(
            "fn f() {{\n    x = {}1{}\n}}\n",
            "(".repeat(n),
            ")".repeat(n)
        ),
        format!("fn f() {{\n    x = {}1\n}}\n", "-".repeat(n)),
        format!(
            "fn f() {{\n    if a {{\n    }}{}\n}}\n",
            " else if a {\n    }".repeat(n)
        ),
        format!("fn f() {{\n    {}{}\n}}\n", "{".repeat(n), "}".repeat(n)),
        format!("fn f(x: {}Int{}) {{\n}}\n", "A[".repeat(n), "]".repeat(n)),
        format!(
            "fn f() {{\n    match x {{\n        {}_{} => 1\n    }}\n}}\n",
            "a(".repeat(n),
            ")".repeat(n)
        ),
    ];
    for src in &cases {
        let messages = errors(src);
        assert!(
            messages.iter().any(|m| m == "nesting is too deep"),
            "{:?}: {messages:?}",
            &src[..40]
        );
    }
}

#[test]
fn impl_method_missing_its_brace_keeps_the_following_item() {
    let (file, diags) = parse("impl A {\n    fn m(self) {\n        x\n\nfn top() {\n}\n");
    assert_eq!(diags.len(), 1);
    assert!(
        file.items
            .iter()
            .any(|i| matches!(i, Item::Fn(f) if f.name.name == "top"))
    );
}

#[test]
fn else_on_its_own_line_explains_the_rule() {
    assert_eq!(
        errors("fn f() {\n    if a {\n    }\n    else {\n    }\n}\n"),
        vec!["`else` must be on the same line as the closing `}`"]
    );
}
