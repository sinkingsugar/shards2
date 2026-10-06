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
stdout:
25
root: 25
stderr:
```
