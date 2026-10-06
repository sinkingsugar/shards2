# 20-match-default

## Task

For each of the integers 1, 2 and 5 (in that order), log `one` for 1, `two` for 2 and `other` for anything else. Use a match/switch construct rather than a chain of ifs.

## Reply 0

```shards
1 | Match([1 {"one" | Log} 2 {"two" | Log} none {"other" | Log}])
2 | Match([1 {"one" | Log} 2 {"two" | Log} none {"other" | Log}])
5 | Match([1 {"one" | Log} 2 {"two" | Log} none {"other" | Log}])
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["one","two","other"],"outcomes":[{"wire":"root","outcome":"completed","value":"5"}],"spawned_failures":[]}
stderr:
```
