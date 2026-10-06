# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = xs
xs | Take(4) >> out
3 >= i
While({ i | IsMoreEqual(0) } { xs | Take(i) >> out  Dec(i) })
out | Log
```

## Run (pass)

```
exit Some(0)
stdout:
[5 4 3 2 1]
root: [5 4 3 2 1]
stderr:
```
