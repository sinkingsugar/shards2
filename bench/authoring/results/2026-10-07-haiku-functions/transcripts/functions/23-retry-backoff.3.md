# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
Keep(attempt 1)
Keep(wait-ticks 1)

While(
  {attempt | IsLess(4)}
  {
    If(
      {attempt | Is(3)}
      {
        f"attempt {attempt} succeeded" | Log
        Return(none)
      }
      {
        f"attempt {attempt} failed, waiting {wait-ticks} ticks" | Log
        wait-ticks | ToFloat | Pause
        Inc(attempt)
        wait-ticks | Math.Multiply(2) | Update(wait-ticks)
      }
    )
  }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"too-many-arguments","message":"Return takes at most 0 arguments, got 1","file":"solution.shs","line":11,"column":9,"shard":"Return","path":[{"wire":"root"},{"shard":2,"name":"While"},{"param":"action"},{"shard":0,"name":"If"},{"param":"then"},{"shard":3,"name":"Return"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
Keep(attempt 1)
Keep(wait-ticks 1)

While(
  {attempt | IsLess(4)}
  {
    If(
      {attempt | Is(3)}
      {
        f"attempt {attempt} succeeded" | Log
        Stop
      }
      {
        f"attempt {attempt} failed, waiting {wait-ticks} ticks" | Log
        wait-ticks | ToFloat | Pause
        Inc(attempt)
        wait-ticks | Math.Multiply(2) | Update(wait-ticks)
      }
    )
  }
)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"root","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
