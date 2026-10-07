# 34-gcd

## Task

Compute the greatest common divisor of 48 and 18 with Euclid's algorithm and log it.

## Reply 0

```shards
48 | Var(a)
18 | Var(b)
While({b | IsNot(0)} {
  a | Math.Divide(b) | Math.Multiply(b) = mult
  a | Math.Subtract(mult) = mod-result
  b | Update(a)
  mod-result | Update(b)
})
a | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["6"],"outcomes":[{"wire":"root","outcome":"completed","value":"6"}],"spawned_failures":[]}
stderr:
```
