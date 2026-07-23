use crate::check::check;
use bumpalo::Bump;
use mutatis::check::CheckResult;
use mutatis::Mutate;
use std::mem;

const MAX_EMPLACE_OPS: usize = 64;
const MAX_EMPLACE_EXTEND_LEN: usize = 32;
const MAX_EMPLACE_RESERVE: usize = 64;

#[derive(Clone, Debug, Mutate)]
enum EmplaceSliceOp {
    Push(u8),
    Extend(Vec<u8>),
    Reserve(usize),
    ShrinkToFit,
}

#[derive(Clone, Debug, Default, Mutate)]
struct EmplaceSliceProgram(Vec<EmplaceSliceOp>);

fn range_of<T>(p: *const T, len: usize) -> (usize, usize) {
    let start = p as usize;
    let end = start + mem::size_of::<T>() * len;
    (start, end)
}

fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

#[test]
fn emplace_single_roundtrip() -> CheckResult<Vec<u64>> {
    check().run(|values: &Vec<u64>| -> Result<(), String> {
        let bump = Bump::new();
        let mut ranges = vec![];

        for v in values {
            let r = bump.emplace().write(*v);
            assert_eq!(*r, *v);
            let rng = range_of(r as *const u64, 1);
            assert_eq!(r as *const u64 as usize % mem::align_of::<u64>(), 0);
            for prev in &ranges {
                assert!(!ranges_overlap(rng, *prev));
            }
            ranges.push(rng);
        }
        Ok(())
    })
}

#[test]
fn try_emplace_single_roundtrip() -> CheckResult<Vec<u32>> {
    check().run(|values: &Vec<u32>| -> Result<(), String> {
        let bump = Bump::new();
        for v in values {
            let r = bump.try_emplace().unwrap().write(*v);
            assert_eq!(*r, *v);
            assert_eq!(r as *const u32 as usize % mem::align_of::<u32>(), 0);
        }
        Ok(())
    })
}

