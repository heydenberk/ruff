//! Cognitive complexity, as defined by SonarSource.
//!
//! See <https://www.sonarsource.com/docs/CognitiveComplexity.pdf>. Where the whitepaper doesn't
//! cover a Python construct, this follows Sonar's own Python analyzer.

use std::mem;

use ruff_python_ast::helpers::body_without_leading_docstring;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, BoolOp, ExceptHandler, Expr, Stmt};
use ruff_text_size::Ranged;

/// Returns the cognitive complexity of the function with the given `body`.
///
/// `function_name` is the name that a bare call must use to count as direct recursion, or `None`
/// when scoring a method, where a bare name never refers to the method itself.
///
/// Functions nested inside `body` contribute to the result.
pub(super) fn cognitive_complexity(function_name: Option<&str>, body: &[Stmt]) -> usize {
    let mut visitor = CognitiveComplexityVisitor {
        function_name,
        function_body: body,
        in_class_body: false,
        nesting: 0,
        complexity: 0,
    };
    visitor.visit_body(body);
    visitor.complexity
}

struct CognitiveComplexityVisitor<'a> {
    /// The name a bare call must use to count as direct recursion, or `None` inside a method,
    /// where a bare name never refers to the method itself.
    function_name: Option<&'a str>,
    /// The body of the innermost function being visited, used to detect wrapper functions.
    function_body: &'a [Stmt],
    /// Whether the statements being visited belong to a class body rather than a function body.
    in_class_body: bool,
    /// The current nesting level, added to each nesting-sensitive increment.
    nesting: usize,
    /// The running total.
    complexity: usize,
}

impl<'a> CognitiveComplexityVisitor<'a> {
    /// Adds one for a control-flow structure, plus one for each level of nesting around it.
    fn increment_with_nesting(&mut self) {
        self.complexity += 1 + self.nesting;
    }

    /// Visits `body` one nesting level deeper than the current one.
    fn visit_nested_body(&mut self, body: &'a [Stmt]) {
        self.nesting += 1;
        self.visit_body(body);
        self.nesting -= 1;
    }

    /// Visits the `else` clause of a `for`, `while`, or `try` statement.
    fn visit_else_clause(&mut self, orelse: &'a [Stmt]) {
        if orelse.is_empty() {
            return;
        }
        // Like `else` on an `if`, this is a flat increment: the reader already paid for the
        // nesting at the statement that owns the clause.
        self.complexity += 1;
        self.visit_nested_body(orelse);
    }

    /// Visits the operands of `bool_op`, looking through directly nested boolean operations.
    fn visit_bool_operands(&mut self, bool_op: &'a ast::ExprBoolOp) {
        for value in &bool_op.values {
            // Directly nested boolean operations were already scored as part of the outer
            // sequence (see `bool_operator_sequences`), so only visit their operands.
            if let Expr::BoolOp(inner) = value {
                self.visit_bool_operands(inner);
            } else {
                self.visit_expr(value);
            }
        }
    }

    fn visit_function_def(&mut self, function_def: &'a ast::StmtFunctionDef) {
        // Decorators, parameter defaults, type parameters, and annotations belong to the enclosing
        // function, so they're scored at its nesting level.
        for decorator in &function_def.decorator_list {
            self.visit_decorator(decorator);
        }
        if let Some(type_params) = &function_def.type_params {
            self.visit_type_params(type_params);
        }
        self.visit_parameters(&function_def.parameters);
        if let Some(returns) = &function_def.returns {
            self.visit_annotation(returns);
        }

        let added_nesting =
            usize::from(!self.in_class_body && !self.is_wrapper_function(function_def));
        let function_name = mem::replace(
            &mut self.function_name,
            (!self.in_class_body).then_some(function_def.name.as_str()),
        );
        let function_body = mem::replace(&mut self.function_body, function_def.body.as_slice());
        let in_class_body = mem::replace(&mut self.in_class_body, false);
        self.nesting += added_nesting;

        self.visit_body(&function_def.body);

        self.nesting -= added_nesting;
        self.function_name = function_name;
        self.function_body = function_body;
        self.in_class_body = in_class_body;
    }

