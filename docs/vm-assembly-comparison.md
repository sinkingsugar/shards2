# Release VM assembly comparison

Inspected 2026-10-05, using the actual binaries from the
[compose-cleanup benchmark](vm-execution-benchmarks.md#compose-time-scratch-release-selection).
1.x is the existing GCC release build at `37597927e`; 2.0 is Rust 1.98.1 default
release, based on `4cbfb0e` with compose-time cleanup selection. Binary hashes,
exact objdump commands and raw excerpts are in
[the result directory](../bench/vm-execution/results/2026-10-05-compose-cleanup/).
No debug build, tracing instrumentation, or toy replacement loop was used.

The comparison is `shards::activateShards` in 1.x versus
`shards_core::inline::run` in 2.0. Both contain the hot inline handlers; comparing
only out-of-line shard methods would miss these paths. 1.x handler addresses
were resolved using its generated `InlineShard` enum and the relative jump
table at `0x936da8c`. The excerpts below omit surrounding dispatch and error
paths where noted. Registers are those in the measured binaries.

## Const: both forward a pointer

1.x (`0x1b6abd1`, handler selected by `CoreConst`):

```asm
lea r12, [rbx+0xd0]       ; address of the shard's stored SHVar
```

2.0 no-release Const (`0x24a327`, then common tail `0x24a68e`):

```asm
add r12, 0x8             ; address of the instruction's stored Var
jmp const_common_tail
mov r14, r12             ; accumulator pointer
jmp next_instruction
```

Neither copies/refcounts the value per Const. Necessary ownership release is
in a separate compose-selected opcode, not a check in this fast handler.
The remaining Const gap therefore involves dispatch/loop/boundary costs, not
an expensive constant operation or an iterator abstraction.

## Get: pre-resolved pointer versus frame-slot resolution

1.x (`0x1b6ab3e`, `CoreGet`):

```asm
mov r12, [rbx+0xd8]       ; cached variable-cell pointer
```

This corresponds to `GetRuntime::core._cell`, resolved before activation.

2.0 no-release Get (`0x24a4ad`):

```asm
movzx ecx, byte ptr [r12+0x8] ; Local or Mesh binding
mov   rax, [r12+0x10]        ; slot index
 test cl, cl
mov   rdx, [rsp+0x1b0]       ; local frame length
cmovne rdx, [rsp+0x1c0]      ; or mesh frame length
cmp   rax, rdx
jae   bounds_failure
 test cl, cl
mov   rcx, [rsp+0x58]        ; local frame base
cmovne rcx, [rsp+0x1b8]      ; or mesh frame base
lea   rax, [rax+rax*2]
lea   r14, [rcx+rax*8]       ; base + index * 24 (sizeof Var)
jmp   next_instruction
```

The local/mesh selection uses conditional moves here, not conditional jumps,
but it still consumes instructions and dependencies. The bounds check has a
conditional branch. No scratch check, Var clone or Arc increment occurs in
this fast Get handler. Compose already knows the binding kind/index: separate
Local/Mesh opcodes and precomputed byte offsets are concrete future experiments.
Eliminating bounds checks would additionally require a proved frame-layout
contract at entry; it should not be done by simply deleting the check.

1.x's cached pointer lives in per-instance shard state. Our compiled stream is
shared by instances with different frame addresses. Storing instance pointers
in that shared stream would violate the design. Specialized offsets, or an
explicit per-instance binding table, can preserve the split.

## Dispatch: the Rust Op representation adds decoding

1.x (`0x1b6aae8`) reads a plain integer opcode and indexes a jump table:

```asm
mov    ecx, [rbx]
cmp    ecx, 0x2b
ja     generic_handler
mov    esi, ecx
movsxd rdi, dword ptr [r14+rsi*4]
add    rdi, r14
jmp    rdi
```

2.0 (`0x24a2e4`) calculates the 32-byte instruction address, then decodes the
compiler-selected enum representation before its jump table:

```asm
mov    rax, rbp
shl    rax, 5
mov    rcx, [rsp+0x170]
lea    r12, [rcx+rax]
mov    rax, [rcx+rax]
mov    rcx, rax
movabs rdx, 0x8000000000000000
xor    rcx, rdx
 test  rax, rax
mov    eax, 0xc
cmovns rcx, rax
lea    rdx, [jump_table]
movsxd rax, dword ptr [rdx+rcx*4]
add    rax, rdx
jmp    rax
```

That is real generated overhead. The high-bit decode is consistent with Rust
using an otherwise invalid payload representation to encode enum variants;
the important observable fact is that dispatch is not a plain byte-tag load.
This is the **Op layout**, distinct from the public **Var layout**.
An explicit opcode representation is worth benchmarking; fewer decode
instructions can trade against instruction size/alignment and cache footprint.
This inspection does not claim that changing `repr` alone achieves parity.

The complete loops have other differences: 1.x checks context flow state after
each activation and advances a null-terminated array of shard pointers. 2.0
increments an index and checks instruction count; its uninterrupted segment has
no per-op context flow-state check. Rust reloads some frame/code metadata from
stack slots. Both use indirect dispatch. Counting only one side's handler or
only the number of assembly lines would be misleading.

## Add: packed arithmetic is already present

1.x's `Math.Add` compose chooses Int64x2 for **Int or Int2**, Float64x2 for
**Float or Float2**, and Float32x4 for **Float3 or Float4** (source:
`shards/modules/core/math_binary.hpp`). Scalar values use the relevant low lane
of the same aligned 16-byte payload operations:

```asm
; Int path, 0x1b6b190
mov     rsi, [rbx+0x110]       ; resolved RHS pointer
vmovdqa xmm8, [rsi]
vpaddq  xmm9, xmm8, [r12]
lea     r12, [rbx+0xd0]
vmovdqa [rbx+0xd0], xmm9
; Float path uses vaddpd; Float4 uses vaddps
```

The handlers also read the context flow state, then join the common loop tail.
They do not check input tags, rebuild the output tag, or branch on integer
overflow. The packed integer instruction wraps lane overflow. 2.0 deliberately
reports integer overflow and retains a runtime input-tag check:

```asm
; AddIntConst, 0x24a477
cmp  byte ptr [r14], 2
jne  invalid_input
mov  rax, [r14+8]
add  rax, [r12+8]
jo   overflow_error
mov  byte ptr [rsp+0xb0], 2
mov  [rsp+0xb8], rax
lea  r14, [rsp+0xb0]
jmp  next_instruction
```

For Float4, Rust uses `movups` + **`addps`** + `movups`; 1.x uses aligned AVX
encodings with **`vaddps`**. Both perform four packed float additions. This
build difference also reflects target flags: 1.x uses `-march=broadwell`, while
2.0 uses default Cargo release targeting. No runtime arithmetic performance
claim should be attributed solely to the mnemonic prefix or alignment.

1.x writes each result into the current shard's `_result`; Rust reuses one
numeric scratch slot across the segment. Rust also embeds constant operands
in instructions, whereas the inspected 1.x handler loads a resolved operand
pointer. These differences help explain why extra checks do not automatically
make Rust slower: the matched scalar Int chain is actually faster in 2.0.

## What the evidence supports

At width 256 the paired median 2.0 stackless / 1.x time ratios are Const(Int)
1.44×, Get(Int) 1.99×, Add(Int) 0.75×, Add(Float) 0.98× and Add(Float4) 1.03×.
The clearest remaining cheap-shard candidates are opcode decoding and binding
resolution. This is static assembly inspection plus measured end-to-end chains,
not a hardware-counter attribution of the exact time spent in each instruction.

Next experiments: explicit opcode representation; Local/Mesh-specialized
bindings with precomputed offsets; then fusion that retains source/error and
ownership boundaries. Arithmetic already has packed SIMD where relevant.
Removing overflow semantics or trusting arbitrary host output without checking
would be a separate contract decision, not a free compiler optimization.
