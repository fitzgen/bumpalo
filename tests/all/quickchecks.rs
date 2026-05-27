use crate::quickcheck;
use ::quickcheck::{Arbitrary, Gen};
use bumpalo::Bump;
#[cfg(feature = "allocator_api")]
use std::alloc::Allocator;
use std::alloc::Layout;
use std::mem;
#[cfg(feature = "allocator_api")]
use std::ptr::NonNull;

const MAX_ALLOCATOR_OPS: usize = 64;
const MAX_ALLOCATOR_SLICE_LEN: usize = 96;
const MAX_ALLOCATOR_LIMIT: usize = 4096;
const MAX_LAYOUT_ALLOC_SIZE: usize = 256;

#[derive(Clone, Debug, PartialEq)]
struct BigValue {
    data: [u64; 32],
}

impl BigValue {
    fn new(x: u64) -> BigValue {
        BigValue {
            data: [
                x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x,
                x, x, x, x,
            ],
        }
    }
}

impl Arbitrary for BigValue {
    fn arbitrary(g: &mut Gen) -> BigValue {
        BigValue::new(u64::arbitrary(g))
    }
}

#[derive(Clone, Debug)]
enum Elems<T, U> {
    OneT(T),
    TwoT(T, T),
    FourT(T, T, T, T),
    OneU(U),
    TwoU(U, U),
    FourU(U, U, U, U),
}

impl<T, U> Arbitrary for Elems<T, U>
where
    T: Arbitrary + Clone,
    U: Arbitrary + Clone,
{
    fn arbitrary(g: &mut Gen) -> Elems<T, U> {
        let x: u8 = u8::arbitrary(g);
        match x % 6 {
            0 => Elems::OneT(T::arbitrary(g)),
            1 => Elems::TwoT(T::arbitrary(g), T::arbitrary(g)),
            2 => Elems::FourT(
                T::arbitrary(g),
                T::arbitrary(g),
                T::arbitrary(g),
                T::arbitrary(g),
            ),
            3 => Elems::OneU(U::arbitrary(g)),
            4 => Elems::TwoU(U::arbitrary(g), U::arbitrary(g)),
            5 => Elems::FourU(
                U::arbitrary(g),
                U::arbitrary(g),
                U::arbitrary(g),
                U::arbitrary(g),
            ),
            _ => unreachable!(),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Elems::OneT(_) => Box::new(vec![].into_iter()),
            Elems::TwoT(a, b) => {
                Box::new(vec![Elems::OneT(a.clone()), Elems::OneT(b.clone())].into_iter())
            }
            Elems::FourT(a, b, c, d) => Box::new(
                vec![
                    Elems::TwoT(a.clone(), b.clone()),
                    Elems::TwoT(a.clone(), c.clone()),
                    Elems::TwoT(a.clone(), d.clone()),
                    Elems::TwoT(b.clone(), c.clone()),
                    Elems::TwoT(b.clone(), d.clone()),
                    Elems::TwoT(c.clone(), d.clone()),
                ]
                .into_iter(),
            ),
            Elems::OneU(_) => Box::new(vec![].into_iter()),
            Elems::TwoU(a, b) => {
                Box::new(vec![Elems::OneU(a.clone()), Elems::OneU(b.clone())].into_iter())
            }
            Elems::FourU(a, b, c, d) => Box::new(
                vec![
                    Elems::TwoU(a.clone(), b.clone()),
                    Elems::TwoU(a.clone(), c.clone()),
                    Elems::TwoU(a.clone(), d.clone()),
                    Elems::TwoU(b.clone(), c.clone()),
                    Elems::TwoU(b.clone(), d.clone()),
                    Elems::TwoU(c.clone(), d.clone()),
                ]
                .into_iter(),
            ),
        }
    }
}

fn overlap((a1, a2): (usize, usize), (b1, b2): (usize, usize)) -> bool {
    assert!(a1 < a2);
    assert!(b1 < b2);
    a1 < b2 && b1 < a2
}

