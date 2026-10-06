# 38-vector-add

## Task

Add the 3D float vectors (1, 2, 3) and (4, 5, 6) and log the resulting vector.

## Reply 0

```shards
[1.0 2.0 3.0] | ToFloat3 | Math.Add([4.0 5.0 6.0] | ToFloat3) | Log
```

Wait, the Add operand must be a literal or variable, so a flow is not allowed. Corrected program:

```shards
[4.0 5.0 6.0] | ToFloat3 = b
[1.0 2.0 3.0] | ToFloat3 | Math.Add(b) | Log
```

## Run (pass)

```
exit Some(0)
stdout:
@f3(5 7 9)
root: @f3(5.0 7.0 9.0)
stderr:
```
