# 22-classify-sign

## Task

For each of the integers -3, 0 and 7 (in that order), log `negative`, `zero` or `positive`.

## Reply 0

```shards
-3 | If({IsLess(0)} {Log("negative")} {If({Is(0)} {Log("zero")} {Log("positive")})})
0 | If({IsLess(0)} {Log("negative")} {If({Is(0)} {Log("zero")} {Log("positive")})})
7 | If({IsLess(0)} {Log("negative")} {If({Is(0)} {Log("zero")} {Log("positive")})})
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["negative: -3","zero: 0","positive: 7"],"outcomes":[{"wire":"root","outcome":"completed","value":"7"}],"spawned_failures":[]}
stderr:
```
