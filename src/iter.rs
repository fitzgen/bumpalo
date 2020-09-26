//! Additional `Bump`-related functionality for iterators.

use crate::Bump;

/// `Iterator` extensions for `Bump`-allocated types.
pub trait BumpIterator: Iterator {
    /// Transforms an iterator into a `Bump`-allocated collection.
    fn collect_in<'bump, C>(self, bump: &'bump Bump) -> C
    where
        Self: Sized,
        C: FromIteratorIn<'bump, Self::Item>,
    {
        C::from_iter_in(self, bump)
    }
}

impl<I: Iterator> BumpIterator for I {}

/// Conversion from an iterator to a `Bump`-allocated type.
/// This trait is an adapted version of `FromIterator`.
///
/// By implementing `FromIteratorIn` for a type, you define how it will be
/// created from an iterator. This is common for types which describe a
/// collection of some kind.
///
/// `FromIteratorIn`'s `from_iter_in` is rarely called explicitly, and is
/// instead used through `BumpIterator`'s `collect_in` method.
pub trait FromIteratorIn<'bump, T> {
    /// Creates a value from an iterator.
    /// This method is an adapted version of `FromIterator::from_iter`.
    fn from_iter_in<I>(iter: I, bump: &'bump Bump) -> Self
    where
        I: IntoIterator<Item = T>;
}

impl<'bump, T, C> FromIteratorIn<'bump, Option<T>> for Option<C>
where
    C: FromIteratorIn<'bump, T>,
{
    /// Takes each element in the `Iterator`: if it is `None`, no further
    /// elements are taken, and the `None` is returned. Should no `None` occur,
    /// a container with the values of each `Option` is returned.
    fn from_iter_in<I>(iter: I, bump: &'bump Bump) -> Self
    where
        I: IntoIterator<Item = Option<T>>,
    {
        let mut none = false;

        let c = iter
            .into_iter()
            .scan((), |(), option| {
                if option.is_none() {
                    none = true;
                }
                option
            })
            .collect_in(bump);

        if none {
            None
        } else {
            Some(c)
        }
    }
}

impl<'bump, T, E, C> FromIteratorIn<'bump, Result<T, E>> for Result<C, E>
where
    C: FromIteratorIn<'bump, T>,
{
    /// Takes each element in the `Iterator`: if it is an `Err`, no further
    /// elements are taken, and the `Err` is returned. Should no `Err` occur, a
    /// container with the values of each `Result` is returned.
    fn from_iter_in<I>(iter: I, bump: &'bump Bump) -> Self
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        let mut error = None;

        let c = iter
            .into_iter()
            .scan((), |(), result| match result {
                Ok(x) => Some(x),
                Err(e) => {
                    error = Some(e);
                    None
                }
            })
            .collect_in(bump);

        match error {
            None => Ok(c),
            Some(e) => Err(e),
        }
    }
}
