# 23-retry-backoff

## Task

Simulate retrying an operation that fails on its first two attempts and succeeds on the third (decide success from the attempt number). Use exponential backoff counted in scheduler ticks: after a failed attempt, wait 1 tick the first time, then 2, doubling each time, actually suspending for that many ticks. Log `attempt N failed, waiting D ticks` after each failure and `attempt N succeeded` on success, then finish.

## Reply 0

```shards
0 >= attempt
1 >= wait-duration

Repeat({
  Inc(attempt)
  
  If({ attempt | Is(3) }
    {
      "attempt 3 succeeded" | Log
      Stop
    }
    {
      f"attempt {attempt} failed, waiting {wait-duration} ticks" | Log
      Repeat({ Pause(0) } Times: wait-duration)
      wait-duration | Math.Multiply(2) > wait-duration
    }
  )
} Forever: true)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"wrong-argument-type","message":"Pause: Seconds must be Float, got Int","file":"solution.shs","line":14,"column":16,"shard":"Pause","actual":{"name":"Int","basic_type":4},"expected":[{"name":"Float","basic_type":10}],"param_index":0,"param":"Seconds","path":[{"wire":"root"},{"shard":4,"name":"Repeat"},{"param":"Action"},{"shard":1,"name":"If"},{"param":"Else"},{"shard":3,"name":"Repeat"},{"param":"Action"},{"shard":0,"name":"Pause"}]}]}
```

## Reply 1

```shards
0 >= attempt
1 >= wait-duration

Repeat({
  Inc(attempt)
  
  If({ attempt | Is(3) }
    {
      "attempt 3 succeeded" | Log
      Stop
    }
    {
      f"attempt {attempt} failed, waiting {wait-duration} ticks" | Log
      Repeat({ Pause(0.0) } Times: wait-duration)
      wait-duration | Math.Multiply(2) > wait-duration
    }
  )
} Forever: true)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt 1 failed, waiting 1 ticks","attempt 2 failed, waiting 2 ticks","attempt 3 succeeded"],"outcomes":[{"wire":"root","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
