# 12-seq-max

## Task

Log the largest element of the sequence [3 8 1 9 4]. Do not hard-code the answer; scan the sequence.

## Reply 0

```shards
[3 8 1 9 4] = xs
xs | Take(0) >= best
0 >= i
xs | Count = n
Repeat({
  When({xs | Take(i) | IsMore(best)} {xs | Take(i) > best})
  Inc(i)
} Times: n)
best | Log
```

## Run (pass)

```
exit Some(0)
stdout:
9
root: 9
stderr:
```
