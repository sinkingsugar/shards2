# 06-countdown

## Task

Count down from 5 to 1, logging each number on its own line, then log `liftoff`.

## Reply 0

```shards
5 >= n
While({n | IsMore(0)} {
  n | Log
  Dec(n)
})
"liftoff" | Log
```

## Run (pass)

```
exit Some(0)
stdout:
5
4
3
2
1
liftoff
root: "liftoff"
stderr:
```
