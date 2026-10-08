# 35-in-range

## Task

For each of 3, 8 and 12, log whether it lies between 5 and 10 inclusive, as a Bool (`true` or `false`), one per line.

## Reply 0

```shards
[3 8 12] = xs
0 >= i
Repeat({
  xs | Take(i) | If(
    {All({IsMoreEqual(5)} {IsLessEqual(10)})}
    {true}
    {false}
  ) | Log
  Inc(i)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
stdout:
false
true
false
root: 0
stderr:
```
