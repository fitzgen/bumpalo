use crate::quickcheck;
use ::quickcheck::{Arbitrary, Gen};
use bumpalo::Bump;
use std::mem;

// =============================================================================
// EmplaceSlice operation-sequence testing
// =============================================================================

const MAX_EMPLACE_OPS: usize = 64;
const MAX_EMPLACE_EXTEND_LEN: usize = 32;
const MAX_EMPLACE_RESERVE: usize = 64;

#[derive(Clone, Debug)]
enum EmplaceSliceOp {
    Push(u8),
    Extend(Vec<u8>),
    Reserve(usize),
    ShrinkToFit,
}

impl Arbitrary for EmplaceSliceOp {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 4 {
            0 => EmplaceSliceOp::Push(u8::arbitrary(g)),
            1 => {
                let mut v = Vec::<u8>::arbitrary(g);
                v.truncate(MAX_EMPLACE_EXTEND_LEN);
                EmplaceSliceOp::Extend(v)
            }
            2 => EmplaceSliceOp::Reserve(usize::from(u8::arbitrary(g)) % MAX_EMPLACE_RESERVE),
            3 => EmplaceSliceOp::ShrinkToFit,
            _ => unreachable!(),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            EmplaceSliceOp::Push(v) => Box::new(v.shrink().map(EmplaceSliceOp::Push)),
            EmplaceSliceOp::Extend(v) => Box::new(v.shrink().map(EmplaceSliceOp::Extend)),
            EmplaceSliceOp::Reserve(n) => Box::new(n.shrink().map(EmplaceSliceOp::Reserve)),
            EmplaceSliceOp::ShrinkToFit => Box::new(std::iter::empty()),
        }
    }
}

#[derive(Clone, Debug)]
struct EmplaceSliceProgram(Vec<EmplaceSliceOp>);

impl Arbitrary for EmplaceSliceProgram {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut ops = Vec::<EmplaceSliceOp>::arbitrary(g);
        ops.truncate(MAX_EMPLACE_OPS);
        EmplaceSliceProgram(ops)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(|mut ops| {
            ops.truncate(MAX_EMPLACE_OPS);
            EmplaceSliceProgram(ops)
        }))
    }
}

fn range_of<T>(p: *const T, len: usize) -> (usize, usize) {
    let start = p as usize;
    let end = start + mem::size_of::<T>() * len;
    (start, end)
}

fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

// =============================================================================
// Quickcheck tests
// =============================================================================

