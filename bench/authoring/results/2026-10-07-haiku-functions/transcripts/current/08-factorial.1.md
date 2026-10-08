# 08-factorial

## Task

Compute 10! (the factorial of 10) as an integer and log it.

## Reply 0

```shards
1 >= result
2 >= i
Repeat({
  result | Math.Multiply(i) > result
  Math.Inc(i)
} Until: { i | IsMore(10) })
result | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["3628800"],"outcomes":[{"wire":"root","outcome":"completed","value":"3628800"}],"spawned_failures":[]}
stderr:
```
