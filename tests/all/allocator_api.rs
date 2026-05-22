#![cfg(feature = "allocator_api")]

use crate::quickcheck;
use bumpalo::Bump;
use std::alloc::{AllocError, Allocator, Layout};
use std::cmp;
use std::ptr;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

const MAX_SEQUENCE_OPS: usize = 64;
const MAX_SEQUENCE_STEP: usize = 128;

/// Map an arbitrary `x` to a power of 2 that is less than or equal to `max`,
/// but with as little bias as possible (eg rounding `min(x, max)` to the
/// nearest power of 2 is unacceptable because it would majorly bias `max` for
/// small values of `max`).
fn clamp_to_pow2_in_range(x: usize, max: usize) -> usize {
    let log_x = max.ilog2() as usize;
    if log_x == 0 {
        return 1;
    }
    let divisor = usize::MAX / log_x;
    let y = 1_usize << (x / divisor);
    assert!(y.is_power_of_two(), "{y} is not a power of two");
    assert!(y <= max, "{y} is larger than {max}");
    y
}

/// Helper to turn a pair of arbitrary `usize`s into a valid `Layout` of
/// reasonable size for use with quickchecks.
pub fn arbitrary_layout(size: usize, align: usize) -> Layout {
    const MAX_ALIGN: usize = 64;
    const MAX_SIZE: usize = 1024;

    let align = clamp_to_pow2_in_range(align, MAX_ALIGN);

    let size = size % (MAX_SIZE + 1);
    let size = size.next_multiple_of(align);

    Layout::from_size_align(size, align).unwrap()
}

fn layout_with_size_align(size: usize, align: usize) -> Layout {
    const MAX_ALIGN: usize = 64;
    const MAX_SIZE: usize = 1024;

    let align = clamp_to_pow2_in_range(align, MAX_ALIGN);
    let size = size.min(MAX_SIZE);
    let size = size.next_multiple_of(align);

    Layout::from_size_align(size.min(MAX_SIZE), align).unwrap()
}

fn shrink_layout_within(old_layout: Layout, size: usize, align: usize) -> Layout {
    let align = clamp_to_pow2_in_range(align, 64);
    let size = size.min(old_layout.size());
    let size = size - (size % align);
    Layout::from_size_align(size, align).unwrap()
}

fn growth_layout_from(old_layout: Layout, delta: usize, align: usize) -> Option<Layout> {
    const MAX_SIZE: usize = 1024;

    if old_layout.size() >= MAX_SIZE {
        return None;
    }

    let requested = old_layout.size() + (delta % MAX_SEQUENCE_STEP) + 1;
    Some(layout_with_size_align(requested, align))
}

#[derive(Clone, Debug)]
enum AllocatorSequenceOp {
    Grow { delta: usize, align: usize },
    GrowZeroed { delta: usize, align: usize },
    Shrink { target_size: usize, align: usize },
}

impl ::quickcheck::Arbitrary for AllocatorSequenceOp {
    fn arbitrary(g: &mut ::quickcheck::Gen) -> Self {
        match usize::arbitrary(g) % 3 {
            0 => AllocatorSequenceOp::Grow {
                delta: usize::arbitrary(g),
                align: usize::arbitrary(g),
            },
            1 => AllocatorSequenceOp::GrowZeroed {
                delta: usize::arbitrary(g),
                align: usize::arbitrary(g),
            },
            _ => AllocatorSequenceOp::Shrink {
                target_size: usize::arbitrary(g),
                align: usize::arbitrary(g),
            },
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(std::iter::empty())
    }
}

#[derive(Clone, Debug)]
struct AllocatorSequence {
    initial_size: usize,
    initial_align: usize,
    ops: Vec<AllocatorSequenceOp>,
}

impl ::quickcheck::Arbitrary for AllocatorSequence {
    fn arbitrary(g: &mut ::quickcheck::Gen) -> Self {
        let mut ops = Vec::<AllocatorSequenceOp>::arbitrary(g);
        ops.truncate(MAX_SEQUENCE_OPS);
        AllocatorSequence {
            initial_size: usize::arbitrary(g),
            initial_align: usize::arbitrary(g),
            ops,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let initial_size = self.initial_size;
        let initial_align = self.initial_align;
        Box::new(self.ops.shrink().map(move |mut ops| {
            ops.truncate(MAX_SEQUENCE_OPS);
            AllocatorSequence {
                initial_size,
                initial_align,
                ops,
            }
        }))
    }
}

fn bytes_for_step(step: usize, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| step.wrapping_mul(37).wrapping_add(i) as u8)
        .collect()
}

fn write_prefix(ptr: NonNull<[u8]>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }

