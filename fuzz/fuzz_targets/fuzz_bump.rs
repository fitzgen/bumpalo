#![no_main]

use core::slice;
use std::{alloc::Layout, ptr::NonNull};

use arbitrary::Arbitrary;
use bumpalo::Bump;
use libfuzzer_sys::fuzz_target;
use rand::distr::SampleString;

#[derive(Arbitrary, Clone, Copy, Debug)]
enum AllocWith {
    None,
    Try(AllocTryWithResult),
    With,
}

#[derive(Arbitrary, Clone, Copy, Debug)]
enum AllocTryWithResult {
    Success,
    Failed,
    FailedAlloced,
}

#[derive(Arbitrary, Clone, Copy, Debug)]
enum AllocAlign {
    A1,
    A2,
    A4,
    A8,
    A16,
    A32,
    A64,
}

impl AllocAlign {
    fn align(self) -> usize {
        match self {
            AllocAlign::A1 => 1,
            AllocAlign::A2 => 2,
            AllocAlign::A4 => 4,
            AllocAlign::A8 => 8,
            AllocAlign::A16 => 16,
            AllocAlign::A32 => 32,
            AllocAlign::A64 => 64,
        }
    }
}

macro_rules! gen_alloc {
    ($name:ident, $($nums:expr),*) => {
        paste::paste! {
            #[derive(Arbitrary, Clone, Copy, Debug)]
            enum $name {
                $( [< U $nums >] ),*
            }

            impl $name {
                fn size(self) -> usize {
                    match self {
                        $( Self::[< U $nums >] => $nums ),*
                    }
                }
            }
        }

        $(gen_alloc_aligns!($nums, 1, 2, 4, 8, 16, 32, 64);)*
    };
}

macro_rules! gen_alloc_aligns {
    ($num:expr, $($aligns:expr),*) => {
        $(
            paste::paste! {
                #[repr(align($aligns))]
                #[derive(Debug)]
                #[allow(dead_code)]
                struct [< T $num _ $aligns >]([u8; $num]);

                impl [< T $num _ $aligns >] {
                    fn new(byte: u8) -> Self {
                        Self([byte; $num])
                    }
                }
            }
        )*
    }
}

