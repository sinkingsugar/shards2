# 04-count-up

## Task

Log the integers 1 to 5, one per line, using a loop.

## Reply 0

```shards
1 | Var(n)
Repeat({ n | Log Inc(n) } times: 5)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","3","4","5"],"outcomes":[{"wire":"root","outcome":"completed","value":"1"}],"spawned_failures":[]}
stderr:
```
