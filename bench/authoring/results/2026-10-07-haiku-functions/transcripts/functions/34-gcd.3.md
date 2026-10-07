# 34-gcd

## Task

Compute the greatest common divisor of 48 and 18 with Euclid's algorithm and log it.

## Reply 0

```shards
48 | Var(a)
18 | Var(b)
0 | Var(temp)
0 | Var(quotient-times-b)

While(
  predicate: { b | IsNot(0) }
  action: {
    a | Math.Divide(b) | Math.Multiply(b) | Update(quotient-times-b)
    a | Math.Subtract(quotient-times-b) | Update(temp)
    b | Update(a)
    temp | Update(b)
  }
)

a | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["6"],"outcomes":[{"wire":"root","outcome":"completed","value":"6"}],"spawned_failures":[]}
stderr:
```
