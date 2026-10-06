# 22-classify-sign

## Task

For each of the integers -3, 0 and 7 (in that order), log `negative`, `zero` or `positive`.

## Reply 0

```shards
[-3 0 7] = xs
0 >= i
Repeat({
  xs | Take(i)
  If({IsLess(0)}
    {"negative" | Log}
    {If({Is(0)} {"zero" | Log} {"positive" | Log})})
  Inc(i)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
stdout:
negative
zero
positive
root: 0
stderr:
```
