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

                // Use QUICKCHECK_TESTS from compiletime to surpass miri isolation
                let tests = option_env!("QUICKCHECK_TESTS").and_then(|s| s.parse().ok());
                #[cfg(miri)]
                let tests = tests.or(Some(1));

                let mut qc = ::quickcheck::QuickCheck::new();

                if let Some(tests) = tests {
                    eprintln!("Executing {} quickchecks", tests);
                    qc = qc.tests(tests)
                }

                qc.quickcheck(prop as fn($($arg_ty),*) -> $ret);
            }
        )*
    };
}
