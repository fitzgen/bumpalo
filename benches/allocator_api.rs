//! This benchmark is adapted from [`blink-alloc`] (MIT/Apache-2) but with a
//! bunch of extraneous stuff trimmed away.
//!
//! [`blink-alloc`]: https://github.com/zakarumych/blink-alloc/blob/845b2db273371260eef2e9858386f6c6aa180e98/benches/bench.rs

#![feature(allocator_api)]

use criterion::*;
use std::{
    alloc::{AllocError, Allocator, Layout},
    cell::RefCell,
    collections::HashMap,
    mem,
    ptr::NonNull,
    time::{Duration, Instant},
};

/// Trait for resetting a bump allocator to its initial state.
trait BumpAllocator: Default
where
    for<'a> &'a Self: Allocator,
{
    fn with_capacity(cap: usize) -> Self;
    fn reset(&mut self);
}

type Bumpalo = bumpalo::Bump<{ std::mem::align_of::<usize>() }>;
impl BumpAllocator for Bumpalo {
    fn with_capacity(cap: usize) -> Self {
        let b = Bumpalo::with_min_align_and_capacity(cap);
        b.set_allocation_limit(Some(cap));
        b
    }

    #[inline(always)]
    fn reset(&mut self) {
        self.reset();
    }
}

impl BumpAllocator for blink_alloc::BlinkAlloc {
    fn with_capacity(cap: usize) -> Self {
        blink_alloc::BlinkAlloc::with_chunk_size(cap)
    }

    #[inline(always)]
    fn reset(&mut self) {
        self.reset();
    }
}

/// System allocator, as if it were a bump allocator. See caveats in
/// `benches/README.md`; it isn't expected that this super accurately reflects
/// the system allocator's performance.
#[derive(Default)]
struct SystemAlloc {
    alloc: std::alloc::System,
    live: RefCell<HashMap<NonNull<u8>, Layout>>,
}

impl BumpAllocator for SystemAlloc {
    fn with_capacity(cap: usize) -> Self {
        SystemAlloc {
            alloc: std::alloc::System,
            live: RefCell::new(HashMap::with_capacity(cap)),
        }
    }

    fn reset(&mut self) {
        let mut live = self.live.borrow_mut();
        for (ptr, layout) in live.drain() {
            unsafe {
                self.alloc.deallocate(ptr, layout);
            }
        }
    }
}

unsafe impl<'a> Allocator for &'a SystemAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = self.alloc.allocate(layout)?;

        let mut live = self.live.borrow_mut();
        live.insert(ptr.cast(), layout);

        Ok(ptr)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.alloc.deallocate(ptr, layout);
        let mut live = self.live.borrow_mut();
        live.remove(&ptr);
    }

    fn allocate_zeroed(&self, _layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unimplemented!()
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        {
            let mut live = self.live.borrow_mut();
            live.remove(&ptr);
        }

        let ptr = self.alloc.grow(ptr, old_layout, new_layout)?;

        let mut live = self.live.borrow_mut();
        live.insert(ptr.cast(), new_layout);

        Ok(ptr)
    }

    unsafe fn grow_zeroed(
        &self,
        _ptr: NonNull<u8>,
        _old_layout: Layout,
        _new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        unimplemented!()
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        {
            let mut live = self.live.borrow_mut();
            live.remove(&ptr);
        }

        let ptr = self.alloc.shrink(ptr, old_layout, new_layout)?;

        let mut live = self.live.borrow_mut();
        live.insert(ptr.cast(), new_layout);

        Ok(ptr)
    }
}

// The number of allocations to perform in each iteration of the
// benchmarks. This used to be 17453, but it wasn't clear to me why that number
// was chosen, or how it related to the warm up allocation's size. Instead, I've
// chosen 10_007 because it is a prime number and therefore should hopefully
// help us avoid any kind of unwanted harmonics in our measurements. It is also
// large enough that we can start to filter out the noise from our alloc
// operations, but small enough that running the benchmarks takes a reasonable
// amount of time. Finally, I factored out the warm-up logic to be directly tied
// to this number, and ensure that we avoid measuring any resizes during our
// allocations, as (a) they are already covered by the "warm-up" benchmark and
// (b) resizing is rare and amortized across allocations (which happen
// frequently, and whose performance is actually important).
const NUM_ALLOCS: usize = 10_007;

