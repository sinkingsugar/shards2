# 19-parse-fallback

## Task

Parse the strings "12" and "abc" as integers. When parsing fails, use -1 instead. Log the two results, one per line. The program must not fail.

## Reply 0

```shards
"12" | Maybe({ParseInt} {-1} Silent: true) | Log
"abc" | Maybe({ParseInt} {-1} Silent: true) | Log
```

## Run (pass)

```
exit Some(0)
stdout:
12
-1
root: -1
stderr:
```
