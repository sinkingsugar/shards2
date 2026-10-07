# 20-match-default

## Task

For each of the integers 1, 2 and 5 (in that order), log `one` for 1, `two` for 2 and `other` for anything else. Use a match/switch construct rather than a chain of ifs.

## Reply 0

```shards
[1 2 5] = values
0 >= i
Repeat({
  values | Take(i) | Match([1 {"one"} 2 {"two"} none {"other"}] Passthrough: false) | Log
  Inc(i)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["one","two","other"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
