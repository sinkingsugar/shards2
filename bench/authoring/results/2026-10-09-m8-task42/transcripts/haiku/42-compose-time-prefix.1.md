# 42-compose-time-prefix

## Task

Write a function `Label` that takes a String parameter `name` and an Int parameter `number` and outputs them joined by a dash, like `build-7`. `Log`'s `prefix` takes only a literal: compute the prefix `build-7` with `Label` at compose time, and log the numbers 1, 2 and 3 with it.

## Reply 0

```shards
@fn(Label input: None output: String params: {name: String number: Int} {
  f"{name}-{number}"
})

@const(prefix #(Label(name: "build" number: 7)))

1 | Log(@prefix)
2 | Log(@prefix)
3 | Log(@prefix)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["build-7: 1","build-7: 2","build-7: 3"],"outcomes":[{"wire":"root","outcome":"completed","value":"3"}],"spawned_failures":[]}
stderr:
```
