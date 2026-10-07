# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@fn(FindIndex input: Int output: Int params: {seq: [Int]} {
  0 | Var(i)
  seq | Count = limit
  Repeat({
    If(
      { seq | Take(i) | Is(input) }
      { i | Return }
    )
    Inc(i)
  } times: limit)
  -1
})

[10 20 30 40] = xs
30 | FindIndex(seq: xs) | Log
99 | FindIndex(seq: xs) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
