#![cfg(feature = "collections")]
#![cfg_attr(
    all(miri, not(feature = "test_skip_miri_quickchecks")),
    allow(unused_imports)
)]
use bumpalo::collections::{CollectIn, String, Vec};
use bumpalo::Bump;
use quickcheck::quickcheck;
use std::string::String as StdString;
use std::vec::Vec as StdVec;

#[cfg(not(all(miri, feature = "test_skip_miri_quickchecks")))]
quickcheck! {
  fn test_string_collect(input: StdString) -> bool {
    let bump = Bump::new();
    let bump_str = input.chars().collect_in::<String>(&bump);

    bump_str == input
  }

  fn test_vec_collect(input: StdVec<i32>) -> bool {
    let bump = Bump::new();
    let bump_vec = input.clone().into_iter().collect_in::<Vec<_>>(&bump);

    bump_vec.as_slice() == input.as_slice()
  }
}
