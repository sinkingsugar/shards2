# 05-sum-range

## Task

Compute the sum of the integers from 1 to 100 with a loop (not a closed formula) and log it.

## Reply 0

```shards
0 >= sum
1 >= i
Repeat({
  i | Add(sum) > sum
  Inc(i)
} Times: 100)
sum | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["5050"],"outcomes":[{"wire":"root","outcome":"completed","value":"5050"}],"spawned_failures":[]}
stderr:
```
