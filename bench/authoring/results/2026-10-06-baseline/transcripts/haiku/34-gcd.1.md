# 34-gcd

## Task

Compute the greatest common divisor of 48 and 18 with Euclid's algorithm and log it.

## Reply 0

```shards
48 >= a
18 >= b
While({b | IsNot(0)} {
  b = temp
  a | Math.Divide(temp) | Math.Multiply(temp) = prod
  a | Sub(prod) > b
  temp > a
})
a | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["6"],"outcomes":[{"wire":"root","outcome":"completed","value":"6"}],"spawned_failures":[]}
stderr:
```
