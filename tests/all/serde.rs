#![cfg(feature = "collections")]
#![cfg(feature = "serde")]

use bumpalo::{boxed::Box, vec, Bump};
use serde::{Deserialize, Serialize};

macro_rules! compare_std_vec {
    (in $bump:ident; $($x:expr),+) => {{
        let vec = vec![in &$bump; $($x),+];
        let std_vec = std::vec![$($x),+];
        (vec, std_vec)
    }}
}

macro_rules! compare_std_box {
    (in $bump:ident; $x:expr) => {
        (Box::new_in($x, &$bump), std::boxed::Box::new($x))
    };
}

macro_rules! assert_eq_json {
    ($a:ident, $b:ident) => {
        assert_eq!(
            serde_json::to_string(&$a).unwrap(),
            serde_json::to_string(&$b).unwrap(),
        )
    };
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "t", content = "c")]
enum Test {
    First,
    Second,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde()]
struct Mixed {
    i: i32,
    s: String,
    o: Option<String>,
    e: Test,
}

#[test]
fn test_vec_serializes_str() {
    let bump = Bump::new();
    let (vec, std_vec) = compare_std_vec![in bump; "hello", "world"];
    assert_eq_json!(vec, std_vec);
    let de: std::vec::Vec<String> =
        serde_json::from_str(&serde_json::to_string(&vec).unwrap()).unwrap();
    assert_eq!(de, std_vec);
}

#[test]
fn test_vec_serializes_f32() {
    let bump = Bump::new();
    let (vec, std_vec) = compare_std_vec![in bump; 1.5707964, 3.1415927];
    assert_eq_json!(vec, std_vec);
    let de: std::vec::Vec<f32> =
        serde_json::from_str(&serde_json::to_string(&vec).unwrap()).unwrap();
    assert_eq!(de, std_vec);
}

#[cfg(feature = "serde")]
#[test]
fn test_vec_serializes_complex() {
    let bump = Bump::new();
    let (vec, std_vec) = compare_std_vec![
        in bump;
        Mixed {
            i: 8,
            s: "a".into(),
            o: None,
            e: Test::Second,
        },
        Mixed {
            i: 8,
            s: "b".into(),
            o: Some("some".into()),
            e: Test::First,
        }
    ];
    assert_eq_json!(vec, std_vec);
    let de: std::vec::Vec<Mixed> =
        serde_json::from_str(&serde_json::to_string(&vec).unwrap()).unwrap();
    assert_eq!(de, std_vec);
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

#[cfg(feature = "serde")]
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
