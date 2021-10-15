#![cfg(feature = "collections")]
use bumpalo::collections::{CollectIn,Vec, String};
use bumpalo::Bump;
use quickcheck::{quickcheck};
use std::string::String as StdString;
use std::vec::Vec as StdVec;


#[cfg(test)]
quickcheck! {
  fn test_string_collect(input: StdString) -> bool {
    let bump = Bump::new();
    let bump_str = input.chars().collect_in::<String>(&bump);
    &bump_str == &input
  }
}

#[cfg(test)]
quickcheck! {
  fn test_vec_collect(input: StdVec<i32>) -> bool {
    let bump = Bump::new();
    let bump_vec = input.clone().into_iter().collect_in::<Vec<_>>(&bump);
    let bump_ref: &[i32] = &bump_vec;

    bump_ref == &input
  }


}