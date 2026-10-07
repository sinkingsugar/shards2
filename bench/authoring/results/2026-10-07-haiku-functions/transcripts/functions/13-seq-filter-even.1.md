# 13-seq-filter-even

## Task

From the sequence [1 2 3 4 5 6 7 8], keep only the even numbers and log the resulting sequence (as one value).

## Reply 0

```shards
[1 2 3 4 5 6 7 8] = input-seq
[] | Var(result)
0 | Var(i)
0 | Var(current)
input-seq | Count = count
Repeat({
  input-seq | Take(i) | Update(current)
  current | If({
    Math.Divide(2) | Math.Multiply(2) | Is(current)
  } {
    Push(result)
  })
  Inc(i)
} times: count)
result | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[2 4 6 8]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[2 4 6 8]"}],"spawned_failures":[]}
stderr:
```