    fn visit_class_def(&mut self, class_def: &'a ast::StmtClassDef) {
        for decorator in &class_def.decorator_list {
            self.visit_decorator(decorator);
        }
        if let Some(type_params) = &class_def.type_params {
            self.visit_type_params(type_params);
        }
        if let Some(arguments) = &class_def.arguments {
            self.visit_arguments(arguments);
        }

        // A class body is a fresh context: nesting restarts at zero, and methods defined in it
        // don't add a level of nesting for their bodies.
        let nesting = mem::replace(&mut self.nesting, 0);
        let in_class_body = mem::replace(&mut self.in_class_body, true);

        self.visit_body(&class_def.body);

        self.nesting = nesting;
        self.in_class_body = in_class_body;
    }

    /// Returns `true` if `function_def` is a wrapper function: the enclosing function's body is,
    /// apart from an optional leading docstring, exactly this definition followed by
    /// `return <its name>`, as in a decorator:
    ///
    /// ```python
    /// def decorator(func):
    ///     """Optional docstring."""
    ///     def wrapper(*args):
    ///         ...
    ///     return wrapper
    /// ```
    ///
    /// The outer function only exists to hold `wrapper`, so `wrapper` isn't any harder to read
    /// than a top-level function and shouldn't pay for an extra level of nesting.
    fn is_wrapper_function(&self, function_def: &ast::StmtFunctionDef) -> bool {
        matches!(
            body_without_leading_docstring(self.function_body),
            [definition, Stmt::Return(ast::StmtReturn { value: Some(value), .. })]
                if definition.range() == function_def.range()
                    && value.as_name_expr().is_some_and(|name| name.id == function_def.name.id)
        )
    }
}

impl<'a> Visitor<'a> for CognitiveComplexityVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => {
                self.increment_with_nesting();
                self.visit_expr(test);
                self.visit_nested_body(body);
                for clause in elif_else_clauses {
                    // `elif` and `else` are flat increments: the nesting was paid for at the `if`.
                    self.complexity += 1;
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                    }
                    self.visit_nested_body(&clause.body);
                }
            }
            Stmt::For(ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                ..
            }) => {
                self.increment_with_nesting();
                self.visit_expr(target);
                self.visit_expr(iter);
                self.visit_nested_body(body);
                self.visit_else_clause(orelse);
            }
            Stmt::While(ast::StmtWhile {
                test, body, orelse, ..
            }) => {
                self.increment_with_nesting();
                self.visit_expr(test);
                self.visit_nested_body(body);
                self.visit_else_clause(orelse);
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                // `try` and `finally` don't branch, so neither is scored or nested.
                self.visit_body(body);
                for handler in handlers {
                    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        type_,
                        body,
                        ..
                    }) = handler;
                    self.increment_with_nesting();
                    if let Some(type_) = type_ {
                        self.visit_expr(type_);
                    }
                    self.visit_nested_body(body);
                }
                self.visit_else_clause(orelse);
                self.visit_body(finalbody);
            }
            Stmt::Match(ast::StmtMatch { subject, cases, .. }) => {
                // Like `switch` in the whitepaper, a `match` is scored once, not per `case`.
                self.increment_with_nesting();
                self.visit_expr(subject);
                for case in cases {
                    self.visit_pattern(&case.pattern);
                    if let Some(guard) = &case.guard {
                        // The guard is deliberately visited before `visit_nested_body` nests
                        // the `case` body, so it's scored at the nesting level of the `match`
                        // statement, like an `elif` condition relative to its `if`.
                        self.visit_expr(guard);
                    }
                    self.visit_nested_body(&case.body);
                }
            }
            Stmt::FunctionDef(function_def) => self.visit_function_def(function_def),
            Stmt::ClassDef(class_def) => self.visit_class_def(class_def),
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::BoolOp(bool_op) => {
                self.complexity += bool_operator_sequences(bool_op);
                self.visit_bool_operands(bool_op);
            }
            Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                self.increment_with_nesting();
                // Unlike an `if` statement, whose test is scored before its body is nested, the
                // whole conditional expression is nested, including the condition. This matches
                // Sonar's Python analyzer.
                self.nesting += 1;
                self.visit_expr(test);
                self.visit_expr(body);
                self.visit_expr(orelse);
                self.nesting -= 1;
            }
            Expr::Lambda(ast::ExprLambda {
                parameters, body, ..
            }) => {
                if let Some(parameters) = parameters {
                    self.visit_parameters(parameters);
                }
                self.nesting += 1;
                self.visit_expr(body);
                self.nesting -= 1;
            }
            Expr::Call(ast::ExprCall { func, .. })
                if func
                    .as_name_expr()
                    .is_some_and(|name| Some(name.id.as_str()) == self.function_name) =>
            {
                // Direct recursion. Only bare calls are recognized: `self.method(...)` is not.
                self.complexity += 1;
                walk_expr(self, expr);
            }
            _ => walk_expr(self, expr),
        }
    }
}

