# 31-string-lengths

## Task

For each string in ["a" "shards" "hello"], log its length in characters, one per line.

## Reply 0

```shards
["a" "shards" "hello"] = words
0 >= i
Repeat({
  words | Take(i) | Count | Log
  Inc(i)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
stdout:
1
6
5
root: 0
stderr:
```
