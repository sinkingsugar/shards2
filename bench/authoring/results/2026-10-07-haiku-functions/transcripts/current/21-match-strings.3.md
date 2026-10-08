# 21-match-strings

## Task

Map colour names to codes: "red" is 1, "green" is 2, anything else is 0. For the colours "green", "blue", "red" (in that order), log each code on its own line.

## Reply 0

```shards
"green" | Match(["green" {2} "red" {1} none {0}] Passthrough: false) | Log
"blue" | Match(["green" {2} "red" {1} none {0}] Passthrough: false) | Log
"red" | Match(["green" {2} "red" {1} none {0}] Passthrough: false) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","0","1"],"outcomes":[{"wire":"root","outcome":"completed","value":"1"}],"spawned_failures":[]}
stderr:
```