fn bench_allocator_api<A>(name: &str, c: &mut Criterion)
where
    for<'a> &'a A: Allocator,
    A: BumpAllocator + Default + 'static,
{
    let mut group = c.benchmark_group(format!("allocator-api/{name}"));

    group.bench_function(format!("allocate(u8) x {NUM_ALLOCS}"), |b| {
        let mut alloc = A::with_capacity(mem::size_of::<u8>() * NUM_ALLOCS);
        b.iter(|| {
            for _ in 0..NUM_ALLOCS {
                let ptr = (&alloc).allocate(Layout::new::<u8>()).unwrap();
                black_box(ptr);
            }
            alloc.reset();
        })
    });

    group.bench_function(format!("allocate(u32) x {NUM_ALLOCS}"), |b| {
        let mut alloc = A::with_capacity(mem::size_of::<u32>() * NUM_ALLOCS);
        b.iter(|| {
            for _ in 0..NUM_ALLOCS {
                let ptr = (&alloc).allocate(Layout::new::<u32>()).unwrap();
                black_box(ptr);
            }
            alloc.reset();
        })
    });

    group.bench_function(format!("allocate(u64) x {NUM_ALLOCS}"), |b| {
        let mut alloc = A::with_capacity(mem::size_of::<u64>() * NUM_ALLOCS);
        b.iter(|| {
            for _ in 0..NUM_ALLOCS {
                let ptr = (&alloc).allocate(Layout::new::<u64>()).unwrap();
                black_box(ptr);
            }
            alloc.reset();
        })
    });

    group.bench_function(format!("allocate(u128) x {NUM_ALLOCS}"), |b| {
        let mut alloc = A::with_capacity(mem::size_of::<u128>() * NUM_ALLOCS);
        b.iter(|| {
            for _ in 0..NUM_ALLOCS {
                let ptr = (&alloc).allocate(Layout::new::<u128>()).unwrap();
                black_box(ptr);
            }
            alloc.reset();
        })
    });

    // Choose some small, medium, and "large" lengths, as well as some prime
    // numbers to see how the allocators deal with "unaligned" sizes.
    for len in [0, 1, 7, 8, 31, 32] {
        group.bench_function(format!("allocate([u8; {len}]) x {NUM_ALLOCS}"), |b| {
            let mut alloc = A::with_capacity(mem::size_of::<u8>() * len * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    // NB: black box the length but not the whole layout, since
                    // that more accurately reflects things like `Vec` where the
                    // element size (and therefore its alignment) is statically
                    // known but the collection length is dynamic.
                    let len = black_box(len);
                    let layout = Layout::array::<u8>(len).unwrap();

                    let ptr = (&alloc).allocate(layout).unwrap();
                    black_box(ptr);
                }
                alloc.reset();
            })
        });
    }

    group.bench_function(
        format!("grow same align (u32 -> [u32; 2]) x {NUM_ALLOCS}"),
        |b| {
            let mut alloc = A::with_capacity(mem::size_of::<[u32; 2]>() * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    unsafe {
                        let ptr = black_box(&alloc).allocate(Layout::new::<u32>()).unwrap();
                        let ptr = black_box(&alloc)
                            .grow(ptr.cast(), Layout::new::<u32>(), Layout::new::<[u32; 2]>())
                            .unwrap();
                        black_box(ptr);
                    }
                }
                alloc.reset();
            })
        },
    );

    group.bench_function(
        format!("grow smaller align (u32 -> [u16; 4]) x {NUM_ALLOCS}"),
        |b| {
            let mut alloc = A::with_capacity(mem::size_of::<[u16; 4]>() * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    unsafe {
                        let ptr = black_box(&alloc).allocate(Layout::new::<u32>()).unwrap();
                        let ptr = black_box(&alloc)
                            .grow(ptr.cast(), Layout::new::<u32>(), Layout::new::<[u16; 4]>())
                            .unwrap();
                        black_box(ptr);
                    }
                }
                alloc.reset();
            })
        },
    );

    group.bench_function(
        format!("grow larger align (u32 -> u64) x {NUM_ALLOCS}"),
        |b| {
            let mut alloc = A::with_capacity(mem::size_of::<u64>() * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    unsafe {
                        let ptr = black_box(&alloc).allocate(Layout::new::<u32>()).unwrap();
                        let ptr = black_box(&alloc)
                            .grow(ptr.cast(), Layout::new::<u32>(), Layout::new::<u64>())
                            .unwrap();
                        black_box(ptr);
                    }
                }
                alloc.reset();
            })
        },
    );

    group.bench_function(
        format!("shrink same align ([u32; 2] -> u32) x {NUM_ALLOCS}"),
        |b| {
            let mut alloc = A::with_capacity(mem::size_of::<u32>() * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    unsafe {
                        let ptr = black_box(&alloc)
                            .allocate(Layout::new::<[u32; 2]>())
                            .unwrap();
                        let ptr = black_box(&alloc)
                            .shrink(ptr.cast(), Layout::new::<[u32; 2]>(), Layout::new::<u32>())
                            .unwrap();
                        black_box(ptr);
                    }
                }
                alloc.reset();
            })
        },
    );

    group.bench_function(
        format!("shrink smaller align (u32 -> u16) x {NUM_ALLOCS}"),
        |b| {
            let mut alloc = A::with_capacity(mem::size_of::<u32>() * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    unsafe {
                        let ptr = black_box(&alloc).allocate(Layout::new::<u32>()).unwrap();
                        let ptr = black_box(&alloc)
                            .shrink(ptr.cast(), Layout::new::<u32>(), Layout::new::<u16>())
                            .unwrap();
                        black_box(ptr);
                    }
                }
                alloc.reset();
            })
        },
    );

    group.bench_function(
        format!("shrink larger align ([u16; 4] -> u32) x {NUM_ALLOCS}"),
        |b| {
            let mut alloc = A::with_capacity(mem::size_of::<[u16; 4]>() * NUM_ALLOCS);
            b.iter(|| {
                for _ in 0..NUM_ALLOCS {
                    unsafe {
                        let ptr = black_box(&alloc)
                            .allocate(Layout::new::<[u16; 4]>())
                            .unwrap();
                        let ptr = black_box(&alloc)
                            .shrink(ptr.cast(), Layout::new::<[u16; 4]>(), Layout::new::<u32>())
                            .unwrap();
                        black_box(ptr);
                    }
                }
                alloc.reset();
            })
        },
    );

    group.finish();
}

