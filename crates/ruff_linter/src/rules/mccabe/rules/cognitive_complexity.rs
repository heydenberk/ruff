use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, Stmt};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions with a high *cognitive* complexity.
///
/// ## Why is this bad?
/// Cognitive complexity measures how hard a function is to *read and
/// understand*, as opposed to McCabe cyclomatic complexity (see `C901`),
/// which counts the number of independent paths through the control
/// flow graph. Cognitive complexity adds cost for nesting, rewards
/// sequential structures over deeply nested ones, and counts each
/// sequence of boolean operators only once.
///
/// This rule implements the metric defined by [Sonar][sonar] and matches
/// the scoring of [complexipy][complexipy].
///
/// Functions with a high cognitive complexity are hard to understand
/// and maintain.
///
/// ## Example
/// ```python
/// def compute(items, flag):
///     total = 0
///     for item in items:              # +1
///         if item.active:             # +2 (nested)
///             if flag and item.age:   # +3 (nested) + 1 (bool-op run)
///                 total += item.value
///             else:                   # +1 (else)
///                 total += 1
///     return total
/// ```
///
/// Use instead:
/// ```python
/// def compute(items, flag):
///     return sum(value_of(item, flag) for item in items)
///
/// def value_of(item, flag):
///     if not item.active:
///         return 0
///     if flag and item.age:
///         return item.value
///     return 1
/// ```
///
/// ## Options
/// - `lint.mccabe.max-cognitive-complexity`
///
/// [sonar]: https://www.sonarsource.com/resources/cognitive-complexity/
/// [complexipy]: https://github.com/rohaquinlop/complexipy
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.12")]
pub(crate) struct HighCognitiveComplexity {
    name: String,
    complexity: usize,
    max_complexity: usize,
}

impl Violation for HighCognitiveComplexity {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HighCognitiveComplexity {
            name,
            complexity,
            max_complexity,
        } = self;
        format!("`{name}` is too cognitively complex ({complexity} > {max_complexity})")
    }
}

/// C902
pub(crate) fn function_is_too_cognitively_complex(
    checker: &Checker,
    stmt: &Stmt,
    name: &str,
    body: &[Stmt],
    max_complexity: usize,
) {
    let complexity = body_complexity(body, 0);
    if complexity > max_complexity {
        checker.report_diagnostic(
            HighCognitiveComplexity {
                name: name.to_string(),
                complexity,
                max_complexity,
            },
            stmt.identifier(),
        );
    }
}

/// Cognitive complexity of a function body.
///
/// Port of [complexipy]'s `statement_cognitive_complexity_shared` and
/// `count_bool_ops`. Differs from complexipy in two ways that fit ruff's
/// per-function rule model:
/// - Nested `def`/`class` statements contribute nothing here; the checker
///   visits them separately and they get their own score.
/// - Recursive calls are not counted (same as complexipy).
///
/// [complexipy]: https://github.com/rohaquinlop/complexipy
fn body_complexity(body: &[Stmt], nesting: usize) -> usize {
    body.iter().map(|s| statement_complexity(s, nesting)).sum()
}

fn statement_complexity(stmt: &Stmt, nesting: usize) -> usize {
    match stmt {
        // Nested `def`/`class` are scored independently by the checker.
        Stmt::FunctionDef(_) | Stmt::ClassDef(_) => 0,
        Stmt::Assign(ast::StmtAssign { value, .. })
        | Stmt::AugAssign(ast::StmtAugAssign { value, .. }) => bool_ops_complexity(value, nesting),
        Stmt::AnnAssign(ast::StmtAnnAssign { value, .. }) => value
            .as_deref()
            .map_or(0, |v| bool_ops_complexity(v, nesting)),
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            1 + nesting + body_complexity(body, nesting + 1) + body_complexity(orelse, nesting + 1)
        }
        Stmt::While(ast::StmtWhile {
            test, body, orelse, ..
        }) => {
            1 + nesting
                + bool_ops_complexity(test, nesting)
                + body_complexity(body, nesting + 1)
                + body_complexity(orelse, nesting + 1)
        }
        Stmt::If(ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            ..
        }) => {
            let mut c = 1 + nesting + bool_ops_complexity(test, nesting);
            c += body_complexity(body, nesting + 1);
            for clause in elif_else_clauses {
                c += 1;
                if let Some(test) = &clause.test {
                    c += bool_ops_complexity(test, nesting);
                }
                c += body_complexity(&clause.body, nesting + 1);
            }
            c
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            let mut c = body_complexity(body, nesting + 1);
            for handler in handlers {
                c += 1;
                let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) =
                    handler;
                c += body_complexity(body, nesting + 1);
            }
            c + body_complexity(orelse, nesting + 1) + body_complexity(finalbody, nesting + 1)
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .map(|case| body_complexity(&case.body, nesting + 1))
            .sum(),
        Stmt::Return(ast::StmtReturn { value, .. }) => value
            .as_deref()
            .map_or(0, |v| bool_ops_complexity(v, nesting)),
        Stmt::Raise(ast::StmtRaise { exc, cause, .. }) => {
            exc.as_deref()
                .map_or(0, |v| bool_ops_complexity(v, nesting))
                + cause
                    .as_deref()
                    .map_or(0, |v| bool_ops_complexity(v, nesting))
        }
        Stmt::Assert(ast::StmtAssert { test, msg, .. }) => {
            bool_ops_complexity(test, nesting)
                + msg
                    .as_deref()
                    .map_or(0, |v| bool_ops_complexity(v, nesting))
        }
        Stmt::With(ast::StmtWith { items, body, .. }) => {
            items
                .iter()
                .map(|item| bool_ops_complexity(&item.context_expr, nesting))
                .sum::<usize>()
                + body_complexity(body, nesting + 1)
        }
        _ => 0,
    }
}

