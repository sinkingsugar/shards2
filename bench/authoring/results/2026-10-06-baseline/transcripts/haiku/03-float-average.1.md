# 03-float-average

## Task

Log the average of the floats 3.0, 4.5 and 6.0.

## Reply 0

```shards
0.0 >= total
[3.0 4.5 6.0] = nums
0 >= i
Repeat({ nums | Take(i) | Math.Add(total) > total  Inc(i) } Times: 3)
total | Math.Divide(3) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["4.5"],"outcomes":[{"wire":"root","outcome":"completed","value":"4.5"}],"spawned_failures":[]}
stderr:
```