// Returns whether `(b1, b2)` is contained in `(a1, a2)`.
fn contains((a1, a2): (usize, usize), (b1, b2): (usize, usize)) -> bool {
    assert!(a1 < a2);
    assert!(b1 < b2);
    a1 <= b1 && b2 <= a2
}

fn range<T>(t: &T) -> (usize, usize) {
    let start = t as *const _ as usize;
    let end = start + mem::size_of::<T>();
    (start, end)
}

#[derive(Clone, Debug)]
enum AllocatorOp {
    Alloc { value: u8, len: u8 },
    Reset,
    SetLimit(Option<u16>),
}

impl Arbitrary for AllocatorOp {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 3 {
            0 => AllocatorOp::Alloc {
                value: u8::arbitrary(g),
                len: u8::arbitrary(g),
            },
            1 => AllocatorOp::Reset,
            _ => AllocatorOp::SetLimit(Option::<u16>::arbitrary(g)),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(std::iter::empty())
    }
}

#[derive(Clone, Debug)]
struct AllocatorProgram(Vec<AllocatorOp>);

impl Arbitrary for AllocatorProgram {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut ops = Vec::<AllocatorOp>::arbitrary(g);
        ops.truncate(MAX_ALLOCATOR_OPS);
        AllocatorProgram(ops)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(|mut ops| {
            ops.truncate(MAX_ALLOCATOR_OPS);
            AllocatorProgram(ops)
        }))
    }
}

#[derive(Clone, Debug)]
enum LayoutAllocatorOp {
    Alloc { size: u16, align_log2: u8 },
    Reset,
    SetLimit(Option<u16>),
}

impl Arbitrary for LayoutAllocatorOp {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 3 {
            0 => LayoutAllocatorOp::Alloc {
                size: u16::arbitrary(g),
                align_log2: u8::arbitrary(g),
            },
            1 => LayoutAllocatorOp::Reset,
            _ => LayoutAllocatorOp::SetLimit(Option::<u16>::arbitrary(g)),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(std::iter::empty())
    }
}

#[derive(Clone, Debug)]
struct LayoutAllocatorProgram(Vec<LayoutAllocatorOp>);

impl Arbitrary for LayoutAllocatorProgram {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut ops = Vec::<LayoutAllocatorOp>::arbitrary(g);
        ops.truncate(MAX_ALLOCATOR_OPS);
        LayoutAllocatorProgram(ops)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(|mut ops| {
            ops.truncate(MAX_ALLOCATOR_OPS);
            LayoutAllocatorProgram(ops)
        }))
    }
}

fn arbitrary_small_layout(size: u16, align_log2: u8) -> Layout {
    let align = 1usize << (usize::from(align_log2) % 7);
    let max_units = (MAX_LAYOUT_ALLOC_SIZE / align).max(1);
    let units = (usize::from(size) % max_units) + 1;
    Layout::from_size_align(units * align, align).unwrap()
}

fn live_bytes(allocs: &[Vec<u8>]) -> Vec<u8> {
    allocs
        .iter()
        .rev()
        .flat_map(|alloc| alloc.iter().copied())
        .collect()
}

fn assert_chunk_views_match(bump: &mut Bump, live_allocs: &[Vec<u8>]) {
    // Safety: We hold `&mut Bump`, so no allocations can occur while the raw
    // iterator is alive and the iterator cannot be invalidated.
    let raw_chunks: Vec<(_, _)> = unsafe { bump.iter_allocated_chunks_raw() }.collect();
    let chunks: Vec<&[_]> = bump.iter_allocated_chunks().collect();
    assert_eq!(raw_chunks.len(), chunks.len());
    for ((ptr, size), chunk) in raw_chunks.into_iter().zip(chunks.iter().copied()) {
        assert_eq!(ptr as *const _, chunk.as_ptr() as *const _);
        assert_eq!(size, chunk.len());
    }

    let actual_bytes: Vec<u8> = bump
        .iter_allocated_chunks()
        .flat_map(|chunk| {
            chunk.iter().map(|byte| {
                // Safety: This helper is only used after allocating `u8` slices
                // into the arena and after clearing the model on `reset`, so the
                // iterated chunk bytes correspond to initialized live `u8` values.
                unsafe { byte.assume_init() }
            })
        })
        .collect();
    assert_eq!(actual_bytes, live_bytes(live_allocs));
}