fn bool_ops_complexity(expr: &Expr, nesting: usize) -> usize {
    match expr {
        Expr::BoolOp(b) => {
            let mut c = 1;
            for value in &b.values {
                c += operator_transitions(value, expr);
            }
            c
        }
        Expr::UnaryOp(u) => operator_transitions(&u.operand, expr),
        Expr::Compare(c) => {
            let mut total = bool_ops_complexity(&c.left, nesting);
            for cmp in &c.comparators {
                total += bool_ops_complexity(cmp, nesting);
            }
            total
        }
        Expr::If(i) => {
            1 + nesting
                + bool_ops_complexity(&i.test, nesting)
                + bool_ops_complexity(&i.body, nesting)
                + bool_ops_complexity(&i.orelse, nesting)
        }
        Expr::Call(c) => c
            .arguments
            .args
            .iter()
            .map(|a| bool_ops_complexity(a, nesting))
            .sum(),
        Expr::Tuple(t) => t.elts.iter().map(|e| bool_ops_complexity(e, nesting)).sum(),
        Expr::List(l) => l.elts.iter().map(|e| bool_ops_complexity(e, nesting)).sum(),
        Expr::Set(s) => s.elts.iter().map(|e| bool_ops_complexity(e, nesting)).sum(),
        Expr::Dict(d) => d
            .items
            .iter()
            .map(|i| bool_ops_complexity(&i.value, nesting))
            .sum(),
        Expr::FString(f) => f
            .value
            .elements()
            .filter_map(|e| e.as_interpolation())
            .map(|i| bool_ops_complexity(&i.expression, nesting))
            .sum(),
        Expr::ListComp(l) => comprehension_complexity(&l.generators, &l.elt, nesting),
        Expr::SetComp(s) => comprehension_complexity(&s.generators, &s.elt, nesting),
        Expr::Generator(g) => comprehension_complexity(&g.generators, &g.elt, nesting),
        Expr::DictComp(d) => {
            comprehension_complexity(&d.generators, &d.key, nesting)
                + bool_ops_complexity(&d.value, nesting + 1)
        }
        _ => 0,
    }
}

fn comprehension_complexity(
    generators: &[ast::Comprehension],
    elt: &Expr,
    nesting: usize,
) -> usize {
    let mut c = 1 + nesting;
    for (i, clause) in generators.iter().enumerate() {
        if i > 0 {
            c += 1;
        }
        for if_expr in &clause.ifs {
            c += 1 + bool_ops_complexity(if_expr, nesting + 1);
        }
        c += bool_ops_complexity(&clause.iter, nesting + 1);
    }
    c + bool_ops_complexity(elt, nesting + 1)
}

