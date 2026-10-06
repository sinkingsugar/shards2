# 36-double-until

## Task

Start from 1 and keep doubling while the value is not greater than 100. Log the first value greater than 100.

## Reply 0

```shards
1 >= v
While({v | IsLessEqual(100)} { v | Math.Multiply(2) > v })
v | Log
```

## Run (pass)

```
exit Some(0)
stdout:
128
root: 128
stderr:
```
