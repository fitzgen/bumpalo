use super::BumpAllocSafe;

impl<'a, T: ?Sized> BumpAllocSafe for &'a T {}
impl<'a, T: ?Sized> BumpAllocSafe for &'a mut T {}

macro_rules! impl_bump_alloc_safe {
    ( $( $t:ty ),* $(,)* ) => {
        $(
            impl BumpAllocSafe for $t {}
        )*
    }
}

impl_bump_alloc_safe! {
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    bool,
}

macro_rules! impl_generic_bump_alloc_safe_without_bounds {
    (
        $( $( $t:ident ),* => $u:ty ),* $(,)*
    ) => {
        $( impl<$( $t ),*> BumpAllocSafe for $u {} )*
    }
}

macro_rules! impl_generic_bump_alloc_safe_with_bounds {
    (
        $( $( $t:ident ),* => $u:ty ),* $(,)*
    ) => {
        $(
            impl<$( $t : BumpAllocSafe ),*> BumpAllocSafe for $u {}
        )*
    }
}

impl_generic_bump_alloc_safe_without_bounds! {
    T => *const T,
    T => *mut T,
}

impl_generic_bump_alloc_safe_with_bounds! {
    A, B => (A, B),
    A, B, C => (A, B, C),
    A, B, C, D => (A, B, C, D),
    A, B, C, D, E => (A, B, C, D, E),
    A, B, C, D, E, F => (A, B, C, D, E, F),
    A, B, C, D, E, F, G => (A, B, C, D, E, F, G),
    A, B, C, D, E, F, G, H => (A, B, C, D, E, F, G, H),
    A, B, C, D, E, F, G, H, I => (A, B, C, D, E, F, G, H, I),
    A, B, C, D, E, F, G, H, I, J => (A, B, C, D, E, F, G, H, I, J),
}

macro_rules! impl_bump_alloc_safe_array {
    ( $t:ident => { $( $n:expr ),* $(,)* } ) => {
        $(
            impl<$t> BumpAllocSafe for [$t; $n] {}
        )*
    }
}

impl_bump_alloc_safe_array! {
    T => {
        0, 1, 2, 3, 4, 5, 6, 7, 8,
        9, 10, 11, 12, 13, 14, 15,
        16, 17, 18, 19, 20, 21, 22, 23,
        24, 25, 26, 27, 28, 29, 30, 31,
        32,
    }
}
