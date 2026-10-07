# 18-parse-and-sum

## Task

Parse each string in ["10" "20" "30"] as an integer and log their sum.

## Reply 0

```shards
0 >= total
["10" "20" "30"] = strings
0 >= i
Repeat({
  strings | Take(i) | ParseInt | Math.Add(total) > total
  Inc(i)
} Times: 3)
total | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["60"],"outcomes":[{"wire":"root","outcome":"completed","value":"60"}],"spawned_failures":[]}
stderr:
```