/// Counts the sequences of like boolean operators in `bool_op`, including directly nested
/// boolean operations: `a and b and c` has one sequence, `a and b or c` has two, and
/// `a or b and c or d` has three.
fn bool_operator_sequences(bool_op: &ast::ExprBoolOp) -> usize {
    count_bool_operator_sequences(bool_op, &mut None)
}

/// Walks the operators of `bool_op` in source order and returns how many differ from the
/// operator that preceded them. `previous` is that preceding operator, if any.
fn count_bool_operator_sequences(
    bool_op: &ast::ExprBoolOp,
    previous: &mut Option<BoolOp>,
) -> usize {
    let mut sequences = 0;
    for (index, value) in bool_op.values.iter().enumerate() {
        if index > 0 {
            if *previous != Some(bool_op.op) {
                sequences += 1;
            }
            *previous = Some(bool_op.op);
        }
        if let Expr::BoolOp(inner) = value {
            sequences += count_bool_operator_sequences(inner, previous);
        }
    }
    sequences
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};
    use ruff_python_ast::Stmt;
    use ruff_python_parser::parse_module;

    use super::cognitive_complexity;

    /// Parses `source`, which must start with a function definition, and scores that function.
    fn score(source: &str) -> Result<usize> {
        score_as(source, false)
    }

    /// Scores the function, treating it as a method if `is_method` (where a bare call to its own
    /// name is not recursion).
    fn score_as(source: &str, is_method: bool) -> Result<usize> {
        let suite = parse_module(source)?.into_suite();
        let Some(Stmt::FunctionDef(function_def)) = suite.first() else {
            bail!("expected the source to start with a function definition");
        };
        Ok(cognitive_complexity(
            (!is_method).then_some(function_def.name.as_str()),
            &function_def.body,
        ))
    }

    #[test]
    fn trivial() -> Result<()> {
        let source = r"
def trivial():
    pass
";
        assert_eq!(score(source)?, 0);
        Ok(())
    }

    #[test]
    fn if_elif_else() -> Result<()> {
        let source = r#"
def sign(n):
    if n > 0:  # +1
        return "positive"
    elif n < 0:  # +1
        return "negative"
    else:  # +1
        return "zero"
"#;
        assert_eq!(score(source)?, 3);
        Ok(())
    }

    #[test]
    fn nesting_increments() -> Result<()> {
        let source = r"
def nested(items):
    for item in items:  # +1
        if item:  # +2 (nesting = 1)
            while item.busy:  # +3 (nesting = 2)
                pass
";
        assert_eq!(score(source)?, 6);
        Ok(())
    }

    /// The whitepaper's `sumOfPrimes`, with the labeled `continue` expressed as `for`-`else`.
    #[test]
    fn sum_of_primes() -> Result<()> {
        let source = r"
def sum_of_primes(limit):
    total = 0
    for i in range(2, limit + 1):  # +1
        for j in range(2, i):  # +2 (nesting = 1)
            if i % j == 0:  # +3 (nesting = 2)
                break
        else:  # +1
            total += i
    return total
";
        assert_eq!(score(source)?, 7);
        Ok(())
    }

    /// The whitepaper's `getWords`: a `switch` counts once, however many cases it has.
    #[test]
    fn get_words() -> Result<()> {
        let source = r#"
def get_words(number):
    match number:  # +1
        case 1:
            return "one"
        case 2:
            return "a couple"
        case _:
            return "lots"
"#;
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn match_is_nested() -> Result<()> {
        let source = r"
def dispatch(events):
    for event in events:  # +1
        match event:  # +2 (nesting = 1)
            case 1:
                if event:  # +3 (nesting = 2)
                    pass
";
        assert_eq!(score(source)?, 6);
        Ok(())
    }

    #[test]
    fn match_guard_is_scored_at_the_match_level() -> Result<()> {
        let source = r"
def f(x, a, b):
    match x:  # +1
        case 1 if (a if x else b):  # +1 (nesting = 0): the guard isn't nested in the `case`
            pass
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn while_else() -> Result<()> {
        let source = r"
def search(n):
    while n:  # +1
        n -= 1
    else:  # +1
        pass
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn try_does_not_nest() -> Result<()> {
        let source = r"
def load(path):
    try:
        if path:  # +1 (`try` adds no nesting)
            pass
    except OSError:  # +1
        pass
    except ValueError:  # +1
        pass
    else:  # +1
        pass
    finally:
        if path:  # +1 (`finally` adds no nesting)
            pass
";
        assert_eq!(score(source)?, 5);
        Ok(())
    }

    #[test]
    fn elif_and_else_are_flat_in_a_loop() -> Result<()> {
        let source = r"
def f(xs, a, b):
    for x in xs:  # +1
        if a:  # +2 (nesting = 1)
            pass
        elif b:  # +1
            pass
        else:  # +1
            pass
";
        assert_eq!(score(source)?, 5);
        Ok(())
    }

    #[test]
    fn try_else_is_flat_in_a_loop() -> Result<()> {
        let source = r"
def f(xs):
    for x in xs:  # +1
        try:
            pass
        except OSError:  # +2 (nesting = 1)
            pass
        else:  # +1
            pass
";
        assert_eq!(score(source)?, 4);
        Ok(())
    }

    #[test]
    fn except_is_nested() -> Result<()> {
        let source = r"
def load_all(paths):
    for path in paths:  # +1
        try:
            pass
        except OSError:  # +2 (nesting = 1)
            pass
";
        assert_eq!(score(source)?, 3);
        Ok(())
    }

    #[test]
    fn except_star_is_nested() -> Result<()> {
        let source = r"
def load_all(paths):
    for path in paths:  # +1
        try:
            pass
        except* ValueError:  # +2 (nesting = 1)
            pass
";
        assert_eq!(score(source)?, 3);
        Ok(())
    }

    #[test]
    fn with_does_not_nest() -> Result<()> {
        let source = r"
def read(path):
    with open(path) as f:
        if f:  # +1
            pass
";
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn async_for_and_with() -> Result<()> {
        let source = r"
async def f(items, lock):
    async with lock:
        async for item in items:  # +1 (`async with` adds no nesting)
            if item:  # +2 (nesting = 1)
                pass
";
        assert_eq!(score(source)?, 3);
        Ok(())
    }

    #[test]
    fn boolean_operator_sequences() -> Result<()> {
        let same = r"
def f(a, b, c):
    return a and b and c  # +1
";
        let mixed = r"
def f(a, b, c):
    return a and b or c  # +2
";
        let alternating = r"
def f(a, b, c, d):
    return a or b and c or d  # +3
";
        let negated = r"
def f(a, b, c):
    return not (a and b) and c  # +2 (the `not` separates two `and` sequences)
";
        // Parenthesized groups of the same operator form one sequence.
        let parenthesized = r"
def f(a, b, c):
    return (a and b) and c  # +1
";
        assert_eq!(score(same)?, 1);
        assert_eq!(score(mixed)?, 2);
        assert_eq!(score(alternating)?, 3);
        assert_eq!(score(negated)?, 2);
        assert_eq!(score(parenthesized)?, 1);
        Ok(())
    }

    #[test]
    fn boolean_operators_in_conditions() -> Result<()> {
        let source = r"
def f(a, b):
    if a and b:  # +1 for `if`, +1 for `and`
        pass
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn ternary() -> Result<()> {
        let flat = r"
def f(a):
    return 1 if a else 2  # +1
";
        let nested_in_if = r"
def f(a, b):
    if a:  # +1
        return 1 if b else 2  # +2 (nesting = 1)
";
        let nested_ternary = r"
def f(a, b):
    return 1 if a else (2 if b else 3)  # +1, +2 (nesting = 1)
";
        assert_eq!(score(flat)?, 1);
        assert_eq!(score(nested_in_if)?, 3);
        assert_eq!(score(nested_ternary)?, 3);
        Ok(())
    }

    #[test]
    fn comprehensions_add_nothing() -> Result<()> {
        let plain = r"
def f(xs):
    return [x for x in xs for y in x if y]
";
        let with_bool_op = r"
def f(xs):
    return [x for x in xs if x.a and x.b]  # +1 for `and`
";
        assert_eq!(score(plain)?, 0);
        assert_eq!(score(with_bool_op)?, 1);
        Ok(())
    }

    #[test]
    fn recursion() -> Result<()> {
        let source = r"
def factorial(n):
    return 1 if n <= 1 else n * factorial(n - 1)  # +1 ternary, +1 recursion
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn attribute_call_is_not_recursion() -> Result<()> {
        let source = r"
def visit(self, node):
    for child in node.children:  # +1
        self.visit(child)  # only bare-name calls count as recursion
";
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn bare_call_in_nested_method_is_not_recursion() -> Result<()> {
        let source = r"
def factory():
    class Impl:
        def open(self, path):
            return open(path)  # the builtin, not `Impl.open`
    return Impl
";
        assert_eq!(score(source)?, 0);
        Ok(())
    }

    #[test]
    fn call_to_outer_function_from_nested_function_is_not_recursion() -> Result<()> {
        let source = r"
def outer(n):
    def inner(m):
        return outer(m)  # not recursion: only calls to `inner` itself count here
    return inner
";
        assert_eq!(score(source)?, 0);
        Ok(())
    }

    #[test]
    fn bare_call_in_method_is_not_recursion() -> Result<()> {
        let source = r"
def open(self, path):
    return open(path)
";
        assert_eq!(score_as(source, true)?, 0);
        Ok(())
    }

    #[test]
    fn type_parameters_are_scored_in_enclosing_function() -> Result<()> {
        let source = r"
def outer(flag):
    def inner[T = (int if flag else str)]():  # +1
        pass
    return inner
";
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn nested_function_adds_nesting() -> Result<()> {
        let source = r"
def outer(items):
    def inner(item):
        if item:  # +2 (nesting = 1)
            return 1
        return 0

    return [inner(item) for item in items]
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn wrapper_function_adds_no_nesting() -> Result<()> {
        let source = r#"
def decorator(func):
    """Wrap `func`."""

    def wrapper(value):
        if value:  # +1
            return func(value)
        return None

    return wrapper
"#;
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn decorated_wrapper_without_docstring() -> Result<()> {
        let source = r"
def decorator(func):
    @functools.wraps(func)
    def wrapper(value):
        if value:  # +1
            return func(value)
        return None
    return wrapper
";
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn returning_another_name_is_not_a_wrapper() -> Result<()> {
        let source = r"
def outer(func):
    def inner():
        if func:  # +2 (nesting = 1): `outer` returns `func`, not `inner`
            pass
    return func
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn nested_function_without_return_is_not_a_wrapper() -> Result<()> {
        let source = r"
def outer(x):
    def inner():
        if x:  # +2 (nesting = 1)
            pass
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn non_wrapper_functions_add_nesting() -> Result<()> {
        let parent_does_more = r"
def decorator(func):
    calls = 0
    def wrapper(value):
        if value:  # +2 (nesting = 1): `decorator` does more than return `wrapper`
            return func(value)
        return None
    return wrapper
";
        let conditional_def = r"
def decorator(func):
    if func:  # +1
        def wrapper():
            if func:  # +3 (nesting = 2)
                pass
        return wrapper
    return func
";
        assert_eq!(score(parent_does_more)?, 2);
        assert_eq!(score(conditional_def)?, 4);
        Ok(())
    }

    #[test]
    fn decorator_factory_with_two_wrappers() -> Result<()> {
        let source = r"
def retry(times):
    def decorator(func):
        def wrapper(*args):
            if times:  # +1 (neither inner function adds nesting)
                return func(*args)
            return None
        return wrapper
    return decorator
";
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn decorators_and_defaults_are_scored_in_the_enclosing_function() -> Result<()> {
        let source = r"
def outer(flag):
    calls = 0  # makes `inner` a non-wrapper, so its body is nested and the two levels differ
    @register(1 if flag else 2)  # +1 (nesting = 0)
    def inner(value=(3 if flag else 4)):  # +1 (nesting = 0)
        pass
    return inner
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn lambda_adds_nesting() -> Result<()> {
        let source = r"
def f(a, b):
    return lambda x: a if x else b  # +2 (nesting = 1)
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }

    #[test]
    fn lambda_defaults_are_scored_in_the_enclosing_function() -> Result<()> {
        let source = r"
def f(flag):
    return lambda x=(1 if flag else 2): x  # +1 (nesting = 0)
";
        assert_eq!(score(source)?, 1);
        Ok(())
    }

    #[test]
    fn class_resets_nesting() -> Result<()> {
        let source = r"
def factory(flag):
    if flag:  # +1
        class Impl:
            def run(self, x):
                if x:  # +1 (nesting restarts inside a class)
                    return 1
                return 0

        return Impl
    return None
";
        assert_eq!(score(source)?, 2);
        Ok(())
    }
}