quickcheck! {
    fn emplace_single_roundtrip(values: Vec<u64>) -> () {
        let bump = Bump::new();
        let mut ranges = vec![];

        for v in &values {
            let r = bump.emplace().write(*v);
            assert_eq!(*r, *v);
            let rng = range_of(r as *const u64, 1);
            assert_eq!(r as *const u64 as usize % mem::align_of::<u64>(), 0);
            for prev in &ranges {
                assert!(!ranges_overlap(rng, *prev));
            }
            ranges.push(rng);
        }
    }

    fn try_emplace_single_roundtrip(values: Vec<u32>) -> () {
        let bump = Bump::new();
        for v in &values {
            let r = bump.try_emplace().unwrap().write(*v);
            assert_eq!(*r, *v);
            assert_eq!(r as *const u32 as usize % mem::align_of::<u32>(), 0);
        }
    }

    fn emplace_slice_operation_sequences(program: EmplaceSliceProgram) -> () {
        let bump = Bump::new();
        let mut place = bump.emplace_slice::<u8>();
        let mut model: Vec<u8> = Vec::new();

        for op in &program.0 {
            match op {
                EmplaceSliceOp::Push(val) => {
                    place.push(*val);
                    model.push(*val);
                }
                EmplaceSliceOp::Extend(vals) => {
                    place.extend(vals.iter().copied());
                    model.extend(vals.iter().copied());
                }
                EmplaceSliceOp::Reserve(n) => {
                    place.reserve(*n);
                    assert!(place.capacity() >= place.len() + n);
                }
                EmplaceSliceOp::ShrinkToFit => {
                    place.shrink_to_fit();
                }
            }

            assert_eq!(place.len(), model.len());
            assert!(place.capacity() >= place.len());
            assert_eq!(place.as_slice(), model.as_slice());
        }

        let result = place.into_mut_slice();
        assert_eq!(result, model.as_slice());
    }

    fn emplace_slice_copy(data: Vec<u8>) -> () {
        let bump = Bump::new();
        let place = bump.emplace_slice_with_capacity(data.len());
        let result = place.copy_slice(&data);
        assert_eq!(result, data.as_slice());
    }

    fn emplace_slice_clone(data: Vec<u8>) -> () {
        let bump = Bump::new();
        let place = bump.emplace_slice_with_capacity(data.len());
        let result = place.clone_slice(&data);
        assert_eq!(result, data.as_slice());
    }

    fn emplace_slice_write_iter(data: Vec<u8>) -> () {
        let bump = Bump::new();
        let place = bump.emplace_slice::<u8>();
        let result = place.write_iter(data.iter().copied());
        assert_eq!(result, data.as_slice());
    }

    fn emplace_str_roundtrip(strings: Vec<String>) -> () {
        let bump = Bump::new();
        for s in &strings {
            let r = bump.emplace_str_with_capacity(s.len()).write_str(s);
            assert_eq!(r, s.as_str());
        }
    }

    fn emplace_str_push_chars(s: String) -> () {
        let bump = Bump::new();
        let mut place = bump.emplace_str();
        for c in s.chars() {
            place.push(c);
        }
        let result = place.into_str();
        assert_eq!(result, s.as_str());
    }

    fn emplace_str_push_strs(parts: Vec<String>) -> () {
        let bump = Bump::new();
        let mut place = bump.emplace_str();
        let mut model = String::new();
        for part in &parts {
            place.push_str(part);
            model.push_str(part);
        }
        let result = place.into_str();
        assert_eq!(result, model.as_str());
    }

    fn heterogeneous_emplace_no_overlap(ops: Vec<u8>) -> () {
        let bump = Bump::new();
        let mut ranges: Vec<(usize, usize)> = vec![];

        for (i, op) in ops.iter().enumerate() {
            let rng = match op % 4 {
                0 => {
                    let r = bump.emplace().write(i as u64);
                    range_of(r as *const u64, 1)
                }
                1 => {
                    let r = bump.alloc(i as u32);
                    range_of(r as *const u32, 1)
                }
                2 => {
                    let data = [i as u8; 4];
                    let r = bump.emplace_slice_with_capacity(4).copy_slice(&data);
                    range_of(r.as_ptr(), r.len())
                }
                3 => {
                    let s = bump.emplace_str().write_str("hi");
                    range_of(s.as_ptr(), s.len())
                }
                _ => unreachable!(),
            };
            if rng.0 != rng.1 {
                for prev in &ranges {
                    assert!(
                        !ranges_overlap(rng, *prev),
                        "overlap: {:?} vs {:?}",
                        rng,
                        prev
                    );
                }
                ranges.push(rng);
            }
        }
    }

    fn emplace_rewind_on_drop(values: Vec<u64>) -> () {
        let bump = Bump::new();
        for v in &values {
            let capacity_before = bump.chunk_capacity();
            {
                let _place = bump.emplace::<u64>();
                // Drop without writing -- should rewind and reclaim the space.
            }
            let capacity_after = bump.chunk_capacity();
            // If the emplace stayed in the same chunk, capacity should be
            // fully restored.  If it triggered a new chunk, capacity_after
            // will be the full new-chunk size which is still >= capacity_before.
            assert!(
                capacity_after >= capacity_before,
                "capacity should be at least restored on drop: before={}, after={}",
                capacity_before, capacity_after
            );

            // Verify we can still allocate correctly after the rewind.
            let r = bump.alloc(*v);
            assert_eq!(*r, *v);
        }
    }

    fn emplace_slice_rewind_on_drop(lens: Vec<u8>) -> () {
        let bump = Bump::new();
        for len in &lens {
            let len = (*len as usize) % 16;
            if len == 0 {
                continue;
            }
            let capacity_before = bump.chunk_capacity();
            {
                let place = bump.emplace_slice_with_capacity::<u8>(len);
                drop(place);
            }
            let capacity_after = bump.chunk_capacity();
            assert!(
                capacity_after >= capacity_before,
                "capacity should be reclaimed on drop: before={}, after={}",
                capacity_before,
                capacity_after
            );
        }
    }

    fn zoo_equivalence_alloc(values: Vec<u64>) -> () {
        let bump1 = Bump::new();
        let bump2 = Bump::new();
        for v in &values {
            let r1 = bump1.alloc(*v);
            let r2 = bump2.emplace().write(*v);
            assert_eq!(*r1, *r2);
        }
    }

    fn zoo_equivalence_slice_copy(slices: Vec<Vec<u8>>) -> () {
        let bump1 = Bump::new();
        let bump2 = Bump::new();
        for s in &slices {
            let r1 = bump1.alloc_slice_copy(s);
            let r2 = bump2.emplace_slice_with_capacity(s.len()).copy_slice(s);
            assert_eq!(r1, r2);
        }
    }

    fn zoo_equivalence_str(strings: Vec<String>) -> () {
        let bump1 = Bump::new();
        let bump2 = Bump::new();
        for s in &strings {
            let r1 = bump1.alloc_str(s);
            let r2 = bump2.emplace_str_with_capacity(s.len()).write_str(s);
            assert_eq!(r1, r2);
        }
    }

    fn alignment_single_values(values: Vec<u64>) -> () {
        macro_rules! test_align {
            ($align:literal) => {{
                let bump = bumpalo::Bump::<$align>::with_min_align();
                for v in &values {
                    let r = bump.emplace().write(*v);
                    let ptr = r as *const u64 as usize;
                    assert_eq!(ptr % mem::align_of::<u64>(), 0);
                    assert_eq!(ptr % $align, 0);
                    assert_eq!(*r, *v);
                }
            }};
        }
        test_align!(1);
        test_align!(2);
        test_align!(4);
        test_align!(8);
        test_align!(16);
    }

    fn alignment_slices(data: Vec<Vec<u32>>) -> () {
        macro_rules! test_align {
            ($align:literal) => {{
                let bump = bumpalo::Bump::<$align>::with_min_align();
                for slice in &data {
                    let r = bump.emplace_slice_with_capacity(slice.len()).copy_slice(slice);
                    let ptr = r.as_ptr() as usize;
                    assert_eq!(ptr % mem::align_of::<u32>(), 0);
                    assert_eq!(ptr % $align, 0);
                    assert_eq!(r, slice.as_slice());
                }
            }};
        }
        test_align!(1);
        test_align!(2);
        test_align!(4);
        test_align!(8);
        test_align!(16);
    }

    fn allocation_limits_respected(data: Vec<Vec<u8>>) -> () {
        let bump = Bump::new();
        bump.set_allocation_limit(Some(256));

        for slice in &data {
            let slice = &slice[..slice.len().min(64)];
            match bump.try_emplace_slice_with_capacity::<u8>(slice.len()) {
                Ok(place) => {
                    match place.try_copy_slice(slice) {
                        Ok(r) => assert_eq!(r, slice),
                        Err(_) => {} // allocation limit hit during copy
                    }
                }
                Err(_) => {} // allocation limit hit
            }
        }
    }

    fn extend_from_slice_copy(data1: Vec<u8>, data2: Vec<u8>) -> () {
        let bump = Bump::new();
        let mut place = bump.emplace_slice_with_capacity::<u8>(data1.len());
        place.extend_from_slice_copy(&data1);
        assert_eq!(place.as_slice(), data1.as_slice());

        place.extend_from_slice_copy(&data2);
        assert_eq!(place.len(), data1.len() + data2.len());
        assert_eq!(&place.as_slice()[..data1.len()], data1.as_slice());
        assert_eq!(&place.as_slice()[data1.len()..], data2.as_slice());
        let _ = place.into_mut_slice();
    }

    fn extend_from_slice_clone(data1: Vec<u8>, data2: Vec<u8>) -> () {
        let bump = Bump::new();
        let mut place = bump.emplace_slice_with_capacity::<u8>(data1.len());
        place.extend_from_slice_clone(&data1);
        assert_eq!(place.as_slice(), data1.as_slice());

        place.extend_from_slice_clone(&data2);
        assert_eq!(place.len(), data1.len() + data2.len());
        assert_eq!(&place.as_slice()[..data1.len()], data1.as_slice());
        assert_eq!(&place.as_slice()[data1.len()..], data2.as_slice());
        let _ = place.into_mut_slice();
    }
}