    // Safety: `ptr` points to an allocation of at least `bytes.len()` bytes,
    // and the source slice is valid for reads of exactly that many bytes.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.as_ptr().cast::<u8>(), bytes.len());
    }
}

fn read_prefix(ptr: NonNull<[u8]>, len: usize) -> Vec<u8> {
    if len == 0 {
        return Vec::new();
    }

    // Safety: Callers only request prefixes that are within the allocation's
    // length, so this produces a valid shared slice over initialized bytes.
    unsafe { std::slice::from_raw_parts(ptr.as_ptr().cast::<u8>(), len) }.to_vec()
}

#[derive(Debug)]
struct AllocatorDebug {
    bump: Bump,
    grows: AtomicUsize,
    shrinks: AtomicUsize,
    allocs: AtomicUsize,
    deallocs: AtomicUsize,
}

impl AllocatorDebug {
    fn new(bump: Bump) -> AllocatorDebug {
        AllocatorDebug {
            bump,
            grows: AtomicUsize::new(0),
            shrinks: AtomicUsize::new(0),
            allocs: AtomicUsize::new(0),
            deallocs: AtomicUsize::new(0),
        }
    }
}

unsafe impl Allocator for AllocatorDebug {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocs.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocs.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.deallocate(ptr, layout)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.shrinks.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.shrink(ptr, old_layout, new_layout)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.grows.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.grow(ptr, old_layout, new_layout)
    }
}

#[test]
fn allocator_api_push_a_bunch_of_items() {
    let b = AllocatorDebug::new(Bump::new());
    let mut v = Vec::with_capacity_in(1024, &b);
    assert_eq!(b.allocs.load(Relaxed), 1);

    for x in 0..1024 {
        v.push(x);
    }

    // Ensure we trigger a grow
    assert_eq!(b.grows.load(Relaxed), 0);
    for x in 1024..2048 {
        v.push(x);
    }
    assert_ne!(b.grows.load(Relaxed), 0);

    // Ensure we trigger a shrink
    v.truncate(1024);
    v.shrink_to_fit();
    assert_eq!(b.shrinks.load(Relaxed), 1);

    // Ensure we trigger a deallocation
    assert_eq!(b.deallocs.load(Relaxed), 0);
    drop(v);
    assert_eq!(b.deallocs.load(Relaxed), 1);
}

#[test]
fn allocator_grow_zeroed() {
    // Create a new bump arena.
    let ref bump = Bump::new();

    // Make an initial allocation.
    let first_layout = Layout::from_size_align(4, 4).expect("create a layout");
    let mut p = bump
        .allocate_zeroed(first_layout)
        .expect("allocate a first chunk");
    let allocated = bump.allocated_bytes();
    unsafe { p.as_mut().fill(42) };
    let p = p.cast();

    // Grow the last allocation. This should just reserve a few more bytes
    // within the current chunk, not allocate a whole new memory block within a
    // new chunk.
    let second_layout = Layout::from_size_align(8, 4).expect("create a expanded layout");
    let p = unsafe { bump.grow_zeroed(p, first_layout, second_layout) }
        .expect("should grow_zeroed okay");
    assert!(bump.allocated_bytes() <= allocated * 2);
    assert_eq!(unsafe { p.as_ref() }, [42, 42, 42, 42, 0, 0, 0, 0]);
}

