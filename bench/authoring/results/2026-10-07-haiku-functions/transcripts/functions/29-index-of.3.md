# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@fn(FindIndex input: None output: Int params: {seq: [Int] value: Int} {
  0 | Var(i)
  Repeat({
    If({seq | Take(i) | Is(value)}
      {i | Return}
      {}
    )
    Inc(i)
  } times: (seq | Count))
  -1 | Return
})

FindIndex(seq: [10 20 30 40] value: 30) | Log
FindIndex(seq: [10 20 30 40] value: 99) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