fn assert_layout_allocations_match(bump: &mut Bump, live_allocs: &[(usize, usize)]) {
    // Safety: We hold `&mut Bump`, so no allocations can occur while the raw
    // iterator is alive and the iterator cannot be invalidated.
    let raw_chunks: Vec<(_, _)> = unsafe { bump.iter_allocated_chunks_raw() }.collect();
    let chunks: Vec<&[_]> = bump.iter_allocated_chunks().collect();
    assert_eq!(raw_chunks.len(), chunks.len());
    for ((ptr, size), chunk) in raw_chunks.iter().copied().zip(chunks.iter().copied()) {
        assert_eq!(ptr as *const _, chunk.as_ptr() as *const _);
        assert_eq!(size, chunk.len());
    }

    for (i, alloc) in live_allocs.iter().enumerate() {
        assert!(raw_chunks.iter().any(|&(ptr, size)| {
            let chunk = (ptr as usize, ptr as usize + size);
            contains(chunk, *alloc)
        }));
        for other in &live_allocs[i + 1..] {
            assert!(!overlap(*alloc, *other));
        }
    }
}

#[derive(Clone, Debug)]
#[cfg(feature = "allocator_api")]
enum RawCheckpointOp {
    TakeCheckpoint,
    ResetToCheckpoint,
    DropCheckpoint,
    Alloc(u8, usize),
    Realloc(u8, usize),
    Dealloc,
}

#[cfg(feature = "allocator_api")]
impl RawCheckpointOp {
    const MAX_SIZE: usize = 100;
}

#[cfg(feature = "allocator_api")]
impl Arbitrary for RawCheckpointOp {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 6 {
            0 => RawCheckpointOp::TakeCheckpoint,
            1 => RawCheckpointOp::ResetToCheckpoint,
            2 => RawCheckpointOp::DropCheckpoint,
            3 => RawCheckpointOp::Alloc(u8::arbitrary(g), usize::arbitrary(g) % Self::MAX_SIZE),
            4 => RawCheckpointOp::Realloc(u8::arbitrary(g), usize::arbitrary(g) % Self::MAX_SIZE),
            5 => RawCheckpointOp::Dealloc,
            _ => unreachable!(),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(std::iter::empty())
    }
}

