# 22-classify-sign

## Task

For each of the integers -3, 0 and 7 (in that order), log `negative`, `zero` or `positive`.

## Reply 0

```shards
-3 | Match([0 {"zero"}] default: {If({IsLess(0)} {"negative"} else: {"positive"})}) | Log
0 | Match([0 {"zero"}] default: {If({IsLess(0)} {"negative"} else: {"positive"})}) | Log
7 | Match([0 {"zero"}] default: {If({IsLess(0)} {"negative"} else: {"positive"})}) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["negative","zero","positive"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"positive\""}],"spawned_failures":[]}
stderr:
```
