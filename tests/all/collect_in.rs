#![cfg(feature = "collections")]

use crate::check::check;
use bumpalo::collections::{CollectIn, String, Vec};
use bumpalo::Bump;
use mutatis::check::CheckResult;
use std::string::String as StdString;
use std::vec::Vec as StdVec;

#[test]
fn test_string_collect() -> CheckResult<StdString> {
    check().run(|input: &StdString| -> Result<(), StdString> {
        let bump = Bump::new();
        let bump_str = input.chars().collect_in::<String>(&bump);

        if bump_str == *input {
            Ok(())
        } else {
            Err(format!("{bump_str:?} != {input:?}"))
        }
    })
}

#[test]
fn test_vec_collect() -> CheckResult<StdVec<i32>> {
    check().run(|input: &StdVec<i32>| -> Result<(), StdString> {
        let bump = Bump::new();
        let bump_vec = input.iter().copied().collect_in::<Vec<_>>(&bump);

        if bump_vec.as_slice() == input.as_slice() {
            Ok(())
        } else {
            Err(format!("{bump_vec:?} != {input:?}"))
        }
    })
}
