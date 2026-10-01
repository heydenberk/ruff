use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

use crate::Violation;
use crate::codes::Category;

use crate::checkers::ast::Checker;
use crate::rules::mccabe::cognitive_complexity::cognitive_complexity;
use crate::rules::mccabe::settings::{ComplexityMetric, Settings};

/// ## What it does
/// Checks for functions whose complexity exceeds a configurable threshold.
///
/// By default, this rule measures `McCabe` (cyclomatic) complexity: a measure of the
/// complexity of the control flow graph of the function. It is calculated by adding one to
/// the number of decision points in the function. A decision point is a place in the code
/// where the program has a choice of two or more paths to follow. In [preview], the rule can
/// measure cognitive complexity instead, as described below.
///
/// ## Why is this bad?
/// Functions with a high complexity are hard to understand and maintain.
///
/// ## Example
/// ```python
/// def normalize_status(status):
///     if status == "new":
///         return "queued"
///     if status == "queued":
///         return "running"
///     if status == "running":
///         return "done"
///     if status == "failed":
///         return "retry"
///     if status == "cancelled":
///         return "closed"
///     return "unknown"
/// ```
///
/// Use instead:
///
/// ```python
/// STATUS_TRANSITIONS = {
///     "new": "queued",
///     "queued": "running",
///     "running": "done",
///     "failed": "retry",
///     "cancelled": "closed",
/// }
///
///
/// def normalize_status(status):
///     return STATUS_TRANSITIONS.get(status, "unknown")
/// ```
///
/// Note that this example assumes a [`lint.mccabe.max-complexity`] of 5 or less to trigger a
/// diagnostic because the initial code has a cyclomatic complexity of 6.
///
/// ## Cognitive complexity
/// In [preview], setting [`lint.mccabe.metric`] to `"cognitive"` switches this rule to cognitive
/// complexity, as defined by
/// [SonarSource](https://www.sonarsource.com/docs/CognitiveComplexity.pdf).
/// Cyclomatic complexity counts decision points, so five `if` statements in a row score the
/// same as five nested inside one another. Cognitive complexity tries to measure how hard the
/// code is to read, so nesting costs more:
///
/// - `if`, `for`, `while`, `except`, `match`, and conditional expressions each add one, plus
///   one for every level of nesting they sit in. Their bodies (or, for a conditional
///   expression, its operands) are one level deeper; the bodies of `try`, `finally`, and
///   `with` are not.
/// - The `elif` and `else` branches of an `if`, and the `else` clauses of `for`, `while`, and
///   `try`, add one each, however deeply they're nested.
/// - Each run of the same boolean operator adds one: `a and b and c` adds one, while
///   `a and b or c` adds two.
/// - A direct recursive call, written as a bare call to the function's own name such as
///   `fib(n - 1)`, adds one. Calls through an attribute, like `self.visit(node)`, aren't
///   counted, and neither is a method calling its own name directly, since inside a method
///   that name never refers to the method itself.
/// - Nested functions and lambdas add a level of nesting, except for a function whose
///   enclosing function only defines and returns it, as in a decorator. A class body starts
///   again at zero nesting, and its methods add no nesting of their own.
/// - A `match` statement counts once, however many `case` arms it has. Comprehensions add
///   nothing on their own.
///
/// For example, `normalize_status` above has a cognitive complexity of 5: five `if`
/// statements, none of them nested.
///
/// Under this metric, only the outermost function is reported. The complexity of any function
/// defined inside it, including methods of a class defined inside it, counts toward the outer
/// function, so the same code isn't scored twice. As a result, a diagnostic on the outer
/// function may come from code in an inner function.
///
/// When [`lint.mccabe.max-complexity`] is unset, the threshold is 15 under this metric,
/// instead of 10.
///
/// ## Options
/// - `lint.mccabe.max-complexity`
/// - `lint.mccabe.metric`
///
/// ## References
/// - [SonarSource: Cognitive Complexity](https://www.sonarsource.com/docs/CognitiveComplexity.pdf)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.127", category = Category::Restriction)]
pub(crate) struct ComplexStructure {
    name: String,
    complexity: usize,
    max_complexity: usize,
    metric: ComplexityMetric,
}

impl Violation for ComplexStructure {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ComplexStructure {
            name,
            complexity,
            max_complexity,
            metric,
        } = self;
        match metric {
            ComplexityMetric::Cyclomatic => {
                format!("`{name}` is too complex ({complexity} > {max_complexity})")
            }
            ComplexityMetric::Cognitive => format!(
                "`{name}` is too complex (cognitive complexity {complexity} > {max_complexity})"
            ),
        }
    }
}

fn get_complexity_number(stmts: &[Stmt]) -> usize {
    let mut complexity = 0;
    for stmt in stmts {
        match stmt {
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                complexity += 1;
                complexity += get_complexity_number(body);
                for clause in elif_else_clauses {
                    if clause.test.is_some() {
                        complexity += 1;
                    }
                    complexity += get_complexity_number(&clause.body);
                }
            }
            Stmt::For(ast::StmtFor { body, orelse, .. }) => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            Stmt::With(ast::StmtWith { body, .. }) => {
                complexity += get_complexity_number(body);
            }
            Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    complexity += 1;
                    complexity += get_complexity_number(&case.body);
                }
                if let Some(last_case) = cases.last() {
                    // The complexity of an irrefutable pattern is similar to an `else` block of an `if` statement.
                    //
                    // For example:
                    // ```python
                    // match subject:
                    //     case 1: ...
                    //     case _: ...
                    //
                    // match subject:
                    //     case 1: ...
                    //     case foo: ...
                    // ```
                    if last_case.guard.is_none() && last_case.pattern.is_irrefutable() {
                        complexity -= 1;
                    }
                }
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                complexity += get_complexity_number(body);
                if !orelse.is_empty() {
                    complexity += 1;
                }
                complexity += get_complexity_number(orelse);
                complexity += get_complexity_number(finalbody);
                for handler in handlers {
                    complexity += 1;
                    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) = handler;
                    complexity += get_complexity_number(body);
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
                complexity += 1;
                complexity += get_complexity_number(body);
            }
            Stmt::ClassDef(ast::StmtClassDef { body, .. }) => {
                complexity += get_complexity_number(body);
            }
            _ => {}
        }
    }
    complexity
}

