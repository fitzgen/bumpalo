# Benchmarks

## Table of Contents

- [Overview](#overview)
- [Reproducing](#reproducing)
- [Benchmark Results](#benchmark-results)
    - [allocator-api](#allocator-api)
    - [warm-up](#warm-up)
    - [reset](#reset)
    - [vec](#vec)

## Overview

This directory contains two suites of benchmarks:

1. `allocator_api.rs`: `std::alloc::Allocator`-based benchmarks that aim to
   measure the performance of bump allocators within the generic `Allocator`
   API.

2. `benches.rs`: Miscellaneous Bumpalo-specific benchmarks.

The tables of benchmark results listed below are the results for the suite of
`std::alloc::Allocator`-based benchmarks. They are originally adapted from
[`blink-alloc`] (another fine bump allocator crate) which was already measuring
the relative performance between `blink-alloc` and `bumpalo`. I wasn't able to
reproduce many of their results showing , which was part of the motivation to bring a
subset of them into this repo and document reproduction.

Furthermore, the tables below include a `std::alloc::System` column, but their
results come with a few caveats. First, in order to implement a `reset` method
for the system allocator and deallocate everything that was allocated within a
certain region of code, I had to add additional bookkeeping to dynamically track
every live allocation. That bookkeeping generally won't be present in real
programs, which will instead use things like `Drop` implementations, so it makes
the system allocator's results look worse than they otherwise would
be. Additionally, these benchmarks are really designed to show off the strengths
of bump allocators and measure the operations that are important for bump
allocators. The system allocator is expected to perform worse, but that's
because it is designed for general purpose scenarios, where as bump allocators
are designed for very specific scenarios. These columns should mostly serve as
just a general reference point to get an idea of the magnitude of allocation
speed up you can expect in the very specific scenarios where using a bump
allocator makes sense.

Finally, all these benchmarks are synthetic. They are micro benchmarks. You
shouldn't expect that anything here will directly translate into speed ups for
your application. Application performance is what really matters, and things
observed in the micro often disappear in the macro. If your application isn't
bottlenecked on allocation, and can't live with the constraints a bump allocator
imposes, there's nothing that a bump allocator can do to help you.

[`blink-alloc`]: https://github.com/zakarumych/blink-alloc/blob/845b2db273371260eef2e9858386f6c6aa180e98/BENCHMARKS.md

## Reproducing

The `std::alloc::Allocator`-based benchmarks require using nightly Rust, since
the `Allocator` trait is still unstable. You must additionally enable Bumpalo's
`allocator_api` cargo feature:

```
$ cargo +nightly bench --bench allocator_api --features allocator_api
```

The miscellaneous benchmarks require Bumpalo's `collections` cargo feature:

```
$ cargo bench --bench benches --features collections
```

To update the tables below, use `cargo-criterion` and [`criterion-table`]:

```
$ cd bumpalo/benches/
$ cargo +nightly bench --features allocator_api \
    --bench allocator_api \
    --message-format=json \
    > results.json
$ criterion-table < results.json > README.md
```

[`cargo-criterion`]: https://github.com/bheisler/cargo-criterion
[`criterion-table`]: https://github.com/nu11ptr/criterion-table

## Benchmark Results

### allocator-api

Benchmarks that measure calls into `std::alloc::Allocator` methods directly.

These operations are generally the ones that happen most often, and therefore
their performance is generally most important. Following the same logic, raw
allocation is generally the very most important.

|                                    | `bumpalo::Bump`          | `blink_alloc::BlinkAlloc`          | `std::alloc::System`               |
|:-----------------------------------|:-------------------------|:-----------------------------------|:---------------------------------- |
| **`allocate(u32) x 10007`**        | `15.80 us` (✅ **1.00x**) | `16.46 us` (✅ **1.04x slower**)    | `528.30 us` (❌ *33.43x slower*)    |
| **`grow same align x 10007`**      | `32.34 us` (✅ **1.00x**) | `28.74 us` (✅ **1.13x faster**)    | `1.15 ms` (❌ *35.67x slower*)      |
| **`grow smaller align x 10007`**   | `32.56 us` (✅ **1.00x**) | `28.90 us` (✅ **1.13x faster**)    | `1.15 ms` (❌ *35.35x slower*)      |
| **`grow larger align x 10007`**    | `37.67 us` (✅ **1.00x**) | `39.28 us` (✅ **1.04x slower**)    | `1.15 ms` (❌ *30.40x slower*)      |
| **`shrink same align x 10007`**    | `28.13 us` (✅ **1.00x**) | `20.00 us` (✅ **1.41x faster**)    | `1.07 ms` (❌ *37.86x slower*)      |
| **`shrink smaller align x 10007`** | `27.94 us` (✅ **1.00x**) | `19.72 us` (✅ **1.42x faster**)    | `1.06 ms` (❌ *37.84x slower*)      |

### warm-up

Benchmarks that measure the first allocation in a fresh allocator.

These aren't generally very important, since the first allocation in a fresh
bump allocator only ever happens once by definition. This is mostly measuring
how long it takes the underlying system allocator to allocate the initial chunk
to bump allocate out of.

|                            | `bumpalo::Bump`          | `blink_alloc::BlinkAlloc`          | `std::alloc::System`             |
|:---------------------------|:-------------------------|:-----------------------------------|:-------------------------------- |
| **`first u32 allocation`** | `26.06 ns` (✅ **1.00x**) | `20.02 ns` (✅ **1.30x faster**)    | `74.16 ns` (❌ *2.85x slower*)    |

### reset

Benchmarks that measure the overhead of resetting a bump allocator to an empty
state, ready to be reused in a new program phase.

This generally doesn't happen as often as allocation, and therefore is generally
less important, but it is important to keep an eye on generally since
deallocation-en-masse and reusing already-allocated chunks can be selling points
for bump allocation over using a generic allocator in certain scenarios.

|                                         | `bumpalo::Bump`           | `blink_alloc::BlinkAlloc`          | `std::alloc::System`                 |
|:----------------------------------------|:--------------------------|:-----------------------------------|:------------------------------------ |
| **`reset after allocate(u32) x 10007`** | `126.43 ns` (✅ **1.00x**) | `190.09 ns` (❌ *1.50x slower*)     | `130.17 us` (❌ *1029.57x slower*)    |

### vec

Benchmarks that measure the various `std::vec::Vec<T> operations when used in
conjuction with a bump allocator.

Bump allocators aren't often used directly, but instead through some sort of
collection. These benchmarks are important in the sense that the standard
`Vec<T>` type is probably the most-commonly used collection (although not
necessarily the most commonly used with bump allocators in Rust, at least until
the `Allocator` trait is stabilized).

|                                | `bumpalo::Bump`          | `blink_alloc::BlinkAlloc`          | `std::alloc::System`              |
|:-------------------------------|:-------------------------|:-----------------------------------|:--------------------------------- |
| **`push x 10007`**             | `20.06 us` (✅ **1.00x**) | `17.73 us` (✅ **1.13x faster**)    | `39.39 us` (❌ *1.96x slower*)     |
| **`reserve_exact(1) x 10007`** | `2.25 ms` (✅ **1.00x**)  | `54.43 us` (🚀 **41.36x faster**)   | `641.79 us` (🚀 **3.51x faster**)  |

---
Made with [criterion-table](https://github.com/nu11ptr/criterion-table)

