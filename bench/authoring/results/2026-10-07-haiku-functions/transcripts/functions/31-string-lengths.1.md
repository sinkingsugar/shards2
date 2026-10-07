# 31-string-lengths

## Task

For each string in ["a" "shards" "hello"], log its length in characters, one per line.

## Reply 0

```shards
0 | Var(i)
["a" "shards" "hello"] = strings
Repeat({
  strings | Take(i) | Count | Log
  Inc(i)
} times: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","6","5"],"outcomes":[{"wire":"root","outcome":"completed","value":"[\"a\" \"shards\" \"hello\"]"}],"spawned_failures":[]}
stderr:
```
