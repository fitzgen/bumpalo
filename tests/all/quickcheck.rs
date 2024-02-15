use std::alloc::Layout;

/// A redefinition/wrapper macro of `quickcheck::quickcheck!` that supports
/// limiting the number of test iterations to one when we are running under
/// MIRI.
#[macro_export]
macro_rules! quickcheck {
    (
        $(
            $(#[$m:meta])*
            fn $fn_name:ident($($arg_name:ident : $arg_ty:ty),*) -> $ret:ty {
                $($code:tt)*
            }
        )*
    ) => {
        $(
            #[test]
            $(#[$m])*
            fn $fn_name() {
                fn prop($($arg_name: $arg_ty),*) -> $ret {
                    $($code)*
                }

                let mut qc = ::quickcheck::QuickCheck::new();

                // Use the `QUICKCHECK_TESTS` environment variable from
                // compiletime to avoid violating MIRI's isolation by looking at
                // the runtime environment variable.
                let tests = option_env!("QUICKCHECK_TESTS").and_then(|s| s.parse().ok());

                // Limit quickcheck tests to a single iteration under MIRI,
                // since they are otherwise super slow.
                #[cfg(miri)]
                let tests = tests.or(Some(1));

                if let Some(tests) = tests {
                    eprintln!("Executing at most {} quickchecks", tests);
                    qc = qc.tests(tests);
                }

                qc.quickcheck(prop as fn($($arg_ty),*) -> $ret);
            }
        )*
    };
}

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