fn bench_warm_up<A>(name: &str, c: &mut Criterion)
where
    for<'a> &'a A: Allocator,
    A: BumpAllocator + Default,
{
    let mut group = c.benchmark_group(format!("warm-up/{name}"));

    group.bench_function(format!("first u32 allocation"), |b| {
        b.iter(|| {
            let alloc = A::default();
            let ptr = black_box(&alloc).allocate(Layout::new::<u32>()).unwrap();
            black_box(ptr);
        })
    });

    group.finish();
}

fn bench_reset<A>(name: &str, c: &mut Criterion)
where
    for<'a> &'a A: Allocator,
    A: BumpAllocator + Default,
{
    let mut group = c.benchmark_group(format!("reset/{name}"));

    group.bench_function(format!("reset after allocate(u32) x {NUM_ALLOCS}"), |b| {
        b.iter_custom(move |iters| {
            let mut duration = Duration::from_millis(0);

            for _ in 0..iters {
                // NB: do not use `with_capacity` here, we want to measure
                // resetting with multiple internal bump chunks.
                let mut alloc = A::default();

                for _ in 0..NUM_ALLOCS {
                    black_box((&alloc).allocate(Layout::new::<u32>()).unwrap());
                }

                let start = Instant::now();

                alloc.reset();
                black_box(&alloc);

                duration += start.elapsed();
            }

            duration
        });
    });

    group.finish();
}

fn bench_vec<A>(name: &str, c: &mut Criterion)
where
    for<'a> &'a A: Allocator,
    A: BumpAllocator + Default,
{
    let mut group = c.benchmark_group(format!("vec/{name}"));

    // Additional room because the vectors are going to potentially resize
    // multiple times.
    const RESIZE_FACTOR: usize = 10;

    group.bench_function(format!("push(usize) x {NUM_ALLOCS}"), |b| {
        let mut alloc = A::with_capacity(mem::size_of::<usize>() * NUM_ALLOCS * RESIZE_FACTOR);
        b.iter(|| {
            let mut vec = Vec::new_in(&alloc);
            for i in 0..NUM_ALLOCS {
                vec.push(i);
            }
            drop(vec);
            alloc.reset();
        })
    });

    group.bench_function(format!("reserve_exact(1) x {NUM_ALLOCS}"), |b| {
        let mut alloc = A::with_capacity(mem::size_of::<usize>() * NUM_ALLOCS * RESIZE_FACTOR);
        b.iter(|| {
            let mut vec = Vec::<u32, &A>::new_in(&alloc);
            for i in 0..NUM_ALLOCS {
                vec.reserve_exact(i);
            }
            drop(vec);
            alloc.reset();
        })
    });

    group.finish();
}

pub fn criterion_benchmark(c: &mut Criterion) {
    bench_allocator_api::<Bumpalo>("bumpalo::Bump", c);
    bench_allocator_api::<blink_alloc::BlinkAlloc>("blink_alloc::BlinkAlloc", c);
    bench_allocator_api::<SystemAlloc>("std::alloc::System", c);

    bench_warm_up::<Bumpalo>("bumpalo::Bump", c);
    bench_warm_up::<blink_alloc::BlinkAlloc>("blink_alloc::BlinkAlloc", c);
    bench_warm_up::<SystemAlloc>("std::alloc::System", c);

    bench_reset::<Bumpalo>("bumpalo::Bump", c);
    bench_reset::<blink_alloc::BlinkAlloc>("blink_alloc::BlinkAlloc", c);
    bench_reset::<SystemAlloc>("std::alloc::System", c);

    bench_vec::<Bumpalo>("bumpalo::Bump", c);
    bench_vec::<blink_alloc::BlinkAlloc>("blink_alloc::BlinkAlloc", c);
    bench_vec::<SystemAlloc>("std::alloc::System", c);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
