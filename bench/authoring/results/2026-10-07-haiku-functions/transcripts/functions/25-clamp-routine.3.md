# 25-clamp-routine

## Task

Define a reusable named routine `Clamp` that clamps its input to a lower and an upper bound given as parameters, and call it (do not copy its body) three times: 5 with bounds 0..3, -2 with bounds 0..3, and 2 with bounds 0..3. Log each result.

## Reply 0

```shards
@fn(Clamp input: Int output: Int params: {lower: Int upper: Int} {
  If({input | IsLess(lower)} {lower} {If({input | IsMore(upper)} {upper} {input})})
})

5 | Clamp(lower: 0 upper: 3) | Log
-2 | Clamp(lower: 0 upper: 3) | Log
2 | Clamp(lower: 0 upper: 3) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["3","0","2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
