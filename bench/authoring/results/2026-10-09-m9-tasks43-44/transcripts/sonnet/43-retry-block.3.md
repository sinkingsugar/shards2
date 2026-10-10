# 43-retry-block

## Task

Write a function `Retry` with an Int parameter `times` and a block parameter `action`. It runs `action` until a run succeeds, at most `times` times, and stops at the first success; if every run fails, `Retry` fails with the error of the last run. (`"x" | ParseInt` fails.)

Use it twice. First, with `times: 5`, an action that adds 1 to a counter starting at 0, logs it with the label `attempt`, and fails while the counter is below 3; log `done` after the call. Second, with a new counter starting at 0 and `times: 2`, an action that adds 1 to that counter, logs it with the label `attempt`, and always fails; the call must not stop the program: log the counter with the label `failed after` afterwards.

## Reply 0

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None)} {
  false | Var(ok)
  0 | Var(i)
  times | Math.Subtract(1) | Var(limit)
  While({ All({ i | IsLess(limit) } { ok | Not }) } {
    Maybe({ Run(action) true | Update(ok) } silent: true)
    Inc(i)
  })
  When({ ok | Not } { Run(action) })
})

0 | Var(c1)
Retry(times: 5 action: {
  Inc(c1)
  c1 | Log("attempt")
  When({ c1 | IsLess(3) } { "x" | ParseInt })
})
"done" | Log

0 | Var(c2)
Maybe({
  Retry(times: 2 action: {
    Inc(c2)
    c2 | Log("attempt")
    "x" | ParseInt
  })
} silent: true)
c2 | Log("failed after")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Int","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Int","basic_type":4},"expected":[{"name":"None","basic_type":0}],"path":[{"wire":"root"},{"shard":2,"name":"Retry"},{"function":"Retry"}]},{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"Retry declares output None but its body outputs Int","file":"solution.shs","line":1,"column":1,"shard":"Retry","actual":{"name":"Int","basic_type":4},"expected":[{"name":"None","basic_type":0}],"path":[{"function":"Retry"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(Retry input: None output: None params: {times: Int action: Flow(input: None)} {
  false | Var(ok)
  0 | Var(i)
  times | Math.Subtract(1) | Var(limit)
  While({ All({ i | IsLess(limit) } { ok | Not }) } {
    Maybe({ Run(action) true | Update(ok) } silent: true)
    Inc(i)
  })
  When({ ok | Not } { Run(action) })
  none
})

0 | Var(c1)
Retry(times: 5 action: {
  Inc(c1)
  c1 | Log("attempt")
  When({ c1 | IsLess(3) } { "x" | ParseInt })
})
"done" | Log

0 | Var(c2)
Maybe({
  Retry(times: 2 action: {
    Inc(c2)
    c2 | Log("attempt")
    "x" | ParseInt
  })
} silent: true)
c2 | Log("failed after")
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["attempt: 1","attempt: 2","attempt: 3","done","attempt: 1","attempt: 2","failed after: 2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
