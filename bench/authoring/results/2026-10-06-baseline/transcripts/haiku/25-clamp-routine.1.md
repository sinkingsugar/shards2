# 25-clamp-routine

## Task

Define a reusable named routine `clamp` that clamps a value to a lower and an upper bound, and call it (do not copy its body) three times: 5 with bounds 0..3, -2 with bounds 0..3, and 2 with bounds 0..3. Log each result.

## Reply 0

```shards
@wire(clamp {
  If(IsLess(min-val) min-val If(IsMore(max-val) max-val))
})

0 >= min-val
3 >= max-val
5 | Do(clamp) | Log

0 > min-val
3 > max-val
-2 | Do(clamp) | Log

0 > min-val
3 > max-val
2 | Do(clamp) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["3","0","2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
