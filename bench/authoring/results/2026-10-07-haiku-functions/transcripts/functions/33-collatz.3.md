# 33-collatz

## Task

Count the Collatz steps for 6: while the number is not 1, halve it when even and otherwise replace it with 3n+1. Log the number of steps.

## Reply 0

```shards
6 | Var(n)
0 | Var(steps)
While({ n | IsNot(1) }
  {
    If({ n | Div(2) | Mul(2) | Is(n) }
      { n | Div(2) | Update(n) }
      { n | Mul(3) | Add(1) | Update(n) }
    )
    Inc(steps)
  }
)
steps | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["8"],"outcomes":[{"wire":"root","outcome":"completed","value":"8"}],"spawned_failures":[]}
stderr:
```