macro_rules! gen_alloc_match {
    (
        $bump:ident,
        $byte:expr,
        $size:ident,
        $align:ident,
        $($nums:expr),*
    ) => {
        paste::paste! {
            match ($size, $align) {
                $(
                    (AllocSize::[<U $nums>], AllocAlign::A1)
                        => NonNull::from($bump.alloc([<T $nums _1>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A2)
                        => NonNull::from($bump.alloc([<T $nums _2>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A4)
                        => NonNull::from($bump.alloc([<T $nums _4>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A8)
                        => NonNull::from($bump.alloc([<T $nums _8>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A16)
                        => NonNull::from($bump.alloc([<T $nums _16>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A32)
                        => NonNull::from($bump.alloc([<T $nums _32>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A64)
                        => NonNull::from($bump.alloc([<T $nums _64>]::new($byte))).cast(),
                )*
            }
        }
    };
}

macro_rules! gen_alloc_with_match {
    (
        $bump:ident,
        $byte:expr,
        $size:ident,
        $align:ident,
        $($nums:expr),*
    ) => {
        paste::paste! {
            match ($size, $align) {
                $(
                    (AllocSize::[<U $nums>], AllocAlign::A1)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _1>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A2)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _2>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A4)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _4>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A8)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _8>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A16)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _16>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A32)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _32>]::new($byte))).cast(),
                    (AllocSize::[<U $nums>], AllocAlign::A64)
                        => NonNull::from($bump.alloc_with(|| [<T $nums _64>]::new($byte))).cast(),
                )*
            }
        }
    };
}

macro_rules! gen_alloc_try_with_match {
    (
        $bump:ident,
        $byte:expr,
        $size:ident,
        $align:ident,
        $result:ident,
        $($nums:expr),*
    ) => {
        paste::paste! {
            match ($size, $align) {
                $(
                    (AllocSize::[<U $nums>], AllocAlign::A1)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _1>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
                    (AllocSize::[<U $nums>], AllocAlign::A2)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _2>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
                    (AllocSize::[<U $nums>], AllocAlign::A4)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _4>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
                    (AllocSize::[<U $nums>], AllocAlign::A8)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _8>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
                    (AllocSize::[<U $nums>], AllocAlign::A16)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _16>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
                    (AllocSize::[<U $nums>], AllocAlign::A32)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _32>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
                    (AllocSize::[<U $nums>], AllocAlign::A64)
                        => {
                            let Ok(ptr) = $bump.alloc_try_with(|| match $result {
                                AllocTryWithResult::Success => Ok([<T $nums _64>]::new($byte)),
                                AllocTryWithResult::Failed => Err(()),
                                AllocTryWithResult::FailedAlloced => {
                                    $bump.alloc(0xFFu8);
                                    Err(())
                                }
                            }) else {
                                continue;
                            };
                            NonNull::from(ptr).cast()
                        }
            )*
            }
        }
    };
}

macro_rules! gen_try_alloc_match {
    (
        $bump:ident,
        $byte:expr,
        $size:ident,
        $align:ident,
        $($nums:expr),*
    ) => {
        paste::paste! {
            match ($size, $align) {
                $(
                    (AllocSize::[<U $nums>], AllocAlign::A1)
                        => $bump.try_alloc::<[<T $nums _1>]>([<T $nums _1>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                    (AllocSize::[<U $nums>], AllocAlign::A2)
                        => $bump.try_alloc::<[<T $nums _2>]>([<T $nums _2>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                    (AllocSize::[<U $nums>], AllocAlign::A4)
                        => $bump.try_alloc::<[<T $nums _4>]>([<T $nums _4>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                    (AllocSize::[<U $nums>], AllocAlign::A8)
                        => $bump.try_alloc::<[<T $nums _8>]>([<T $nums _8>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                    (AllocSize::[<U $nums>], AllocAlign::A16)
                        => $bump.try_alloc::<[<T $nums _16>]>([<T $nums _16>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                    (AllocSize::[<U $nums>], AllocAlign::A32)
                        => $bump.try_alloc::<[<T $nums _32>]>([<T $nums _32>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                    (AllocSize::[<U $nums>], AllocAlign::A64)
                        => $bump.try_alloc::<[<T $nums _64>]>([<T $nums _64>]::new($byte)).map(NonNull::from).map(NonNull::cast),
                )*
            }
        }
    };
}

gen_alloc!(AllocSize, 0, 1, 13, 47, 71, 97, 157, 311, 523, 727, 997);

#[derive(Debug)]
enum BumpMethod {
    Alloc {
        with: AllocWith,
        size: AllocSize,
        align: AllocAlign,
        byte: u8,
    },
    AllocLayout {
        layout: Layout,
        byte: u8,
    },
    AllocStr {
        length: usize,
    },
    ModifyContent {
        index: usize,
    },
    CheckContent {
        index: usize,
    },
    SetLimit {
        limit: Option<usize>,
    },
    Reset,
}

impl Arbitrary<'_> for BumpMethod {
    fn arbitrary(
        u: &mut libfuzzer_sys::arbitrary::Unstructured<'_>,
    ) -> libfuzzer_sys::arbitrary::Result<Self> {
        let method = match u.choose_index(128)? {
            0..100 => {
                let size = match u.choose_index(16)? {
                    0..12 => u.int_in_range(0..=1024)?,
                    12..15 => u.int_in_range(1025..=65536)?,
                    _ => u.int_in_range(65537..=1048576)?,
                };

                match u.choose_index(4)? {
                    0 => BumpMethod::AllocStr { length: size / 64 },
                    1 => BumpMethod::Alloc {
                        with: u.arbitrary()?,
                        size: u.arbitrary()?,
                        align: u.arbitrary()?,
                        byte: rand::random(),
                    },
                    _ => {
                        let alignment = 1 << u.choose_index(12)?;
                        let byte = rand::random();

                        BumpMethod::AllocLayout {
                            layout: Layout::from_size_align(size, alignment).unwrap(),
                            byte,
                        }
                    }
                }
            }
            100..110 => BumpMethod::ModifyContent {
                index: u.arbitrary()?,
            },
            110..125 => BumpMethod::CheckContent {
                index: u.arbitrary()?,
            },
            125..127 => BumpMethod::SetLimit {
                limit: u.arbitrary()?,
            },
            _ => BumpMethod::Reset,
        };
        Ok(method)
    }
}

#[derive(Debug, Clone, Copy, Arbitrary)]
enum MinAlign {
    U1,
    U2,
    U4,
    U8,
    U16,
}

#[derive(Debug, Clone, Copy)]
struct BumpCapacity(usize);

impl Arbitrary<'_> for BumpCapacity {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let size = match u.choose_index(16)? {
            0..12 => u.int_in_range(0..=1024)?,
            12..15 => u.int_in_range(1025..=65536)?,
            _ => u.int_in_range(65537..=1048576)?,
        };

        Ok(Self(size))
    }
}

#[derive(Debug, Arbitrary)]
struct BumpConfig {
    min_align: MinAlign,
    capacity: Option<BumpCapacity>,
    methods: Vec<BumpMethod>,
}

#[derive(Debug)]
enum AllocatedEntry {
    T {
        ptr: NonNull<u8>,
        byte: u8,
        size: AllocSize,
    },
    Layout {
        ptr: NonNull<u8>,
        byte: u8,
        layout: Layout,
    },
    Str {
        ptr: NonNull<[u8]>,
        reference: String,
    },
}

fuzz_target!(|config: BumpConfig| {
    match config.min_align {
        MinAlign::U1 => run_fuzz::<1>(config),
        MinAlign::U2 => run_fuzz::<2>(config),
        MinAlign::U4 => run_fuzz::<4>(config),
        MinAlign::U8 => run_fuzz::<8>(config),
        MinAlign::U16 => run_fuzz::<16>(config),
    };
});

fn run_fuzz<const N: usize>(config: BumpConfig) {
    let BumpConfig {
        capacity, methods, ..
    } = config;

    let mut bump = if let Some(capacity) = capacity {
        Bump::<N>::with_min_align_and_capacity(capacity.0)
    } else {
        Bump::<N>::with_min_align()
    };
    let mut has_allocation_limit = false;
    let mut allocated = Vec::new();
    let mut rng = rand::rng();

    assert_eq!(bump.min_align(), N);

    for method in methods {
        match method {
            BumpMethod::Alloc {
                with,
                size,
                align,
                byte,
            } => {
                let ptr: NonNull<u8> = if has_allocation_limit {
                    let Ok(ptr) = gen_try_alloc_match!(
                        bump, byte, size, align, 0, 1, 13, 47, 71, 97, 157, 311, 523, 727, 997
                    ) else {
                        continue;
                    };
                    ptr
                } else {
                    match with {
                        AllocWith::None => gen_alloc_match!(
                            bump, byte, size, align, 0, 1, 13, 47, 71, 97, 157, 311, 523, 727, 997
                        ),
                        AllocWith::Try(result) => gen_alloc_try_with_match!(
                            bump, byte, size, align, result, 0, 1, 13, 47, 71, 97, 157, 311, 523,
                            727, 997
                        ),
                        AllocWith::With => gen_alloc_with_match!(
                            bump, byte, size, align, 0, 1, 13, 47, 71, 97, 157, 311, 523, 727, 997
                        ),
                    }
                };

                let adjusted_ptr = if !has_allocation_limit && matches!(with, AllocWith::Try(_)) {
                    // min_align affects only the pointer to the surrounding Result<T, ()>.
                    unsafe { ptr.sub(align.align()) }
                } else {
                    ptr
                };

                // check correct alignment for both layout and min_align.
                assert_eq!(ptr.addr().get() % align.align(), 0);
                assert_eq!(adjusted_ptr.addr().get() % N, 0);

                allocated.push(AllocatedEntry::T { ptr, byte, size });
            }
            BumpMethod::AllocLayout { layout, byte } => {
                let ptr = if has_allocation_limit {
                    let Ok(ptr) = bump.try_alloc_layout(layout) else {
                        continue;
                    };
                    ptr
                } else {
                    bump.alloc_layout(layout)
                };
                // check correct alignment for both layout and min_align.
                assert_eq!(ptr.addr().get() % layout.align(), 0);
                assert_eq!(ptr.addr().get() % N, 0);
                unsafe {
                    core::ptr::write_bytes(ptr.as_ptr(), byte, layout.size());
                }
                allocated.push(AllocatedEntry::Layout { ptr, byte, layout })
            }
            BumpMethod::AllocStr { length } => {
                let string = rand::distr::StandardUniform.sample_string(&mut rng, length);
                let str = if has_allocation_limit {
                    let Ok(str) = bump.try_alloc_str(&string) else {
                        continue;
                    };
                    str
                } else {
                    bump.alloc_str(&string)
                };
                assert_eq!(str.len(), string.len());
                let ptr = unsafe { NonNull::from(str.as_bytes_mut()) };
                allocated.push(AllocatedEntry::Str {
                    ptr,
                    reference: string,
                })
            }
            BumpMethod::ModifyContent { index } => {
                if !allocated.is_empty() {
                    let len = allocated.len();
                    let alloc = &mut allocated[index % len];
                    match alloc {
                        AllocatedEntry::T { ptr, byte, size } => {
                            *byte = rand::random();
                            unsafe {
                                core::ptr::write_bytes(ptr.as_ptr(), *byte, size.size());
                            }
                        }
                        AllocatedEntry::Layout { ptr, byte, layout } => {
                            *byte = rand::random();
                            unsafe {
                                core::ptr::write_bytes(ptr.as_ptr(), *byte, layout.size());
                            }
                        }
                        AllocatedEntry::Str { ptr, reference } => {
                            let len = ptr.len();
                            loop {
                                let mut string =
                                    rand::distr::StandardUniform.sample_string(&mut rng, len);
                                // false when len > string.len().
                                if !string.is_char_boundary(len) {
                                    continue;
                                }

                                string.truncate(len);
                                *reference = string;
                                break;
                            }

                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    reference.as_ptr(),
                                    ptr.as_ptr().cast(),
                                    len,
                                );
                            }
                        }
                    }
                }
            }
            BumpMethod::CheckContent { index } => {
                // check allocated hasn't been overwritten.
                if !allocated.is_empty() {
                    let alloc = &allocated[index % allocated.len()];
                    match alloc {
                        AllocatedEntry::T { ptr, byte, size } => {
                            let slice = unsafe { slice::from_raw_parts(ptr.as_ptr(), size.size()) };
                            assert!(slice.iter().all(|x| *x == *byte));
                        }
                        AllocatedEntry::Layout { ptr, byte, layout } => {
                            let slice =
                                unsafe { slice::from_raw_parts(ptr.as_ptr(), layout.size()) };
                            assert!(slice.iter().all(|x| *x == *byte));
                        }
                        AllocatedEntry::Str { ptr, reference } => {
                            let str = unsafe { str::from_utf8(ptr.as_ref()).unwrap() };
                            assert_eq!(str, reference);
                        }
                    }
                }
            }
            BumpMethod::SetLimit { limit } => {
                bump.set_allocation_limit(limit);
                assert_eq!(bump.allocation_limit(), limit);
                has_allocation_limit = limit.is_some();
            }
            BumpMethod::Reset => {
                allocated.clear();
                bump.reset();
            }
        }
    }

    allocated.clear();
    bump.reset();
}
