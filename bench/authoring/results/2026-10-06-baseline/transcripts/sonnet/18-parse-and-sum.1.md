# 18-parse-and-sum

## Task

Parse each string in ["10" "20" "30"] as an integer and log their sum.

## Reply 0

```shards
["10" "20" "30"] = xs
0 >= i
0 >= total
Repeat({ xs | Take(i) | ExpectString | ParseInt | Math.Add(total) > total  Inc(i) } Times: 3)
total | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["60"],"outcomes":[{"wire":"root","outcome":"completed","value":"60"}],"spawned_failures":[]}
stderr:
```