/// Counts additional increments from operator transitions in chained boolean
/// expressions (matches complexipy's `count_different_childs_type`).
fn operator_transitions(expr: &Expr, prev: &Expr) -> usize {
    match (expr, prev) {
        (Expr::BoolOp(cur), Expr::BoolOp(prev_bool)) => {
            let mut c = usize::from(cur.op != prev_bool.op);
            for value in &prev_bool.values {
                c += operator_transitions(value, expr);
            }
            c
        }
        (Expr::BoolOp(_), Expr::UnaryOp(p)) => 1 + operator_transitions(&p.operand, expr),
        (Expr::UnaryOp(_), Expr::BoolOp(prev_bool)) => prev_bool
            .values
            .iter()
            .map(|v| operator_transitions(v, expr))
            .sum(),
        (Expr::UnaryOp(_), Expr::UnaryOp(p)) => operator_transitions(&p.operand, expr),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_ast::Suite;
    use ruff_python_parser::parse_module;

    use super::body_complexity;

    fn parse_suite(source: &str) -> Result<Suite> {
        Ok(parse_module(source)?.into_suite())
    }

    fn score(source: &str) -> Result<usize> {
        let stmts = parse_suite(source)?;
        // Tests below each wrap the body in `def f(): ...`, so grab that function's body.
        let def = stmts.first().expect("source must define at least one fn");
        if let ruff_python_ast::Stmt::FunctionDef(f) = def {
            Ok(body_complexity(&f.body, 0))
        } else {
            panic!("expected first statement to be a function def")
        }
    }

    #[test]
    fn trivial() -> Result<()> {
        assert_eq!(score("def f():\n    return 0\n")?, 0);
        Ok(())
    }

    #[test]
    fn single_if() -> Result<()> {
        assert_eq!(
            score("def f(x):\n    if x:\n        return 1\n    return 0\n")?,
            1
        );
        Ok(())
    }

    #[test]
    fn nested_ifs_cost_nesting() -> Result<()> {
        // if(+1) > if(+2) > if(+3) = 6
        let src = "\
def f(x, y, z):
    if x:
        if y:
            if z:
                return 1
    return 0
";
        assert_eq!(score(src)?, 6);
        Ok(())
    }

    #[test]
    fn bool_chain_counts_once() -> Result<()> {
        assert_eq!(
            score("def f(a, b, c, d):\n    return a and b and c and d\n")?,
            1
        );
        Ok(())
    }

    #[test]
    fn bool_mixed_counts_each_run() -> Result<()> {
        // (a and b) or (c and d): outer or (+1), two inner ands each (+1) = 3
        assert_eq!(
            score("def f(a, b, c, d):\n    return a and b or c and d\n")?,
            3
        );
        Ok(())
    }

    #[test]
    fn ternary_chain_flat() -> Result<()> {
        // Three ternaries at nesting 0, each +1+0 = 1, total 3.
        let src = "def f(a, b, c, d):\n    return a if a else (b if b else (c if c else d))\n";
        assert_eq!(score(src)?, 3);
        Ok(())
    }

    #[test]
    fn ternary_inside_if_uses_nesting() -> Result<()> {
        // if(+1) at nest 0, body at nest 1. Return's ternary: +1 + 1 = 2. Total 3.
        let src = "def f(x, a, b):\n    if x:\n        return a if a else b\n    return 0\n";
        assert_eq!(score(src)?, 3);
        Ok(())
    }

    #[test]
    fn dict_literal_with_ternaries() -> Result<()> {
        // Two ternaries inside a return'd dict literal at nesting 0: 1+1 = 2.
        let src = "\
def f(a, b):
    return {\"x\": a if a else None, \"y\": b if b else None}
";
        assert_eq!(score(src)?, 2);
        Ok(())
    }

    #[test]
    fn try_except_else_finally() -> Result<()> {
        // try body (0) + except (+1) + body (0) + else (0) + finally (0) = 1
        let src = "\
def f():
    try:
        print(1)
    except Exception:
        print(2)
    else:
        print(3)
    finally:
        print(4)
";
        assert_eq!(score(src)?, 1);
        Ok(())
    }

    #[test]
    fn match_only_cases_count() -> Result<()> {
        // Match itself adds nothing; bodies at nesting+1 add only their own costs.
        let src = "\
def f(x):
    match x:
        case 1:
            if x:
                return 1
        case _:
            return 0
";
        // body of case 1 at nest 1: if +1+1 = 2. Case 2 body: return, 0. Total 2.
        assert_eq!(score(src)?, 2);
        Ok(())
    }

    #[test]
    fn list_comp_with_filter() -> Result<()> {
        // 1 (comp) + 1 (if-filter) + 0 = 2.
        assert_eq!(score("def f(xs):\n    return [x for x in xs if x]\n")?, 2);
        Ok(())
    }

    #[test]
    fn nested_def_contributes_nothing_to_parent() -> Result<()> {
        let src = "\
def outer(x):
    def inner(y):
        if y:
            return y
    return inner
";
        // Outer body: nested def (0) + return (0) = 0.
        assert_eq!(score(src)?, 0);
        Ok(())
    }

    #[test]
    fn with_body_nests() -> Result<()> {
        // with body at nest 1: if +1+1 = 2.
        let src = "\
def f(lock, x):
    with lock:
        if x:
            print(1)
";
        assert_eq!(score(src)?, 2);
        Ok(())
    }
}
