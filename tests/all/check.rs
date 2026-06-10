//! Helpers for our property-based tests using `mutatis::check::Check`.

use mutatis::check::Check;

#[cfg(miri)]
const ITERS: usize = 48;
#[cfg(not(miri))]
const ITERS: usize = 12000;

/// Create and configure a [`mutatis::check::Check`] for use in our
/// property-based tests.
pub fn check() -> Check {
    let mut check = Check::new();
    check.iters(ITERS).shrink_iters(ITERS);
    check
}
