# 22-classify-sign

## Task

For each of the integers -3, 0 and 7 (in that order), log `negative`, `zero` or `positive`.

## Reply 0

```shards
@wire(classify {
  If(
    Predicate: {Is(0)}
    Then: {Const("zero")}
    Else: {
      If(
        Predicate: {IsLess(0)}
        Then: {Const("negative")}
        Else: {Const("positive")}
      )
    }
  ) | Log
})

-3 | Do(classify)
0 | Do(classify)
7 | Do(classify)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["negative","zero","positive"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"positive\""}],"spawned_failures":[]}
stderr:
```
