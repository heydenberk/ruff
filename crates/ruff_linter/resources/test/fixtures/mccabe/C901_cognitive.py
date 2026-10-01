# Scores assume `lint.mccabe.metric = "cognitive"`. A `# N` comment on a `def` line is that
# function's expected score, and `# +N` marks the increment contributed by a line.


def trivial():  # 0
    pass


def if_elif_else(n):  # 3
    if n > 0:  # +1
        return "positive"
    elif n < 0:  # +1
        return "negative"
    else:  # +1
        return "zero"


def factorial(n):  # 2
    return 1 if n <= 1 else n * factorial(n - 1)  # +1 ternary, +1 recursion


def sum_of_primes(limit):  # 7
    total = 0
    for i in range(2, limit + 1):  # +1
        for j in range(2, i):  # +2 (nesting = 1)
            if i % j == 0:  # +3 (nesting = 2)
                break
        else:  # +1
            total += i
    return total


def get_words(number):  # 1
    match number:  # +1
        case 1:
            return "one"
        case 2:
            return "a couple"
        case _:
            return "lots"


# `inner` is not reported on its own: its score counts toward `outer`.
def outer(items):  # 2
    def inner(item):
        if item:  # +2 (nesting = 1)
            return 1
        return 0

    return [inner(item) for item in items]


# A wrapper function adds no nesting, and is not reported on its own.
def decorator(func):  # 1
    """Wrap `func`."""

    def wrapper(value):
        if value:  # +1
            return func(value)
        return None

    return wrapper


# Methods of a class defined inside a function count toward the function, not on their own.
def factory(flag):  # 2
    class Impl:
        def run(self):
            if flag:  # +1 (a class body resets nesting)
                return 1

    if flag:  # +1
        return Impl
    return None


class Service:
    def handle(self, request):  # 17
        for header in request.headers:  # +1
            if header.name == "x":  # +2 (nesting = 1)
                for part in header.parts:  # +3 (nesting = 2)
                    if part and (part.a or part.b):  # +4 (nesting = 3), +2 for `and`, `or`
                        while part.pending:  # +5 (nesting = 4)
                            pass
        return None


class Value:
    # A bare call in a method refers to the builtin, not to the method, so it isn't recursion.
    def format(self, spec):  # 1
        if spec:  # +1
            return format(self.value, spec)
        return format(self.value)
