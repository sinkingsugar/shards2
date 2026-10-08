# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
Keep(attempt 1)
Keep(wait-ticks 1)

Repeat(
  {
    If(
      { attempt | IsLess(3) }
      {
        f"attempt {attempt} failed, waiting {wait-ticks} ticks" | Log
        Repeat({ Pause } times: wait-ticks)
        wait-ticks | Math.Multiply(2) | Update(wait-ticks)
        attempt | Math.Add(1) | Update(attempt)
      }
      {
        f"attempt {attempt} succeeded" | Log
        Return(none)
      }
    )
  }
  forever: true
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"too-many-arguments","message":"Return takes at most 0 arguments, got 1","file":"solution.shs","line":16,"column":9,"shard":"Return","path":[{"wire":"root"},{"shard":2,"name":"Repeat"},{"param":"action"},{"shard":0,"name":"If"},{"param":"else"},{"shard":3,"name":"Return"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
Keep(attempt 1)
Keep(wait-ticks 1)

Repeat(
  {
    If(
      { attempt | IsLess(3) }
      {
        f"attempt {attempt} failed, waiting {wait-ticks} ticks" | Log
        Repeat({ Pause } times: wait-ticks)
        wait-ticks | Math.Multiply(2) | Update(wait-ticks)
        attempt | Math.Add(1) | Update(attempt)
      }
      {
        f"attempt {attempt} succeeded" | Log
        Stop
      }
    )
  }
  forever: true
)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"root","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
