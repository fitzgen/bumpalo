use criterion::*;

#[derive(Default)]
struct Small(u8);

#[derive(Default)]
struct Big([usize; 32]);

fn alloc<T: Default>(n: usize) {
    let arena = bumpalo::Bump::with_capacity(n * std::mem::size_of::<T>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: &mut T = arena.alloc(black_box(Default::default()));
        black_box(val);
    }
}

fn alloc_with<T: Default>(n: usize) {
    let arena = bumpalo::Bump::with_capacity(n * std::mem::size_of::<T>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: &mut T = arena.alloc_with(|| black_box(Default::default()));
        black_box(val);
    }
}

fn alloc_try_with<T: Default, E>(n: usize) {
    let arena = bumpalo::Bump::with_capacity(n * std::mem::size_of::<Result<T, E>>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: Result<&mut T, E> = arena.alloc_try_with(|| black_box(Ok(Default::default())));
        let _ = black_box(val);
    }
}

fn alloc_try_with_err<T, E: Default>(n: usize) {
    // Only enough capacity for one, since the allocation is undone.
    let arena = bumpalo::Bump::with_capacity(std::mem::size_of::<Result<T, E>>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: Result<&mut T, E> = arena.alloc_try_with(|| black_box(Err(Default::default())));
        let _ = black_box(val);
    }
}

fn try_alloc<T: Default>(n: usize) {
    let arena = bumpalo::Bump::with_capacity(n * std::mem::size_of::<T>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: Result<&mut T, _> = arena.try_alloc(black_box(Default::default()));
        let _ = black_box(val);
    }
}

fn try_alloc_with<T: Default>(n: usize) {
    let arena = bumpalo::Bump::with_capacity(n * std::mem::size_of::<T>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: Result<&mut T, _> = arena.try_alloc_with(|| black_box(Default::default()));
        let _ = black_box(val);
    }
}

fn try_alloc_try_with<T: Default, E>(n: usize) {
    let arena = bumpalo::Bump::with_capacity(n * std::mem::size_of::<Result<T, E>>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: Result<&mut T, bumpalo::AllocOrInitError<E>> =
            arena.try_alloc_try_with(|| black_box(Ok(Default::default())));
        let _ = black_box(val);
    }
}

fn try_alloc_try_with_err<T, E: Default>(n: usize) {
    // Only enough capacity for one, since the allocation is undone.
    let arena = bumpalo::Bump::with_capacity(std::mem::size_of::<Result<T, E>>());
    for _ in 0..n {
        let arena = black_box(&arena);
        let val: Result<&mut T, bumpalo::AllocOrInitError<E>> =
            arena.try_alloc_try_with(|| black_box(Err(Default::default())));
        let _ = black_box(val);
    }
}

#[cfg(feature = "collections")]
fn format_realloc(bump: &bumpalo::Bump, n: usize) {
    let n = criterion::black_box(n);
    let s = bumpalo::format!(in bump, "Hello {:.*}", n, "World! ");
    criterion::black_box(s);
}

#[cfg(feature = "collections")]
fn string_from_str_in(bump: &bumpalo::Bump, str: &str) {
    let str = criterion::black_box(str);
    let s = bumpalo::collections::string::String::from_str_in(str, bump);
    criterion::black_box(s);
}

#[cfg(feature = "collections")]
fn string_push_str(bump: &bumpalo::Bump, str: &str) {
    let str = criterion::black_box(str);
    let mut s = bumpalo::collections::string::String::with_capacity_in(str.len(), bump);
    s.push_str(str);
    criterion::black_box(s);
}

#[cfg(feature = "collections")]
fn extend_u8(bump: &bumpalo::Bump, slice: &[u8]) {
    let slice = criterion::black_box(slice);
    let mut vec = bumpalo::collections::Vec::<u8>::with_capacity_in(slice.len(), bump);
    vec.extend(slice.iter().copied());
    criterion::black_box(vec);
}

#[cfg(feature = "collections")]
fn extend_from_slice_u8(bump: &bumpalo::Bump, slice: &[u8]) {
    let slice = criterion::black_box(slice);
    let mut vec = bumpalo::collections::Vec::<u8>::with_capacity_in(slice.len(), bump);
    vec.extend_from_slice(slice);
    criterion::black_box(vec);
}

#[cfg(feature = "collections")]
fn extend_from_slice_copy_u8(bump: &bumpalo::Bump, slice: &[u8]) {
    let slice = criterion::black_box(slice);
    let mut vec = bumpalo::collections::Vec::<u8>::with_capacity_in(slice.len(), bump);
    vec.extend_from_slice_copy(slice);
    criterion::black_box(vec);
}

const ALLOCATIONS: usize = 10_000;

fn bench_extend_from_slice_copy(c: &mut Criterion) {
    let lengths = &[
        4usize,
        5,
        8,
        11,
        16,
        64,
        128,
        331,
        1024,
        4 * 1024,
        16 * 1024,
    ];

    for len in lengths.iter().copied() {
        let str = "x".repeat(len);
        let mut group = c.benchmark_group(format!("extend {len} bytes"));
        group.throughput(Throughput::Elements(len as u64));
        group.bench_function("extend", |b| {
            let mut bump = bumpalo::Bump::with_capacity(len);
            b.iter(|| {
                bump.reset();
                extend_u8(&bump, str.as_bytes());
            });
        });
        group.bench_function("extend_from_slice", |b| {
            let mut bump = bumpalo::Bump::with_capacity(len);
            let str = "x".repeat(len);
            b.iter(|| {
                bump.reset();
                extend_from_slice_u8(&bump, str.as_bytes());
            });
        });
        group.bench_function("extend_from_slice_copy", |b| {
            let mut bump = bumpalo::Bump::with_capacity(len);
            let str = "x".repeat(len);
            b.iter(|| {
                bump.reset();
                extend_from_slice_copy_u8(&bump, str.as_bytes());
            });
        });
        group.finish();
    }
}

