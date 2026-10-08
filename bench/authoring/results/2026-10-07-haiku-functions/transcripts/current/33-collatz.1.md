# 33-collatz

## Task

Count the Collatz steps for 6: while the number is not 1, halve it when even and otherwise replace it with 3n+1. Log the number of steps.

## Reply 0

```shards
6 >= n
0 >= steps
While({ n | IsNot(1) } {
  If({ n | Div(2) | Mul(2) | Is(n) }
    { n | Div(2) > n }
    { n | Mul(3) | Add(1) > n }
  )
  Inc(steps)
})
steps | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["8"],"outcomes":[{"wire":"root","outcome":"completed","value":"8"}],"spawned_failures":[]}
stderr:
```
