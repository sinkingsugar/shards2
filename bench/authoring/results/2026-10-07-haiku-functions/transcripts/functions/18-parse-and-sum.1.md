# 18-parse-and-sum

## Task

Parse each string in ["10" "20" "30"] as an integer and log their sum.

## Reply 0

```shards
["10" "20" "30"] = strings
0 | Var(sum)
0 | Var(i)
Repeat({ strings | Take(i) | ParseInt | Math.Add(sum) | Update(sum) | Inc(i) } times: 3)
sum | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["60"],"outcomes":[{"wire":"root","outcome":"completed","value":"60"}],"spawned_failures":[]}
stderr:
```
