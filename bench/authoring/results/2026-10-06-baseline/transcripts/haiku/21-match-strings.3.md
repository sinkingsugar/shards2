# 21-match-strings

## Task

Map colour names to codes: "red" is 1, "green" is 2, anything else is 0. For the colours "green", "blue", "red" (in that order), log each code on its own line.

## Reply 0

```shards
["green" "blue" "red"] = colors
0 >= i
Repeat({
  colors | Take(i) | Match([
    "red" {Const(1)}
    "green" {Const(2)}
    none {Const(0)}
  ] Passthrough: false) | Log
  Inc(i)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","0","1"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
