# 11-seq-sum

## Task

Log the sum of the elements of the sequence [3 8 1 9 4].

## Reply 0

```shards
[3 8 1 9 4] = xs
0 >= total
0 >= i
Repeat({ xs | Take(i) | Math.Add(total) > total  Inc(i) } Times: 5)
total | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["25"],"outcomes":[{"wire":"root","outcome":"completed","value":"25"}],"spawned_failures":[]}
stderr:
```
