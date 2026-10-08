# 05-sum-range

## Task

Compute the sum of the integers from 1 to 100 with a loop (not a closed formula) and log it.

## Reply 0

```shards
0 >= sum
0 >= i
Repeat({
  Inc(i) | Math.Add(sum) > sum
} Times: 100)
sum | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["5050"],"outcomes":[{"wire":"root","outcome":"completed","value":"5050"}],"spawned_failures":[]}
stderr:
```
