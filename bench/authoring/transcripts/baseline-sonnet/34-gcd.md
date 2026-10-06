# 34-gcd

## Task

Compute the greatest common divisor of 48 and 18 with Euclid's algorithm and log it.

## Reply 0

```shards
48 >= a
18 >= b
0 >= t
While({b | IsMore(0)} {
  a | Math.Divide(b) | Math.Multiply(b) > t
  a | Math.Subtract(t) > t
  b > a
  t > b
})
a | Log
```

## Run (pass)

```
exit Some(0)
stdout:
6
root: 6
stderr:
```
