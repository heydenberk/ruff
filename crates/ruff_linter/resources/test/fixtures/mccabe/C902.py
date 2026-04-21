### Trivial — complexity 0
def trivial():
    return 0


### Single if — complexity 1
def single_if(x):
    if x:
        return 1
    return 0


### Nested ifs — complexity 6
def nested_ifs(x, y, z):
    if x:  # +1
        if y:  # +2 (nested)
            if z:  # +3 (nested)
                return 1
    return 0


### Flat if chain — complexity 3
def flat_if_chain(x, y, z):
    if x:  # +1
        return 1
    elif y:  # +1
        return 2
    elif z:  # +1
        return 3
    return 0


### Boolean-op single run — complexity 1
def bool_single_run(a, b, c, d):
    return a and b and c and d


### Boolean-op mixed — complexity 3
def bool_mixed(a, b, c, d):
    # (a and b) or (c and d): outer or +1, two inner ands +1 each
    return a and b or c and d


### Ternary chain — complexity 3
def ternary_chain(a, b, c, d):
    return a if a else (b if b else (c if c else d))


### Ternary inside if — complexity 3
def ternary_inside_if(x, a, b):
    if x:  # +1
        # ternary at nesting 1: +1+1
        return a if a else b
    return 0


### Dict literal with ternaries — complexity 2
def dict_ternaries(a, b):
    return {"x": a if a else None, "y": b if b else None}


### Try / except / else / finally — complexity 1
def try_except_else_finally():
    try:
        print(1)
    except Exception:  # +1
        print(2)
    else:
        print(3)
    finally:
        print(4)


### Match — only case bodies count — complexity 2
def match_simple(x):
    match x:
        case 1:
            if x:  # body nesting 1: +1+1
                return 1
        case _:
            return 0


### List comp with filter — complexity 2
def listcomp_filter(xs):
    return [x for x in xs if x]  # comp +1, filter +1


### Nested def contributes nothing to parent — complexity 0
def outer_with_nested(x):
    def inner(y):
        if y:
            return y

    return inner


### For loop with `else` — complexity 1
# Only the `for` itself adds 1; the `else` body adds nothing here, since the
# print call contains no boolean ops or control flow.
def for_else(xs):
    for x in xs:  # +1
        print(x)
    else:
        print("done")


### Deeply nested composite — above default threshold
def deeply_nested(items, flag):
    total = 0
    for item in items:  # +1 (nest 0)
        if item.active:  # +2 (nest 1)
            if flag and item.age > 0 or item.bonus:  # +3 (nest 2) + 2 bool runs
                for extra in item.extras:  # +4 (nest 3)
                    if extra.weight > 0:  # +5 (nest 4)
                        total += extra.weight * item.value
                    else:  # +1
                        total += 1
            else:  # +1
                total -= 1
    return total


### Method on a class — methods are functions too
class Payments:
    def process(self, items):
        for item in items:  # +1
            if item.ok:  # +2 (nested)
                self.send(item)
        return True
