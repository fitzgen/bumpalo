extern crate criterion;

use criterion::{criterion_group, criterion_main, Criterion, ParameterizedBenchmark, Throughput};

#[derive(Default)]
struct Small(u8);

impl bumpalo::BumpAllocSafe for Small {}

#[derive(Default)]
struct Big([usize; 32]);

impl bumpalo::BumpAllocSafe for Big {}

fn allocate<T: Default + bumpalo::BumpAllocSafe>(n: usize) {
    let arena = bumpalo::Bump::new();
    for _ in 0..n {
        let val: &mut T = arena.alloc(Default::default());
        criterion::black_box(val);
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench(
        "allocate",
        ParameterizedBenchmark::new(
            "allocate-small",
            |b, n| b.iter(|| allocate::<Small>(*n)),
            (1..3).map(|n| n * 1000).collect::<Vec<usize>>(),
        )
        .throughput(|n| Throughput::Elements(*n as u32)),
    );

    c.bench(
        "allocate",
        ParameterizedBenchmark::new(
            "allocate-big",
            |b, n| b.iter(|| allocate::<Big>(*n)),
            (1..3).map(|n| n * 1000).collect::<Vec<usize>>(),
        )
        .throughput(|n| Throughput::Elements(*n as u32)),
    );
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
