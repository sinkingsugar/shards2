# 31-string-lengths

## Task

For each string in ["a" "shards" "hello"], log its length in characters, one per line.

## Reply 0

```shards
["a" "shards" "hello"] = words
0 >= i
Repeat({ words | Take(i) | ExpectString | Count | Log  Inc(i) } Times: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","6","5"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