quickcheck! {
    fn can_allocate_big_values(values: Vec<BigValue>) -> () {
        let bump = Bump::new();
        let mut alloced = vec![];

        for vals in values.iter().cloned() {
            alloced.push(bump.alloc(vals));
        }

        for (vals, alloc) in values.iter().zip(alloced.into_iter()) {
            assert_eq!(vals, alloc);
        }
    }

    fn big_allocations_never_overlap(values: Vec<BigValue>) -> () {
        let bump = Bump::new();
        let mut alloced = vec![];

        for v in values {
            let a = bump.alloc(v);
            let start = a as *const _ as usize;
            let end = unsafe { (a as *const BigValue).offset(1) as usize };
            let range = (start, end);

            for r in &alloced {
                assert!(!overlap(*r, range));
            }

            alloced.push(range);
        }
    }

    fn can_allocate_heterogeneous_things_and_they_dont_overlap(things: Vec<Elems<u8, u64>>) -> () {
        let bump = Bump::new();
        let mut ranges = vec![];

        for t in things {
            let r = match t {
                Elems::OneT(a) => {
                    range(bump.alloc(a))
                },
                Elems::TwoT(a, b) => {
                    range(bump.alloc([a, b]))
                },
                Elems::FourT(a, b, c, d) => {
                    range(bump.alloc([a, b, c, d]))
                },
                Elems::OneU(a) => {
                    range(bump.alloc(a))
                },
                Elems::TwoU(a, b) => {
                    range(bump.alloc([a, b]))
                },
                Elems::FourU(a, b, c, d) => {
                    range(bump.alloc([a, b, c, d]))
                },
            };

            for s in &ranges {
                assert!(!overlap(r, *s));
            }

            ranges.push(r);
        }
    }


    fn test_alignment_chunks(sizes: Vec<usize>) -> () {
        const SUPPORTED_ALIGNMENTS: &[usize] = &[1, 2, 4, 8, 16];
        for &alignment in SUPPORTED_ALIGNMENTS {
            let mut b = Bump::<1>::with_min_align_and_capacity(513);
            let mut sizes = sizes.iter().map(|&size| (size % 10) * alignment).collect::<Vec<_>>();

            for &size in &sizes {
                let layout = std::alloc::Layout::from_size_align(size, alignment).unwrap();
                let ptr = b.alloc_layout(layout).as_ptr() as *const u8 as usize;
                assert_eq!(ptr % alignment, 0);
            }

            for chunk in b.iter_allocated_chunks() {
                let mut remaining = chunk.len();
                while remaining > 0 {
                    let size = sizes.pop().expect("too many bytes in the chunk output");
                    assert!(remaining >= size, "returned chunk contained padding");
                    remaining -= size;
                }
            }
            assert_eq!(sizes.into_iter().sum::<usize>(), 0);
        }
    }

    fn alloc_slices(allocs: Vec<(u8, usize)>) -> () {
        let b = Bump::new();
        let mut allocated: Vec<(usize, usize)> = vec![];
        for (val, len) in allocs {
            let len = len % 100;
            let s = b.alloc_slice_fill_copy(len, val);

            assert_eq!(s.len(), len);
            assert!(s.iter().all(|v| v == &val));

            let range = (s.as_ptr() as usize, unsafe { s.as_ptr().add(s.len()) } as usize);
            for r in &allocated {
                let no_overlap = range.1 <= r.0 || r.1 <= range.0;
                assert!(no_overlap);
            }
            allocated.push(range);
        }
    }

    fn alloc_strs(allocs: Vec<String>) -> () {
        let b = Bump::new();
        let allocated: Vec<&str> = allocs.iter().map(|s| b.alloc_str(s) as &_).collect();
        for (val, alloc) in allocs.into_iter().zip(allocated) {
            assert_eq!(val, alloc);
        }
    }

    fn all_allocations_in_a_chunk(values: Vec<BigValue>) -> () {
        let b = Bump::new();
        let allocated: Vec<&BigValue> = values.into_iter().map(|val| b.alloc(val) as &_).collect();
        let chunks: Vec<(*mut u8, usize)> = unsafe { b.iter_allocated_chunks_raw() }.collect();
        for alloc in allocated.into_iter() {
            assert!(chunks.iter().any(|&(ptr, size)| {
                let ptr = ptr as usize;
                let chunk = (ptr, ptr + size);
                contains(chunk, range(alloc))
            }));
        }
    }

    fn chunks_and_raw_chunks_are_same(values: Vec<BigValue>) -> () {
        let mut b = Bump::new();
        for val in values {
            b.alloc(val);
        }
        let raw_chunks: Vec<(_, _)> = unsafe { b.iter_allocated_chunks_raw() }.collect();
        let chunks: Vec<&[_]> = b.iter_allocated_chunks().collect();
        assert_eq!(raw_chunks.len(), chunks.len());
        for ((ptr, size), chunk) in raw_chunks.into_iter().zip(chunks) {
            assert_eq!(ptr as *const _, chunk.as_ptr() as *const _);
            assert_eq!(size, chunk.len());
        }
    }

    // MIRI exits with failure when we try to allocate more memory than its
    // sandbox has, rather than returning null from the allocation
    // function. This test runs afoul of that bug.
    #[cfg(not(miri))]
    fn limit_is_never_exceeded(limit: usize) -> bool {
        let bump = Bump::new();

        bump.set_allocation_limit(Some(limit));

        // The exact numbers here on how much to allocate are a bit murky but we
        // have two main goals.
        //
        // - Attempt to allocate over the allocation limit imposed
        // - Allocate in increments small enough that at least a few allocations succeed
        let layout = std::alloc::Layout::array::<u8>(limit / 16).unwrap();
        for _ in 0..32 {
            let _ = bump.try_alloc_layout(layout);
        }

        bump.allocated_bytes() <= limit
    }

    fn allocated_bytes_including_metadata(allocs: Vec<usize>) -> () {
        let b = Bump::new();
        let mut slice_bytes = 0;
        let allocs_len = allocs.len();
        for len in allocs {
            const MAX_LEN: usize = 512;
            let len = len % MAX_LEN;
            b.alloc_slice_fill_copy(len, 0);
            slice_bytes += len;
            let allocated_bytes = b.allocated_bytes();
            let allocated_bytes_including_metadata = b.allocated_bytes_including_metadata();
            if slice_bytes == 0 {
                assert_eq!(allocated_bytes, 0);
                assert_eq!(allocated_bytes_including_metadata, 0);
            } else {
                assert!(allocated_bytes >= slice_bytes);
                assert!(allocated_bytes_including_metadata > allocated_bytes);
                assert!(allocated_bytes_including_metadata < allocated_bytes + allocs_len * 100);
            }
        }
    }

    fn allocator_operation_sequences_preserve_chunk_views(program: AllocatorProgram) -> () {
        let mut bump = Bump::new();
        let mut live_allocs = Vec::new();
        let mut current_limit = None;

        for op in &program.0 {
            match op {
                AllocatorOp::Alloc { value, len } => {
                    let len = usize::from(*len) % MAX_ALLOCATOR_SLICE_LEN;
                    if let Ok(slice) = bump.try_alloc_slice_fill_copy(len, *value) {
                        assert_eq!(slice, vec![*value; len].as_slice());
                        live_allocs.push(vec![*value; len]);
                    }
                }
                AllocatorOp::Reset => {
                    bump.reset();
                    live_allocs.clear();
                    assert_eq!(bump.allocation_limit(), current_limit);
                }
                AllocatorOp::SetLimit(limit) => {
                    let limit = limit.map(|limit| usize::from(limit) % MAX_ALLOCATOR_LIMIT);
                    bump.set_allocation_limit(limit);
                    current_limit = limit;
                    assert_eq!(bump.allocation_limit(), current_limit);
                }
            }

            assert_chunk_views_match(&mut bump, &live_allocs);
        }
    }

    fn layout_allocation_sequences_preserve_alignment_and_containment(program: LayoutAllocatorProgram) -> () {
        let mut bump = Bump::new();
        let mut live_allocs = Vec::new();
        let mut current_limit = None;

        for op in &program.0 {
            match *op {
                LayoutAllocatorOp::Alloc { size, align_log2 } => {
                    let layout = arbitrary_small_layout(size, align_log2);
                    if let Ok(ptr) = bump.try_alloc_layout(layout) {
                        assert_eq!(ptr.as_ptr() as usize % layout.align(), 0);
                        let range = (ptr.as_ptr() as usize, ptr.as_ptr() as usize + layout.size());
                        for other in &live_allocs {
                            assert!(!overlap(range, *other));
                        }
                        live_allocs.push(range);
                    }
                }
                LayoutAllocatorOp::Reset => {
                    bump.reset();
                    live_allocs.clear();
                    assert_eq!(bump.allocation_limit(), current_limit);
                }
                LayoutAllocatorOp::SetLimit(limit) => {
                    let limit = limit.map(|limit| usize::from(limit) % MAX_ALLOCATOR_LIMIT);
                    bump.set_allocation_limit(limit);
                    current_limit = limit;
                    assert_eq!(bump.allocation_limit(), current_limit);
                }
            }

            assert_layout_allocations_match(&mut bump, &live_allocs);
        }
    }

    #[cfg(feature = "collections")]
    fn extending_from_slice(data1: Vec<usize>, data2: Vec<usize>) -> () {
        let bump = Bump::new();

        // Create a bumpalo Vec with the contents of `data1`
        let mut vec = bumpalo::collections::Vec::new_in(&bump);
        vec.extend_from_slice_copy(&data1);
        assert_eq!(vec.as_slice(), data1);

        // Extend the Vec using the contents of `data2`
        vec.extend_from_slice_copy(&data2);
        // Confirm that the Vec now has the expected number of items
        assert_eq!(vec.len(), data1.len() + data2.len());
        // Confirm that the beginning of the Vec matches `data1`'s elements
        assert_eq!(&vec[0..data1.len()], data1);
        // Confirm that the end of the Vec matches `data2`'s elements
        assert_eq!(&vec[data1.len()..], data2);
    }

    #[cfg(feature = "collections")]
    fn extending_from_slices(data: Vec<Vec<usize>>) -> () {
        let bump = Bump::new();

        // Convert the Vec<Vec<usize>> into a &[&[usize]]
        let slices_vec: Vec<&[usize]> = data.iter().map(Vec::as_slice).collect();
        let slices = slices_vec.as_slice();

        // Isolate the first slice from the remaining slices. If `slices` is empty,
        // fall back to empty slices for both.
        let (first_slice, remaining_slices) = match slices {
            [head, tail @ ..] => (*head, tail),
            [] => (&[][..], &[][..])
        };

        // Create a bumpalo `Vec` and populate it with the contents of the first slice.
        let mut vec = bumpalo::collections::Vec::new_in(&bump);
        vec.extend_from_slice_copy(first_slice);
        assert_eq!(vec.as_slice(), first_slice);

        // Append all of the other slices onto the end of the Vec
        vec.extend_from_slices_copy(remaining_slices);

        let total_length: usize = slices.iter().map(|s| s.len()).sum();
        assert_eq!(vec.len(), total_length);

        let total_data: Vec<usize> = slices.iter().flat_map(|s| s.iter().copied()).collect();
        assert_eq!(vec.as_slice(), total_data.as_slice());
    }

    #[cfg(feature = "collections")]
    fn compare_extending_from_slice_and_from_slices(data: Vec<Vec<usize>>) -> () {
        let bump = Bump::new();

        // Convert the Vec<Vec<usize>> into a &[&[usize]]
        let slices_vec: Vec<&[usize]> = data.iter().map(Vec::as_slice).collect();
        let slices = slices_vec.as_slice();

        // Isolate the first slice from the remaining slices. If `slices` is empty,
        // fall back to empty slices for both.
        let (first_slice, remaining_slices) = match slices {
            [head, tail @ ..] => (*head, tail),
            [] => (&[][..], &[][..])
        };

        // Create a bumpalo `Vec` and populate it with the contents of the first slice.
        let mut vec1 = bumpalo::collections::Vec::new_in(&bump);
        vec1.extend_from_slice_copy(first_slice);
        assert_eq!(vec1.as_slice(), first_slice);

        // Append each remaining slice individually
        for slice in remaining_slices {
            vec1.extend_from_slice_copy(slice);
        }

        // Create a second Vec populated with the contents of the first slice.
        let mut vec2 = bumpalo::collections::Vec::new_in(&bump);
        vec2.extend_from_slice_copy(first_slice);
        assert_eq!(vec2.as_slice(), first_slice);

        // Append the remaining slices en masse
        vec2.extend_from_slices_copy(remaining_slices);

        // Confirm that the two approaches to extending a Vec resulted in the same data
        assert_eq!(vec1, vec2);
    }

    #[cfg(feature = "allocator_api")]
    fn stress_raw_checkpoints(ops: Vec<RawCheckpointOp>) -> () {
        eprintln!("======================================================================================");
        use bumpalo::RawCheckpoint;

        const MAX_OPS: usize = 100;

        let bump = Bump::new();
        let alloc = &bump;

        // The set of live allocations, as well as their expected fill byte and
        // layout. Can be set to `None` to mark an allocation as no-longer live.
        let mut live_allocs: Vec<Option<(u8, NonNull<u8>, Layout)>> = Vec::new();

        // The raw checkpoints we've made as well as the index that
        // `live_allocs` needs to be truncated to upon checkpoint reset to
        // remove allocations that aren't live anymore after the reset.
        let mut checkpoints: Vec<(usize, RawCheckpoint)> = Vec::new();

        for op in ops.into_iter().take(MAX_OPS) {
            eprintln!("evaluating op: {op:?}");

            match op {
                RawCheckpointOp::TakeCheckpoint => {
                    checkpoints.push((live_allocs.len(), bump.raw_checkpoint()));
                }
                RawCheckpointOp::ResetToCheckpoint => {
                    if let Some((n, checkpoint)) = checkpoints.pop() {
                        unsafe {
                            bump.reset_to_raw_checkpoint(checkpoint);
                        }
                        live_allocs.truncate(n);
                    }
                }
                RawCheckpointOp::DropCheckpoint => {
                    checkpoints.pop();
                }
                RawCheckpointOp::Alloc(byte, size) => {
                    let layout = Layout::from_size_align(size, 1).unwrap();
                    if let Ok(ptr) = alloc.allocate(layout) {
                        let ptr = ptr.cast::<u8>();
                        unsafe {
                            std::ptr::write_bytes(ptr.as_ptr(), byte, size);
                        }
                        live_allocs.push(Some((byte, ptr, layout)));
                    }
                }
                RawCheckpointOp::Realloc(byte, new_size) => {
                    if let Some(&Some((_, ptr, old_layout))) = live_allocs.last() {
                        if new_size == 0 {
                            continue;
                        }
                        let new_layout =
                            Layout::from_size_align(new_size, old_layout.align()).unwrap();
                        let result = if new_size <= old_layout.size() {
                            unsafe { alloc.shrink(ptr, old_layout, new_layout) }
                        } else {
                            unsafe { alloc.grow(ptr, old_layout, new_layout) }
                        };
                        if let Ok(new_ptr) = result {
                            let new_ptr = new_ptr.cast::<u8>();
                            unsafe {
                                std::ptr::write_bytes(new_ptr.as_ptr(), byte, new_size);
                            }
                            // Model reallocations as freeing the old pointer
                            // and allocating a new one, as that is what they do
                            // in the limit.
                            *live_allocs.last_mut().unwrap() = None;
                            live_allocs.push(Some((byte, new_ptr, new_layout)));
                        }
                    }
                }
                RawCheckpointOp::Dealloc => {
                    if let Some(&Some((_, ptr, layout))) = live_allocs.last() {
                        *live_allocs.last_mut().unwrap() = None;
                        unsafe {
                            alloc.deallocate(ptr, layout);
                        }
                    }
                }
            }

            // Verify that all of the live allocations have their expected
            // values (and weren't overwritten by any bogus allocation or heap
            // corruption).
            for &entry in &live_allocs {
                if let Some((byte, ptr, layout)) = entry {
                    let slice =
                        unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size()) };
                    for (i, &b) in slice.iter().enumerate() {
                        assert_eq!(
                            b, byte,
                            "byte mismatch at offset {i} in allocation of size {}",
                            layout.size()
                        );
                    }
                }
            }
        }
    }
}
