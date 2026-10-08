# 34-gcd

## Task

Compute the greatest common divisor of 48 and 18 with Euclid's algorithm and log it.

## Reply 0

```shards
48 >= a
18 >= b
0 >= quotient-product
0 >= remainder
While(
  { b | IsNot(0) }
  {
    a | Math.Divide(b) | Math.Multiply(b) > quotient-product
    a | Math.Subtract(quotient-product) > remainder
    b > a
    remainder > b
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
