# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
[10 20 30 40] = xs

// Find 30
0 >= i1
-1 >= result1

Repeat({
  If(
    Predicate: { xs | Take(i1) | Is(30) }
    Then: { Get(i1) > result1 }
  )
  Inc(i1)
} Times: 4)

result1 | Log

// Find 99
0 >= i2
-1 >= result2

Repeat({
  If(
    Predicate: { xs | Take(i2) | Is(99) }
    Then: { Get(i2) > result2 }
  )
  Inc(i2)
} Times: 4)

result2 | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
