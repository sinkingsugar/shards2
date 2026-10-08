# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
30 = t1
-1 >= r1
0 >= i1
Repeat({
  When({[10 20 30 40] | Take(i1) | Is(t1)} {i1 > r1})
  Inc(i1)
} Times: 4)
r1 | Log

99 = t2
-1 >= r2
0 >= i2
Repeat({
  When({[10 20 30 40] | Take(i2) | Is(t2)} {i2 > r2})
  Inc(i2)
} Times: 4)
r2 | Log
```

## Run (pass)

```
exit Some(0)
stdout:
2
-1
root: -1
stderr:
```