/// C901
pub(crate) fn function_is_too_complex(
    checker: &Checker,
    stmt: &Stmt,
    name: &str,
    body: &[Stmt],
    settings: &Settings,
) {
    let semantic = checker.semantic();
    // A nested function's cognitive complexity counts toward the outermost enclosing
    // function, so reporting it separately would count the same code twice. This also
    // covers methods of a class defined inside a function.
    if settings.metric == ComplexityMetric::Cognitive
        && semantic
            .current_scopes()
            .any(|scope| scope.kind.is_function())
    {
        return;
    }
    let complexity = match settings.metric {
        ComplexityMetric::Cyclomatic => get_complexity_number(body) + 1,
        ComplexityMetric::Cognitive => {
            // A bare name inside a method never refers to the method itself (the class
            // scope is skipped during name lookup), so a method can't call itself that way.
            let recursion_name = (!semantic.current_scope().kind.is_class()).then_some(name);
            cognitive_complexity(recursion_name, body)
        }
    };
    if complexity > settings.max_complexity {
        checker.report_diagnostic(
            ComplexStructure {
                name: name.to_string(),
                complexity,
                max_complexity: settings.max_complexity,
                metric: settings.metric,
            },
            stmt.identifier(),
        );
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_ast::Suite;
    use ruff_python_parser::parse_module;

    use super::get_complexity_number;

    fn parse_suite(source: &str) -> Result<Suite> {
        Ok(parse_module(source)?.into_suite())
    }

    #[test]
    fn trivial() -> Result<()> {
        let source = r"
def trivial():
    pass
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn expr_as_statement() -> Result<()> {
        let source = r"
def expr_as_statement():
    0xF00D
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn sequential() -> Result<()> {
        let source = r"
def sequential(n):
    k = n + 4
    s = k + n
    return s
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn if_elif_else_dead_path() -> Result<()> {
        let source = r#"
def if_elif_else_dead_path(n):
    if n > 3:
        return "bigger than three"
    elif n > 4:
        return "is never executed"
    else:
        return "smaller than or equal to three"
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn nested_ifs() -> Result<()> {
        let source = r#"
def nested_ifs():
    if n > 3:
        if n > 4:
            return "bigger than four"
        else:
            return "bigger than three"
    else:
        return "smaller than or equal to three"
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn for_loop() -> Result<()> {
        let source = r"
def for_loop():
    for i in range(10):
        print(i)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source = r"
def for_else(mylist):
    for i in mylist:
        print(i)
    else:
        print(None)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn recursive() -> Result<()> {
        let source = r"
def recursive(n):
    if n > 4:
        return f(n - 1)
    else:
        return n
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn nested_functions() -> Result<()> {
        let source = r"
def nested_functions():
    def a():
        def b():
            pass

        b()

    a()
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn try_else() -> Result<()> {
        let source = r"
def try_else():
    try:
        print(1)
    except TypeA:
        print(2)
    except TypeB:
        print(3)
    else:
        print(4)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 4);
        Ok(())
    }

    #[test]
    fn nested_try_finally() -> Result<()> {
        let source = r"
def nested_try_finally():
    try:
        try:
            print(1)
        finally:
            print(2)
    finally:
        print(3)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn foobar() -> Result<()> {
        let source = r"
async def foobar(a, b, c):
    await whatever(a, b, c)
    if await b:
        pass
    async with c:
        pass
    async for x in a:
        pass
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn annotated_assign() -> Result<()> {
        let source = r"
def annotated_assign():
    x: Any = None
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn class() -> Result<()> {
        let source = r"
class Class:
    def handle(self, *args, **options):
        if args:
            return

        class ServiceProvider:
            def a(self):
                pass

            def b(self, data):
                if not args:
                    pass

        class Logger:
            def c(*args, **kwargs):
                pass

            def error(self, message):
                pass

            def info(self, message):
                pass

            def exception(self):
                pass

        return ServiceProvider(Logger())
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 9);
        Ok(())
    }

    #[test]
    fn finally() -> Result<()> {
        let source = r"
def process_detect_lines():
    try:
        pass
    finally:
        pass
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn if_in_finally() -> Result<()> {
        let source = r#"
def process_detect_lines():
    try:
        pass
    finally:
        if res:
            errors.append(f"Non-zero exit code {res}")
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn with() -> Result<()> {
        let source = r"
def with_lock():
    with lock:
        if foo:
            print('bar')
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn simple_match_case() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('foo')
        case _:
            print('bar')
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn multiple_match_case() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('foo')
        case 2:
            print('bar')
        case _:
            print('baz')
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn named_catch_all_match_case() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('hello')
        case x:
            print(x)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn match_case_catch_all_with_sequence() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('hello')
        case 5 | _:
            print(x)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }
}
