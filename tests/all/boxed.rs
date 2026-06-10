#![cfg(feature = "boxed")]
#[cfg(feature = "collections")]
use crate::check::check;
use bumpalo::boxed::Box;
#[cfg(feature = "collections")]
use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump;
#[cfg(feature = "collections")]
use mutatis::check::CheckResult;
#[cfg(feature = "collections")]
use mutatis::Mutate;
#[cfg(feature = "collections")]
use std::cell::RefCell;
#[cfg(feature = "collections")]
use std::rc::Rc;

#[cfg(feature = "collections")]
const MAX_BOX_OPS: usize = 64;
#[cfg(feature = "collections")]
const MAX_BOX_LEN: usize = 48;

#[cfg(feature = "collections")]
#[derive(Clone, Debug, Mutate)]
enum BoxOp {
    Push(u8),
    Pop,
    Remove(u8),
    Truncate(u8),
    Clear,
}

#[cfg(feature = "collections")]
#[derive(Clone, Debug, Default, Mutate)]
struct BoxProgram(std::vec::Vec<BoxOp>);

#[cfg(feature = "collections")]
#[derive(Debug)]
struct DropSpy {
    serial: usize,
    payload: u8,
    drops: Rc<RefCell<std::vec::Vec<usize>>>,
}

#[cfg(feature = "collections")]
impl Drop for DropSpy {
    fn drop(&mut self) {
        self.drops.borrow_mut().push(self.serial);
    }
}

#[cfg(feature = "collections")]
fn live_payloads(vec: &BumpVec<'_, Box<'_, DropSpy>>) -> std::vec::Vec<u8> {
    vec.iter().map(|boxed| boxed.payload).collect()
}

#[cfg(feature = "collections")]
fn drop_ids_since(drops: &Rc<RefCell<std::vec::Vec<usize>>>, start: usize) -> std::vec::Vec<usize> {
    drops.borrow()[start..].to_vec()
}

#[test]
fn into_raw_aliasing() {
    let bump = Bump::new();
    let boxed = Box::new_in(1, &bump);
    let raw = Box::into_raw(boxed);

    let mut_ref = unsafe { &mut *raw };
    dbg!(mut_ref);
}

// This tests some basic functionality of the box.
#[test]
fn test_box_basic() {
    let bump = Bump::new();
    let mut value = Box::new_in("hello".to_string(), &bump);
    assert_eq!("hello", &*value);
    *value = "world".to_string();
    assert_eq!("world", &*value);
}

// This function tests that `Box` is covariant.
fn _box_is_covariant<'sup, 'sub: 'sup>(
    a: Box<&'sup u32>,
    b: Box<&'sub u32>,
    f: impl Fn(Box<&'sup u32>),
) {
    f(a);
    f(b);
}

#[test]
fn box_is_send_sync() {
    fn assert_send(_: impl Send) {}
    fn assert_sync(_: impl Sync) {}

    let bump = Bump::new();
    assert_send(Box::new_in(42, &bump));
    assert_sync(Box::new_in(42, &bump));

    // Check `?Sized` types as well.
    let boxed_str: Box<'static, str> = Default::default();
    assert_send(boxed_str);
    let boxed_str: Box<'static, str> = Default::default();
    assert_sync(boxed_str);
}

#[cfg(feature = "collections")]
#[test]
fn boxed_vec_operation_sequences_drop_exactly_once() -> CheckResult<BoxProgram> {
    check().run(|program: &BoxProgram| -> Result<(), String> {
        let bump = Bump::new();
        let drops = Rc::new(RefCell::new(std::vec::Vec::new()));
        let mut actual = BumpVec::new_in(&bump);
        let mut expected = std::vec::Vec::<(usize, u8)>::new();
        let mut next_serial = 0usize;

        for op in program.0.iter().take(MAX_BOX_OPS) {
            let checkpoint = drops.borrow().len();
            let mut expected_drops = std::vec::Vec::new();

            match op {
                BoxOp::Push(payload) => {
                    actual.push(Box::new_in(
                        DropSpy {
                            serial: next_serial,
                            payload: *payload,
                            drops: drops.clone(),
                        },
                        &bump,
                    ));
                    expected.push((next_serial, *payload));
                    next_serial += 1;
                }
                BoxOp::Pop => {
                    let actual_payload = actual.pop().map(|boxed| {
                        let payload = boxed.payload;
                        drop(boxed);
                        payload
                    });
                    let expected_payload = expected.pop().map(|(serial, payload)| {
                        expected_drops.push(serial);
                        payload
                    });
                    assert_eq!(actual_payload, expected_payload);
                }
                BoxOp::Remove(index) => {
                    let actual_payload = if actual.is_empty() {
                        None
                    } else {
                        let index = usize::from(*index) % actual.len();
                        Some({
                            let boxed = actual.remove(index);
                            let payload = boxed.payload;
                            drop(boxed);
                            payload
                        })
                    };
                    let expected_payload = if expected.is_empty() {
                        None
                    } else {
                        let index = usize::from(*index) % expected.len();
                        let (serial, payload) = expected.remove(index);
                        expected_drops.push(serial);
                        Some(payload)
                    };
                    assert_eq!(actual_payload, expected_payload);
                }
                BoxOp::Truncate(len) => {
                    let len = usize::from(*len) % (MAX_BOX_LEN + 1);
                    if len < expected.len() {
                        expected_drops.extend(expected[len..].iter().map(|(serial, _)| *serial));
                    }
                    expected.truncate(len);
                    actual.truncate(len);
                }
                BoxOp::Clear => {
                    expected_drops.extend(expected.iter().map(|(serial, _)| *serial));
                    expected.clear();
                    actual.clear();
                }
            }

            let actual_drops = drop_ids_since(&drops, checkpoint);
            expected_drops.sort_unstable();
            let mut actual_drops = actual_drops;
            actual_drops.sort_unstable();
            assert_eq!(actual_drops, expected_drops);
            assert_eq!(
                live_payloads(&actual),
                expected
                    .iter()
                    .map(|(_, payload)| *payload)
                    .collect::<std::vec::Vec<_>>(),
            );
        }

        while !actual.is_empty() {
            let checkpoint = drops.borrow().len();
            let actual_payload = actual.pop().map(|boxed| {
                let payload = boxed.payload;
                drop(boxed);
                payload
            });
            let (serial, expected_payload) = expected.pop().unwrap();
            assert_eq!(actual_payload, Some(expected_payload));
            assert_eq!(drop_ids_since(&drops, checkpoint), vec![serial]);
        }

        assert!(expected.is_empty());
        assert_eq!(drops.borrow().len(), next_serial);
        Ok(())
    })
}
