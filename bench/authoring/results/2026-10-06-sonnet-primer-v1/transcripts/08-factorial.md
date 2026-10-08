# 08-factorial

## Task

Compute 10! (the factorial of 10) as an integer and log it.

## Reply 0

```shards
1 >= result
1 >= i
Repeat({
  result | Math.Multiply(i) > result
  Inc(i)
} Times: 10)
result | Log
```

Wait — this multiplies by `i` before incrementing, and `i` starts at 1, so it computes 1·1·2·…·10 = 3628800 correctly.

## Run (pass)

```
exit Some(0)
stdout:
3628800
root: 3628800
stderr:
```
