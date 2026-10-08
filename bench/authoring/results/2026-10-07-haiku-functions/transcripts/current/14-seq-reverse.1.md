# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = original
5 >= len
[] >= reversed
Repeat({
  Dec(len) = index
  original | Take(index) | Push(reversed)
} Times: 5)
reversed | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[5 4 3 2 1]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[5 4 3 2 1]"}],"spawned_failures":[]}
stderr:
```
