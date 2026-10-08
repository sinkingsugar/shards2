# 34-gcd

## Task

Compute the greatest common divisor of 48 and 18 with Euclid's algorithm and log it.

## Reply 0

```shards
48 | Var(a)
18 | Var(b)
While({b | IsNot(0)} {
  b | Var(temp)
  a | Math.Divide(b) | Math.Multiply(b) | Var(quotient-times-b)
  a | Math.Subtract(quotient-times-b) | Update(b)
  temp | Update(a)
})
a | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["6"],"outcomes":[{"wire":"root","outcome":"completed","value":"6"}],"spawned_failures":[]}
stderr:
```
