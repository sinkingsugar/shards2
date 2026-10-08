# 31-string-lengths

## Task

For each string in ["a" "shards" "hello"], log its length in characters, one per line.

## Reply 0

```shards
["a" "shards" "hello"] = xs
0 | Var(i)
Repeat({
  xs | Take(i) | Count | Log
  Inc(i)
} times: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","6","5"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
