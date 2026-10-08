# 22-classify-sign

## Task

For each of the integers -3, 0 and 7 (in that order), log `negative`, `zero` or `positive`.

## Reply 0

```shards
@fn(SignText input: Int output: String params: {} {
  If({IsLess(0)} {"negative"} { If({Is(0)} {"zero"} {"positive"}) })
})

-3 | SignText | Log
0 | SignText | Log
7 | SignText | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["negative","zero","positive"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"positive\""}],"spawned_failures":[]}
stderr:
```
