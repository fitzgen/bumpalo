#![cfg(all(feature = "boxed", feature = "serde"))]

use super::{assert_eq_json, Mixed, Test};

use bumpalo::{boxed::Box, Bump};

macro_rules! compare_std_box {
    (in $bump:ident; $x:expr) => {
        (Box::new_in($x, &$bump), std::boxed::Box::new($x))
    };
}

#[test]
fn test_box_serializes() {
    let bump = Bump::new();
    let (box_int, std_box_int) = compare_std_box!(in bump; 1);
    assert_eq_json!(box_int, std_box_int);
    let (box_str, std_box_str) = compare_std_box!(in bump; 1);
    assert_eq_json!(box_str, std_box_str);
    let (box_vec, std_box_vec) = compare_std_box!(in bump; std::vec!["hello", "world"]);
    assert_eq_json!(box_vec, std_box_vec);
}

#[test]
fn test_box_serializes_unsized() {
    let bump = Bump::new();

    // Box<'a, [T]>, built from a sized array box since bumpalo's Box does not
    // rely on unsize coercion.
    let boxed_slice: Box<[i32]> = Box::from(Box::new_in([1, 2, 3], &bump));
    let std_boxed_slice: std::boxed::Box<[i32]> = std::boxed::Box::new([1, 2, 3]);
    assert_eq_json!(boxed_slice, std_boxed_slice);

    // Box<'a, str>.
    let boxed_str: Box<str> = unsafe { Box::from_raw(bump.alloc_str("hello")) };
    let std_boxed_str: std::boxed::Box<str> = std::boxed::Box::from("hello");
    assert_eq_json!(boxed_str, std_boxed_str);
}

#[test]
fn test_box_serializes_complex() {
    let bump = Bump::new();
    let (vec, std_vec) = compare_std_box![
        in bump;
        Mixed {
            i: 8,
            s: "a".into(),
            o: None,
            e: Test::Second,
        }
    ];
    assert_eq_json!(vec, std_vec);
    let de: std::boxed::Box<Mixed> =
        serde_json::from_str(&serde_json::to_string(&vec).unwrap()).unwrap();
    assert_eq!(de, std_vec);
}
