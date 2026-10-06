# 11-seq-sum

## Task

Log the sum of the elements of the sequence [3 8 1 9 4].

## Reply 0

```shards
0 >= sum
0 >= i
[3 8 1 9 4] = xs
Repeat({
  xs | Take(i) | Math.Add(sum) > sum
  Inc(i)
} Times: 5)
sum | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["25"],"outcomes":[{"wire":"root","outcome":"completed","value":"25"}],"spawned_failures":[]}
stderr:
```