quickcheck! {
    fn allocator_grow_align_increase(layouts: Vec<(usize, usize)>) -> bool {
        let mut layouts: Vec<_> = layouts.into_iter().map(|(size, align)| {
            arbitrary_layout(size, align)
        }).collect();

        layouts.sort_by_key(|l| (l.size(), l.align()));

        let b = AllocatorDebug::new(Bump::new());
        let mut layout_iter = layouts.into_iter();

        if let Some(initial_layout) = layout_iter.next() {
            let mut pointer = b.allocate(initial_layout).unwrap();
            if !is_pointer_aligned_to(pointer, initial_layout.align()) {
                return false;
            }

            let mut old_layout = initial_layout;

            for new_layout in layout_iter {
                pointer = unsafe { b.grow(pointer.cast(), old_layout, new_layout).unwrap() };
                if !is_pointer_aligned_to(pointer, new_layout.align()) {
                    return false;
                }

                old_layout = new_layout;
            }
        }

        true
    }

    fn allocator_shrink_align_change(layouts: Vec<(usize, usize)>) -> () {
        let mut layouts: Vec<_> = layouts.into_iter().map(|(size, align)| {
            arbitrary_layout(size, align)
        }).collect();

        layouts.sort_by_key(|l| l.size());
        layouts.reverse();

        let b = AllocatorDebug::new(Bump::new());
        let mut layout_iter = layouts.into_iter();

        if let Some(initial_layout) = layout_iter.next() {
            let mut pointer = b.allocate(initial_layout).unwrap();
            assert!(is_pointer_aligned_to(pointer, initial_layout.align()));

            let mut old_layout = initial_layout;

            for new_layout in layout_iter {
                let res = unsafe { b.shrink(pointer.cast(), old_layout, new_layout) };
                if old_layout.align() < new_layout.align() {
                    match res {
                        Ok(p) => assert!(is_pointer_aligned_to(p, new_layout.align())),
                        Err(_) => {}
                    }
                } else {
                    pointer = res.unwrap();
                    assert!(is_pointer_aligned_to(pointer, new_layout.align()));

                    old_layout = new_layout;
                }
            }
        }
    }

    fn allocator_grow_or_shrink(layouts: Vec<((usize, usize), (usize, usize))>) -> () {
        let layouts = layouts
            .into_iter()
            .map(|((from_size, from_align), (to_size, to_align))| {
                let from_layout = arbitrary_layout(from_size, from_align);
                let to_layout = arbitrary_layout(to_size, to_align);
                (from_layout, to_layout)
            });

        let b = AllocatorDebug::new(Bump::new());
        for (from_layout, to_layout) in layouts {
            let pointer = b.allocate(from_layout).unwrap();
            assert!(is_pointer_aligned_to(pointer, from_layout.align()));
            let pointer = pointer.cast::<u8>();

            let result = if to_layout.size() <= from_layout.size() {
                unsafe { b.shrink(pointer, from_layout, to_layout) }
            } else {
                unsafe { b.grow(pointer, from_layout, to_layout) }
            };

            match result {
                Ok(new_pointer) => {
                    assert!(is_pointer_aligned_to(new_pointer, to_layout.align()));
                }
                // Bumpalo can return allocation errors in various situations,
                // for example if we try to shrink an allocation but also grow
                // its alignment in such a way that we cannot satisfy the
                // requested alignment, and that is okay.
                Err(_) => continue,
            }
        }
    }

    fn allocator_transition_sequences_preserve_bytes(program: AllocatorSequence) -> () {
        let b = AllocatorDebug::new(Bump::new());
        let mut layout = layout_with_size_align(program.initial_size.max(1), program.initial_align);
        let mut pointer = b.allocate(layout).unwrap();
        let mut model = bytes_for_step(0, layout.size());
        write_prefix(pointer, &model);
        assert!(is_pointer_aligned_to(pointer, layout.align()));

        for (step, op) in program.ops.iter().enumerate() {
            match *op {
                AllocatorSequenceOp::Grow { delta, align } => {
                    let Some(new_layout) = growth_layout_from(layout, delta, align) else {
                        continue;
                    };
                    // Safety: `pointer` is the currently live allocation for `layout`,
                    // and `new_layout` is chosen so its size is at least `layout.size()`.
                    let new_pointer = unsafe { b.grow(pointer.cast(), layout, new_layout) }.unwrap();
                    assert!(is_pointer_aligned_to(new_pointer, new_layout.align()));
                    assert_eq!(read_prefix(new_pointer, model.len()), model);

                    layout = new_layout;
                    pointer = new_pointer;
                    model = bytes_for_step(step + 1, layout.size());
                    write_prefix(pointer, &model);
                }
                AllocatorSequenceOp::GrowZeroed { delta, align } => {
                    let Some(new_layout) = growth_layout_from(layout, delta, align) else {
                        continue;
                    };
                    let old_len = model.len();
                    // Safety: `pointer` is the currently live allocation for `layout`,
                    // and `new_layout` is chosen so its size is at least `layout.size()`.
                    let new_pointer = unsafe { b.grow_zeroed(pointer.cast(), layout, new_layout) }.unwrap();
                    assert!(is_pointer_aligned_to(new_pointer, new_layout.align()));
                    let new_contents = read_prefix(new_pointer, new_layout.size());
                    assert_eq!(&new_contents[..old_len], model.as_slice());
                    assert_eq!(&new_contents[old_len..], vec![0; new_layout.size() - old_len].as_slice());

                    layout = new_layout;
                    pointer = new_pointer;
                    model = bytes_for_step(step + 1, layout.size());
                    write_prefix(pointer, &model);
                }
                AllocatorSequenceOp::Shrink { target_size, align } => {
                    let new_layout = shrink_layout_within(layout, target_size, align);
                    let preserved_len = cmp::min(model.len(), new_layout.size());
                    let expected_prefix = model[..preserved_len].to_vec();
                    // Safety: `pointer` is the currently live allocation for `layout`,
                    // and `new_layout` is constructed so its size is at most `layout.size()`.
                    let new_pointer = unsafe { b.shrink(pointer.cast(), layout, new_layout) }.unwrap();
                    assert!(is_pointer_aligned_to(new_pointer, new_layout.align()));
                    assert_eq!(read_prefix(new_pointer, preserved_len), expected_prefix);

                    layout = new_layout;
                    pointer = new_pointer;
                    model = bytes_for_step(step + 1, layout.size());
                    write_prefix(pointer, &model);
                }
            }
        }
    }
}

#[test]
fn allocator_shrink_layout_change() {
    let b = AllocatorDebug::new(Bump::with_capacity(1024));

    let layout_align4 = Layout::from_size_align(1024, 4).unwrap();
    let layout_align16 = Layout::from_size_align(256, 16).unwrap();

    // Allocate a chunk of memory and attempt to shrink it while increasing
    // alignment requirements.
    let p4: NonNull<u8> = b.allocate(layout_align4).unwrap().cast();
    let p16_res = unsafe { b.shrink(p4, layout_align4, layout_align16) };

    // This could either happen to succeed because `p4` already happened to be
    // 16-aligned and could be reused, or `bumpalo` could return an error.
    match p16_res {
        Ok(p16) => assert!(is_pointer_aligned_to(p16, 16)),
        Err(_) => {}
    }
}

fn is_pointer_aligned_to(p: NonNull<[u8]>, align: usize) -> bool {
    debug_assert!(align.is_power_of_two());

    let pointer = p.as_ptr() as *mut u8 as usize;
    let pointer_aligned = pointer & !(align - 1);

    pointer == pointer_aligned
}
