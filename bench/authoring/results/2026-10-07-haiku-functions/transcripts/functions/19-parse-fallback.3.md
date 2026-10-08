# 19-parse-fallback

## Task

Parse the strings "12" and "abc" as integers. When parsing fails, use -1 instead. Log the two results, one per line. The program must not fail.

## Reply 0

```shards
"12" | ParseInt | Log
"abc" | Maybe({ParseInt} {Const(-1)} silent: true) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["12","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