#[test]
fn emplace_slice_operation_sequences() -> CheckResult<EmplaceSliceProgram> {
    check().run(|program: &EmplaceSliceProgram| -> Result<(), String> {
        let bump = Bump::new();
        let mut place = bump.emplace_slice::<u8>();
        let mut model: Vec<u8> = Vec::new();

        for op in program.0.iter().take(MAX_EMPLACE_OPS) {
            match op {
                EmplaceSliceOp::Push(val) => {
                    place.push(*val);
                    model.push(*val);
                }
                EmplaceSliceOp::Extend(vals) => {
                    let vals = &vals[..vals.len().min(MAX_EMPLACE_EXTEND_LEN)];
                    place.extend(vals.iter().copied());
                    model.extend(vals.iter().copied());
                }
                EmplaceSliceOp::Reserve(n) => {
                    let n = *n % MAX_EMPLACE_RESERVE;
                    place.reserve(n);
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
        Ok(())
    })
}

#[test]
fn emplace_slice_copy() -> CheckResult<Vec<u8>> {
    check().run(|data: &Vec<u8>| -> Result<(), String> {
        // Sweep the initial capacity across values below, equal to, and above
        // the data length. This exercises all cases of our capacity-growing
        // behavior.
        for capacity in 0..=(data.len() + 2).min(10) {
            let bump = Bump::new();
            let place = bump.emplace_slice_with_capacity(capacity);
            let result = place.copy_slice(data.as_slice());
            assert_eq!(result, data.as_slice());
        }
        Ok(())
    })
}

#[test]
fn emplace_slice_clone() -> CheckResult<Vec<u8>> {
    check().run(|data: &Vec<u8>| -> Result<(), String> {
        // See comment in `emplace_slice_copy`.
        for capacity in 0..=(data.len() + 2).min(10) {
            let bump = Bump::new();
            let place = bump.emplace_slice_with_capacity(capacity);
            let result = place.clone_slice(data.as_slice());
            assert_eq!(result, data.as_slice());
        }
        Ok(())
    })
}

#[test]
fn emplace_slice_write_iter() -> CheckResult<Vec<u8>> {
    check().run(|data: &Vec<u8>| -> Result<(), String> {
        let bump = Bump::new();
        let place = bump.emplace_slice::<u8>();
        let result = place.write_iter(data.iter().copied());
        assert_eq!(result, data.as_slice());
        Ok(())
    })
}

#[test]
fn emplace_str_roundtrip() -> CheckResult<Vec<String>> {
    check().run(|strings: &Vec<String>| -> Result<(), String> {
        let bump = Bump::new();
        for s in strings {
            let r = bump.emplace_str_with_capacity(s.len()).write_str(s);
            assert_eq!(r, s.as_str());
        }
        Ok(())
    })
}

#[test]
fn emplace_str_push_chars() -> CheckResult<String> {
    check().run(|s: &String| -> Result<(), String> {
        let bump = Bump::new();
        let mut place = bump.emplace_str();
        for c in s.chars() {
            place.push(c);
        }
        let result = place.into_str();
        assert_eq!(result, s.as_str());
        Ok(())
    })
}

#[test]
fn emplace_str_push_strs() -> CheckResult<Vec<String>> {
    check().run(|parts: &Vec<String>| -> Result<(), String> {
        let bump = Bump::new();
        let mut place = bump.emplace_str();
        let mut model = String::new();
        for part in parts {
            place.push_str(part);
            model.push_str(part);
        }
        let result = place.into_str();
        assert_eq!(result, model.as_str());
        Ok(())
    })
}

#[derive(Clone, Copy, Debug, Mutate)]
enum NoOverlapOp {
    Emplace,
    EmplaceSliceWithCapacity,
    EmplaceStr,
    Alloc,
}

#[test]
fn heterogeneous_emplace_no_overlap() -> CheckResult<Vec<NoOverlapOp>> {
    check().run(|ops: &Vec<NoOverlapOp>| -> Result<(), String> {
        let bump = Bump::new();
        let mut ranges: Vec<(usize, usize)> = vec![];

        for (i, op) in ops.iter().enumerate() {
            let range = match op {
                NoOverlapOp::Emplace => {
                    let r = bump.emplace().write(i as u64);
                    range_of(r as *const u64, 1)
                }
                NoOverlapOp::EmplaceSliceWithCapacity => {
                    let data = [i as u8; 4];
                    let r = bump.emplace_slice_with_capacity(4).copy_slice(&data);
                    range_of(r.as_ptr(), r.len())
                }
                NoOverlapOp::EmplaceStr => {
                    let s = bump.emplace_str().write_str("hi");
                    range_of(s.as_ptr(), s.len())
                }
                NoOverlapOp::Alloc => {
                    let r = bump.alloc(i as u32);
                    range_of(r as *const u32, 1)
                }
            };
            if range.0 != range.1 {
                for prev in &ranges {
                    assert!(
                        !ranges_overlap(range, *prev),
                        "overlap: {:?} vs {:?}",
                        range,
                        prev
                    );
                }
                ranges.push(range);
            }
        }
        Ok(())
    })
}

#[test]
fn emplace_rewind_on_drop() -> CheckResult<Vec<u64>> {
    check().run(|values: &Vec<u64>| -> Result<(), String> {
        let bump = Bump::new();
        for v in values {
            let capacity_before = bump.chunk_capacity();
            {
                let place = bump.emplace::<u64>();
                drop(place);
            }
            let capacity_after = bump.chunk_capacity();

            // If the emplace stayed in the same chunk, capacity should be fully
            // restored. If it triggered a new chunk, capacity_after will be the
            // full new-chunk size which is still >= capacity_before.
            assert!(
                capacity_before <= capacity_after,
                "capacity should be at least restored on drop: before={}, after={}",
                capacity_before,
                capacity_after
            );

            // Verify we can still allocate correctly after the rewind.
            let r = bump.alloc(*v);
            assert_eq!(*r, *v);
        }
        Ok(())
    })
}

#[test]
fn emplace_slice_rewind_on_drop() -> CheckResult<Vec<u8>> {
    check().run(|lens: &Vec<u8>| -> Result<(), String> {
        let bump = Bump::new();
        for len in lens {
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
                capacity_before <= capacity_after,
                "capacity should be reclaimed on drop: before={}, after={}",
                capacity_before,
                capacity_after
            );
        }
        Ok(())
    })
}

#[test]
fn emplace_equivalence_alloc() -> CheckResult<Vec<u64>> {
    check().run(|values: &Vec<u64>| -> Result<(), String> {
        let bump1 = Bump::new();
        let bump2 = Bump::new();
        for v in values {
            let r1 = bump1.alloc(*v);
            let r2 = bump2.emplace().write(*v);
            assert_eq!(*r1, *r2);
        }
        Ok(())
    })
}

#[test]
fn emplace_equivalence_slice_copy() -> CheckResult<Vec<Vec<u8>>> {
    check().run(|slices: &Vec<Vec<u8>>| -> Result<(), String> {
        let bump1 = Bump::new();
        let bump2 = Bump::new();
        for s in slices {
            let r1 = bump1.alloc_slice_copy(s);
            let r2 = bump2.emplace_slice_with_capacity(s.len()).copy_slice(s);
            assert_eq!(r1, r2);
        }
        Ok(())
    })
}

#[test]
fn emplace_equivalence_str() -> CheckResult<Vec<String>> {
    check().run(|strings: &Vec<String>| -> Result<(), String> {
        let bump1 = Bump::new();
        let bump2 = Bump::new();
        for s in strings {
            let r1 = bump1.alloc_str(s);
            let r2 = bump2.emplace_str_with_capacity(s.len()).write_str(s);
            assert_eq!(r1, r2);
        }
        Ok(())
    })
}

#[test]
fn alignment_single_values() -> CheckResult<Vec<u64>> {
    check().run(|values: &Vec<u64>| -> Result<(), String> {
        macro_rules! test_align {
            ($align:literal) => {{
                let bump = bumpalo::Bump::<$align>::with_min_align();
                for v in values {
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
        Ok(())
    })
}

#[test]
fn alignment_slices() -> CheckResult<Vec<Vec<u32>>> {
    check().run(|data: &Vec<Vec<u32>>| -> Result<(), String> {
        macro_rules! test_align {
            ($align:literal) => {{
                let bump = bumpalo::Bump::<$align>::with_min_align();
                for slice in data {
                    let r = bump
                        .emplace_slice_with_capacity(slice.len())
                        .copy_slice(slice);
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
        Ok(())
    })
}

#[test]
fn allocation_limits_respected() -> CheckResult<Vec<Vec<u8>>> {
    check().run(|data: &Vec<Vec<u8>>| -> Result<(), String> {
        let data = data.clone();
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
        Ok(())
    })
}

#[test]
fn extend_from_slice_copy() -> CheckResult<(Vec<u8>, Vec<u8>)> {
    check().run(|input: &(Vec<u8>, Vec<u8>)| -> Result<(), String> {
        let (data1, data2) = input;
        let bump = Bump::new();
        let mut place = bump.emplace_slice_with_capacity::<u8>(data1.len());
        place.extend_from_slice_copy(data1);
        assert_eq!(place.as_slice(), data1.as_slice());

        place.extend_from_slice_copy(data2);
        assert_eq!(place.len(), data1.len() + data2.len());
        assert_eq!(&place.as_slice()[..data1.len()], data1.as_slice());
        assert_eq!(&place.as_slice()[data1.len()..], data2.as_slice());
        let _ = place.into_mut_slice();
        Ok(())
    })
}

#[test]
fn extend_from_slice_clone() -> CheckResult<(Vec<u8>, Vec<u8>)> {
    check().run(|input: &(Vec<u8>, Vec<u8>)| -> Result<(), String> {
        let (data1, data2) = input;
        let bump = Bump::new();
        let mut place = bump.emplace_slice_with_capacity::<u8>(data1.len());
        place.extend_from_slice_clone(data1);
        assert_eq!(place.as_slice(), data1.as_slice());

        place.extend_from_slice_clone(data2);
        assert_eq!(place.len(), data1.len() + data2.len());
        assert_eq!(&place.as_slice()[..data1.len()], data1.as_slice());
        assert_eq!(&place.as_slice()[data1.len()..], data2.as_slice());
        let _ = place.into_mut_slice();
        Ok(())
    })
}

#[test]
fn issue_330_emplace_slice_copy_grows() {
    for &(capacity, len) in &[(1, 2), (1, 10), (4, 6), (4, 8), (4, 100), (16, 17)] {
        let bump = Bump::new();
        let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
        let place = bump.emplace_slice_with_capacity::<u8>(capacity);
        let result = place.copy_slice(&data);
        assert_eq!(result, data.as_slice());
    }

    // The default capacity (from `emplace_slice`) is smaller than the data.
    let bump = Bump::new();
    let data: Vec<u8> = (0..64).collect();
    let result = bump.emplace_slice::<u8>().copy_slice(&data);
    assert_eq!(result, data.as_slice());
}

#[test]
fn issue_330_emplace_slice_clone_grows() {
    for &(capacity, len) in &[(1, 2), (1, 10), (4, 6), (4, 8), (4, 100), (16, 17)] {
        let bump = Bump::new();
        let data: Vec<String> = (0..len).map(|i| i.to_string()).collect();
        let place = bump.emplace_slice_with_capacity::<String>(capacity);
        let result = place.clone_slice(&data);
        assert_eq!(result, data.as_slice());
    }

    // The default capacity (from `emplace_slice`) is smaller than the data.
    let bump = Bump::new();
    let data: Vec<String> = (0..64).map(|i| i.to_string()).collect();
    let result = bump.emplace_slice::<String>().clone_slice(&data);
    assert_eq!(result, data.as_slice());
}