fn bench_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small", |b| b.iter(|| alloc::<Small>(ALLOCATIONS)));
    group.bench_function("big", |b| b.iter(|| alloc::<Big>(ALLOCATIONS)));
}

fn bench_alloc_with(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc-with");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small", |b| b.iter(|| alloc_with::<Small>(ALLOCATIONS)));
    group.bench_function("big", |b| b.iter(|| alloc_with::<Big>(ALLOCATIONS)));
}

fn bench_alloc_try_with(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc-try-with");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small, small", |b| {
        b.iter(|| alloc_try_with::<Small, Small>(ALLOCATIONS))
    });
    group.bench_function("small, big", |b| {
        b.iter(|| alloc_try_with::<Small, Big>(ALLOCATIONS))
    });
    group.bench_function("big, small", |b| {
        b.iter(|| alloc_try_with::<Big, Small>(ALLOCATIONS))
    });
    group.bench_function("big, big", |b| {
        b.iter(|| alloc_try_with::<Big, Big>(ALLOCATIONS))
    });
}

fn bench_alloc_try_with_err(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc-try-with-err");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small, small", |b| {
        b.iter(|| alloc_try_with_err::<Small, Small>(ALLOCATIONS))
    });
    group.bench_function("small, big", |b| {
        b.iter(|| alloc_try_with_err::<Small, Big>(ALLOCATIONS))
    });
    group.bench_function("big, small", |b| {
        b.iter(|| alloc_try_with_err::<Big, Small>(ALLOCATIONS))
    });
    group.bench_function("big, big", |b| {
        b.iter(|| alloc_try_with_err::<Big, Big>(ALLOCATIONS))
    });
}

fn bench_try_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("try-alloc");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small", |b| b.iter(|| try_alloc::<Small>(ALLOCATIONS)));
    group.bench_function("big", |b| b.iter(|| try_alloc::<Big>(ALLOCATIONS)));
}

fn bench_try_alloc_with(c: &mut Criterion) {
    let mut group = c.benchmark_group("try-alloc-with");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small", |b| b.iter(|| try_alloc_with::<Small>(ALLOCATIONS)));
    group.bench_function("big", |b| b.iter(|| try_alloc_with::<Big>(ALLOCATIONS)));
}

fn bench_try_alloc_try_with(c: &mut Criterion) {
    let mut group = c.benchmark_group("try-alloc-try-with");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small, small", |b| {
        b.iter(|| try_alloc_try_with::<Small, Small>(ALLOCATIONS))
    });
    group.bench_function("small, big", |b| {
        b.iter(|| try_alloc_try_with::<Small, Big>(ALLOCATIONS))
    });
    group.bench_function("big, small", |b| {
        b.iter(|| try_alloc_try_with::<Big, Small>(ALLOCATIONS))
    });
    group.bench_function("big, big", |b| {
        b.iter(|| try_alloc_try_with::<Big, Big>(ALLOCATIONS))
    });
}

fn bench_try_alloc_try_with_err(c: &mut Criterion) {
    let mut group = c.benchmark_group("try-alloc-try-with-err");
    group.throughput(Throughput::Elements(ALLOCATIONS as u64));
    group.bench_function("small, small", |b| {
        b.iter(|| try_alloc_try_with_err::<Small, Small>(ALLOCATIONS))
    });
    group.bench_function("small, big", |b| {
        b.iter(|| try_alloc_try_with_err::<Small, Big>(ALLOCATIONS))
    });
    group.bench_function("big, small", |b| {
        b.iter(|| try_alloc_try_with_err::<Big, Small>(ALLOCATIONS))
    });
    group.bench_function("big, big", |b| {
        b.iter(|| try_alloc_try_with_err::<Big, Big>(ALLOCATIONS))
    });
}

fn bench_format_realloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("format-realloc");

    for n in (1..5).map(|n| n * n * n * 10) {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("format-realloc", n), &n, |b, n| {
            let mut bump = bumpalo::Bump::new();
            b.iter(|| {
                bump.reset();
                format_realloc(&bump, *n);
            });
        });
    }
}

fn bench_string_from_str_in(c: &mut Criterion) {
    let len: usize = 16;

    let mut group = c.benchmark_group("alloc");
    group.throughput(Throughput::Elements(len as u64));
    group.bench_function("from_str_in", |b| {
        let mut bump = bumpalo::Bump::with_capacity(len);
        let str = "x".repeat(len);
        b.iter(|| {
            bump.reset();
            string_from_str_in(&bump, &*str);
        });
    });
}

fn bench_string_push_str(c: &mut Criterion) {
    let len: usize = 16 * 1024; // 16 KiB

    let mut group = c.benchmark_group("alloc");
    group.throughput(Throughput::Elements(len as u64));
    group.bench_function("push_str", |b| {
        let mut bump = bumpalo::Bump::with_capacity(len);
        let str = "x".repeat(len);
        b.iter(|| {
            bump.reset();
            string_push_str(&bump, &*str);
        });
    });
}

criterion_group!(
    benches,
    bench_extend_from_slice_copy,
    bench_alloc,
    bench_alloc_with,
    bench_alloc_try_with,
    bench_alloc_try_with_err,
    bench_try_alloc,
    bench_try_alloc_with,
    bench_try_alloc_try_with,
    bench_try_alloc_try_with_err,
    bench_format_realloc,
    bench_string_from_str_in,
    bench_string_push_str
);
criterion_main!(benches);
