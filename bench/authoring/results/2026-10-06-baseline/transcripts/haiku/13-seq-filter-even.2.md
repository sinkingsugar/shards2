# 13-seq-filter-even

## Task

From the sequence [1 2 3 4 5 6 7 8], keep only the even numbers and log the resulting sequence (as one value).

## Reply 0

```shards
[] >= result
[1 2 3 4 5 6 7 8] = xs
0 >= i
Repeat({
  xs | Take(i) = n
  If({n | Math.Divide(2) | Math.Multiply(2) | Is(n)} {n >> result})
  Inc(i)
} Times: 8)
result | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[2 4 6 8]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[2 4 6 8]"}],"spawned_failures":[]}
stderr:
```
