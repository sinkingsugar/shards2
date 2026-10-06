# 05-sum-range

## Task

Compute the sum of the integers from 1 to 100 with a loop (not a closed formula) and log it.

## Reply 0

```shards
0 >= total
0 >= i
Repeat({ Inc(i) | Math.Add(total) > total } Times: 100)
total | Log
```

## Run (pass)

```
exit Some(0)
stdout:
5050
root: 5050
stderr:
```
