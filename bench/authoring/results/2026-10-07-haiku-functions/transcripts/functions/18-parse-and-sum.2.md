# 18-parse-and-sum

## Task

Parse each string in ["10" "20" "30"] as an integer and log their sum.

## Reply 0

```shards
["10" "20" "30"] = strs
0 | Var(total)
0 | Var(i)
Repeat({
  strs | Take(i) | ParseInt | Math.Add(total) | Update(total)
  Inc(i)
} times: 3)
total | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["60"],"outcomes":[{"wire":"root","outcome":"completed","value":"60"}],"spawned_failures":[]}
stderr:
```
