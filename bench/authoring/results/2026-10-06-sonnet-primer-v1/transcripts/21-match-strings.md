# 21-match-strings

## Task

Map colour names to codes: "red" is 1, "green" is 2, anything else is 0. For the colours "green", "blue", "red" (in that order), log each code on its own line.

## Reply 0

```shards
["green" "blue" "red"] = colours
0 >= i
Repeat({
  colours | Take(i)
  | Match(["red" {1} "green" {2} none {0}] Passthrough: false)
  | Log
  Inc(i)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
stdout:
2
0
1
root: 0
stderr:
```
