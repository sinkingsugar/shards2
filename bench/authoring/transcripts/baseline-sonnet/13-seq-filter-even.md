# 13-seq-filter-even

## Task

From the sequence [1 2 3 4 5 6 7 8], keep only the even numbers and log the resulting sequence (as one value).

## Reply 0

```shards
[1 2 3 4 5 6 7 8] = xs
[] >= evens
0 >= i
Repeat({
  xs | Take(i) = x
  When({x | Math.Divide(2) | Math.Multiply(2) | Is(x)} {x >> evens})
  Inc(i)
} Times: 8)
evens | Log
```

## Run (pass)

```
exit Some(0)
stdout:
[2 4 6 8]
root: [2 4 6 8]
stderr:
```
